#include "dvda-formats.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#include <wchar.h>
#endif

enum {
    DVDA_FORMATS_INVALID = -1,
    DVDA_FORMATS_IO = -2,
    DVDA_FORMATS_ALLOC = -3,
    DVDA_FORMATS_ARGUMENT = -4
};

static uint16_t read_be16(const uint8_t *data)
{
    return (uint16_t)(((uint16_t)data[0] << 8) | data[1]);
}

static void write_be16(uint8_t *data, uint16_t value)
{
    data[0] = (uint8_t)(value >> 8);
    data[1] = (uint8_t)value;
}

static int sample_rate_of(const uint8_t *major_sync, size_t size, int32_t *value)
{
    uint8_t rate_bits;
    if (major_sync == NULL || value == NULL || size <= 5) return DVDA_FORMATS_ARGUMENT;
    rate_bits = (uint8_t)((major_sync[5] >> 4) & 0x0fU);
    if (rate_bits == 0x0fU) return DVDA_FORMATS_INVALID;
    *value = ((rate_bits & 8U) != 0U ? 44100 : 48000) << (rate_bits & 7U);
    return 0;
}

static int32_t peak_bitrate_raw(int32_t sample_rate)
{
    const int64_t numerator = (int64_t)9600000 * 16 - 8;
    if (sample_rate <= 0) return 0;
    return (int32_t)((numerator + sample_rate - 1) / sample_rate);
}

static uint32_t crc_av(uint32_t polynomial, int bits, uint32_t initial,
                       const uint8_t *data, size_t count)
{
    uint32_t table[256];
    size_t index;
    int bit;
    uint32_t crc = initial;
    for (index = 0; index < 256; ++index) {
        uint32_t value = (uint32_t)index << 24;
        for (bit = 0; bit < 8; ++bit)
            value = (value << 1) ^ ((value >> 31) != 0U ? polynomial << (32 - bits) : 0U);
        table[index] = ((value & 0x000000ffU) << 24) |
                       ((value & 0x0000ff00U) << 8) |
                       ((value & 0x00ff0000U) >> 8) |
                       ((value & 0xff000000U) >> 24);
    }
    for (index = 0; index < count; ++index)
        crc = table[(crc & 0xffU) ^ data[index]] ^ (crc >> 8);
    return crc;
}

static uint16_t checksum16_value(const uint8_t *data, size_t size)
{
    uint32_t crc = crc_av(0x002dU, 16, 0U, data, size - 2);
    crc ^= (uint16_t)(data[size - 2] | ((uint16_t)data[size - 1] << 8));
    /* The wire stores this field little endian, while the C# helper reads it
       as a little-endian value and returns the numeric CRC. */
    return (uint16_t)crc;
}

static uint8_t checksum8_value(const uint8_t *data, size_t size)
{
    uint32_t crc = crc_av(0x0063U, 8, 0x3cU, data, size - 1);
    crc ^= data[size - 1];
    return (uint8_t)crc;
}

static uint8_t parity_value(const uint8_t *data, size_t size)
{
    size_t index;
    unsigned int parity = 0;
    for (index = 0; index < size; ++index) parity ^= data[index];
    parity ^= parity >> 16;
    parity ^= parity >> 8;
    return (uint8_t)parity;
}

static int ends_with_eos(const uint8_t *data, size_t size)
{
    static const uint8_t eos[] = { 0xd2, 0x34, 0xd2, 0x34 };
    return size >= sizeof(eos) && memcmp(data + size - sizeof(eos), eos, sizeof(eos)) == 0;
}

typedef struct inspection_state {
    uint64_t size;
    uint32_t units;
    uint32_t majors;
    int32_t major_errors;
    int32_t parity_errors;
    int32_t substream_errors;
    int32_t peak;
    int32_t extended;
    int32_t sample_rate;
    int32_t has_peak;
    int32_t has_extended;
    int32_t has_sample_rate;
    int32_t last_eos;
} inspection_state;

static void inspection_state_init(inspection_state *state)
{
    memset(state, 0, sizeof(*state));
    state->peak = -1;
    state->extended = -1;
    state->sample_rate = -1;
}

static int inspect_unit(inspection_state *state, const uint8_t *unit, size_t length)
{
    const uint8_t signature[] = { 0xf8, 0x72, 0x6f };
    int has_major = length >= 7 && memcmp(unit + 4, signature, sizeof(signature)) == 0;
    size_t header_offset;
    uint16_t substream_header;
    size_t end;
    size_t data_offset;
    const uint8_t *substream_data;
    uint16_t timing;
    unsigned int parity;
    unsigned int expected;
    unsigned int actual;
    int32_t current_sample_rate;
    int32_t current_peak;
    int32_t current_extended;

    state->last_eos = 0;
    if (has_major) {
        const uint8_t *major;
        state->majors++;
        if (length < 4 + 28) {
            state->major_errors++;
        } else {
            major = unit + 4;
            if (read_be16(major + 8) != 0xb752U) state->major_errors++;
            if (checksum16_value(major, 26) != (uint16_t)(major[26] | ((uint16_t)major[27] << 8)))
                state->major_errors++;
            current_peak = (int32_t)(read_be16(major + 14) & 0x7fffU);
            current_extended = (int32_t)(major[16] & 3U);
            if (sample_rate_of(major, 28, &current_sample_rate) != 0) {
                state->major_errors++;
            } else {
                if ((int64_t)current_peak < ((int64_t)9600000 * 16) / current_sample_rate ||
                    current_peak > peak_bitrate_raw(current_sample_rate))
                    state->major_errors++;
                if (state->has_sample_rate && current_sample_rate != state->sample_rate)
                    state->major_errors++;
                state->sample_rate = current_sample_rate;
                state->has_sample_rate = 1;
            }
            if (current_extended != 1) state->major_errors++;
            if (state->has_peak && current_peak != state->peak) state->major_errors++;
            if (state->has_extended && current_extended != state->extended) state->major_errors++;
            state->peak = current_peak;
            state->extended = current_extended;
            state->has_peak = 1;
            state->has_extended = 1;
        }
    }

    header_offset = 4 + (has_major ? 28 : 0);
    if (header_offset + 2 > length) {
        state->substream_errors++;
        return 0;
    }
    substream_header = read_be16(unit + header_offset);
    end = (size_t)(substream_header & 0x0fffU) * 2U;
    data_offset = header_offset + 2;
    if (end < 2 || data_offset > length || end > length - data_offset) {
        state->substream_errors++;
        return 0;
    }
    substream_data = unit + data_offset;
    timing = read_be16(unit + 2);
    parity = (unsigned int)timing ^ (unsigned int)(length / 2U);
    parity ^= (unsigned int)((substream_header >> 8) & 0xffU);
    parity ^= (unsigned int)(substream_header & 0xffU);
    parity ^= parity >> 8;
    parity ^= parity >> 4;
    parity &= 0x0fU;
    expected = (unsigned int)(unit[0] >> 4);
    actual = parity ^ 0x0fU;
    if (actual != expected) state->parity_errors++;
    if ((substream_header & 0x2000U) != 0U) {
        size_t body_length;
        if (end < 2) {
            state->substream_errors++;
            return 0;
        }
        body_length = end - 2;
        if (body_length < 1 ||
            parity_value(substream_data, body_length) != (uint8_t)(substream_data[body_length] ^ 0xa9U) ||
            checksum8_value(substream_data, body_length) != substream_data[end - 1])
            state->substream_errors++;
        state->last_eos = body_length >= 4 && ends_with_eos(substream_data, body_length);
    } else {
        state->last_eos = end >= 6 && ends_with_eos(substream_data, end - 2);
    }
    return 0;
}

static void inspection_complete(const inspection_state *state, dvda_mlp_inspection *inspection)
{
    memset(inspection, 0, sizeof(*inspection));
    inspection->size = state->size;
    inspection->access_unit_count = state->units;
    inspection->major_sync_count = state->majors;
    inspection->major_sync_interval = state->majors == 0 ? 0.0 : (double)state->units / (double)state->majors;
    inspection->major_sync_error_count = state->major_errors;
    inspection->access_unit_parity_error_count = state->parity_errors;
    inspection->substream_error_count = state->substream_errors;
    inspection->has_end_of_stream = state->last_eos;
    inspection->peak_bitrate_raw = state->has_peak ? state->peak : -1;
    inspection->extended_substream_info = state->has_extended ? state->extended : -1;
    inspection->sample_rate = state->has_sample_rate ? state->sample_rate : -1;
    inspection->is_valid = state->units > 0 && state->majors > 0 && state->has_sample_rate &&
        state->has_peak && state->has_extended && state->last_eos &&
        state->major_errors == 0 && state->parity_errors == 0 && state->substream_errors == 0 &&
        state->peak >= ((int64_t)9600000 * 16) / state->sample_rate &&
        state->peak <= peak_bitrate_raw(state->sample_rate) && state->extended == 1;
}

static int inspect_buffer_internal(const uint8_t *data, size_t size, dvda_mlp_inspection *inspection)
{
    inspection_state state;
    size_t position = 0;
    if (data == NULL || inspection == NULL) return DVDA_FORMATS_ARGUMENT;
    inspection_state_init(&state);
    while (position < size) {
        uint16_t header;
        size_t length;
        if (size - position < 4) return DVDA_FORMATS_INVALID;
        header = read_be16(data + position);
        length = (size_t)(header & 0x0fffU) * 2U;
        if (length < 4 || length > size - position) return DVDA_FORMATS_INVALID;
        inspect_unit(&state, data + position, length);
        state.units++;
        state.size += length;
        position += length;
    }
    inspection_complete(&state, inspection);
    return 0;
}

#ifdef _WIN32
static FILE *open_utf8(const char *path, const wchar_t *mode)
{
    int length;
    wchar_t *wide;
    FILE *file;
    if (path == NULL || mode == NULL) return NULL;
    length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    if (length <= 0) return NULL;
    wide = (wchar_t *)malloc((size_t)length * sizeof(*wide));
    if (wide == NULL) return NULL;
    if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, length) <= 0) {
        free(wide);
        return NULL;
    }
    file = _wfopen(wide, mode);
    free(wide);
    return file;
}
#else
static FILE *open_utf8(const char *path, const char *mode)
{
    return fopen(path, mode);
}
#endif

DVDA_FORMATS_API int dvda_formats_mlp_inspect_buffer(
    const uint8_t *data, size_t size, dvda_mlp_inspection *inspection)
{
    return inspect_buffer_internal(data, size, inspection);
}

DVDA_FORMATS_API int dvda_formats_mlp_inspect_file(
    const char *utf8_path, dvda_mlp_inspection *inspection)
{
    inspection_state state;
    FILE *file;
    uint8_t header[4];
    if (utf8_path == NULL || inspection == NULL) return DVDA_FORMATS_ARGUMENT;
#ifdef _WIN32
    file = open_utf8(utf8_path, L"rb");
#else
    file = open_utf8(utf8_path, "rb");
#endif
    if (file == NULL) return DVDA_FORMATS_IO;
    inspection_state_init(&state);
    for (;;) {
        size_t read_count = fread(header, 1, sizeof(header), file);
        uint16_t word;
        size_t length;
        uint8_t *unit;
        if (read_count == 0) {
            if (ferror(file)) { fclose(file); return DVDA_FORMATS_IO; }
            break;
        }
        if (read_count != sizeof(header)) { fclose(file); return DVDA_FORMATS_INVALID; }
        word = read_be16(header);
        length = (size_t)(word & 0x0fffU) * 2U;
        if (length < 4) { fclose(file); return DVDA_FORMATS_INVALID; }
        unit = (uint8_t *)malloc(length);
        if (unit == NULL) { fclose(file); return DVDA_FORMATS_ALLOC; }
        memcpy(unit, header, sizeof(header));
        if (fread(unit + sizeof(header), 1, length - sizeof(header), file) != length - sizeof(header)) {
            free(unit); fclose(file); return ferror(file) ? DVDA_FORMATS_IO : DVDA_FORMATS_INVALID;
        }
        inspect_unit(&state, unit, length);
        free(unit);
        state.units++;
        state.size += length;
    }
    fclose(file);
    inspection_complete(&state, inspection);
    return 0;
}

DVDA_FORMATS_API int dvda_formats_parse_pts(
    const uint8_t *data, size_t size, int64_t *value)
{
    uint64_t high;
    uint64_t middle;
    uint64_t low;
    if (data == NULL || value == NULL || size < 5) return DVDA_FORMATS_ARGUMENT;
    high = ((uint64_t)(data[0] >> 1)) & 7U;
    middle = (((uint64_t)data[1] << 8) | data[2]) >> 1;
    low = (((uint64_t)data[3] << 8) | data[4]) >> 1;
    *value = (int64_t)((high << 30) | (middle << 15) | low);
    return 0;
}

DVDA_FORMATS_API int dvda_formats_sample_rate(
    const uint8_t *major_sync, size_t size, int32_t *value)
{
    return sample_rate_of(major_sync, size, value);
}

DVDA_FORMATS_API int32_t dvda_formats_peak_bitrate_raw(int32_t sample_rate)
{
    return peak_bitrate_raw(sample_rate);
}

DVDA_FORMATS_API int dvda_formats_checksum16(
    const uint8_t *data, size_t size, uint16_t *value)
{
    if (data == NULL || value == NULL || size < 2) return DVDA_FORMATS_ARGUMENT;
    *value = checksum16_value(data, size);
    return 0;
}

DVDA_FORMATS_API int dvda_formats_checksum8(
    const uint8_t *data, size_t size, uint8_t *value)
{
    if (data == NULL || value == NULL || size < 1) return DVDA_FORMATS_ARGUMENT;
    *value = checksum8_value(data, size);
    return 0;
}

DVDA_FORMATS_API uint8_t dvda_formats_calculate_parity(
    const uint8_t *data, size_t size)
{
    return data == NULL ? 0 : parity_value(data, size);
}

typedef struct mlp_unit_info {
    size_t offset;
    size_t length;
    int has_major;
} mlp_unit_info;

static int collect_units(const uint8_t *data, size_t size,
                         mlp_unit_info **units, size_t *unit_count)
{
    const uint8_t signature[] = { 0xf8, 0x72, 0x6f };
    size_t position = 0;
    size_t count = 0;
    size_t capacity = 0;
    mlp_unit_info *items = NULL;
    if (data == NULL || units == NULL || unit_count == NULL) return DVDA_FORMATS_ARGUMENT;
    while (position < size) {
        uint16_t header;
        size_t length;
        mlp_unit_info *grown;
        if (size - position < 4) { free(items); return DVDA_FORMATS_INVALID; }
        header = read_be16(data + position);
        length = (size_t)(header & 0x0fffU) * 2U;
        if (length < 4 || length > size - position) { free(items); return DVDA_FORMATS_INVALID; }
        if (count == capacity) {
            capacity = capacity == 0 ? 16 : capacity * 2;
            grown = (mlp_unit_info *)realloc(items, capacity * sizeof(*items));
            if (grown == NULL) { free(items); return DVDA_FORMATS_ALLOC; }
            items = grown;
        }
        items[count].offset = position;
        items[count].length = length;
        items[count].has_major = length >= 7 && memcmp(data + position + 4, signature, sizeof(signature)) == 0;
        count++;
        position += length;
    }
    *units = items;
    *unit_count = count;
    return 0;
}

DVDA_FORMATS_API int dvda_formats_mlp_align_buffer(
    const uint8_t *data, size_t size, uint8_t **aligned_data,
    size_t *aligned_size, dvda_mlp_alignment *alignment)
{
    mlp_unit_info *units = NULL;
    size_t unit_count = 0;
    uint8_t *buffer;
    size_t index;
    int result;
    if (aligned_data == NULL || aligned_size == NULL || alignment == NULL || data == NULL)
        return DVDA_FORMATS_ARGUMENT;
    memset(alignment, 0, sizeof(*alignment));
    alignment->old_header = -1;
    alignment->new_header = -1;
    result = collect_units(data, size, &units, &unit_count);
    if (result != 0) return result;
    if (unit_count == 0) { free(units); return DVDA_FORMATS_INVALID; }
    buffer = (uint8_t *)malloc(size == 0 ? 1 : size);
    if (buffer == NULL) { free(units); return DVDA_FORMATS_ALLOC; }
    if (size > 0) memcpy(buffer, data, size);
    for (index = 0; index < unit_count; ++index) {
        size_t major_offset;
        uint8_t *major;
        int32_t sample_rate;
        int32_t wanted_peak;
        uint16_t peak_value;
        uint16_t checksum;
        if (!units[index].has_major) continue;
        major_offset = units[index].offset + 4;
        if (major_offset + 28 > size) { free(units); free(buffer); return DVDA_FORMATS_INVALID; }
        major = buffer + major_offset;
        if (sample_rate_of(major, 28, &sample_rate) != 0) { free(units); free(buffer); return DVDA_FORMATS_INVALID; }
        wanted_peak = peak_bitrate_raw(sample_rate);
        peak_value = read_be16(major + 14);
        if ((peak_value & 0x7fffU) != (uint16_t)wanted_peak) {
            write_be16(major + 14, (uint16_t)((peak_value & 0x8000U) | ((uint16_t)wanted_peak & 0x7fffU)));
            alignment->peak_changes++;
        }
        if ((major[16] & 3U) != 1U) {
            major[16] = (uint8_t)((major[16] & 0xfcU) | 1U);
            alignment->extended_changes++;
        }
        checksum = checksum16_value(major, 26);
        if ((uint16_t)(major[26] | ((uint16_t)major[27] << 8)) != checksum) {
            major[26] = (uint8_t)checksum;
            major[27] = (uint8_t)(checksum >> 8);
            alignment->checksum_changes++;
        }
    }
    {
        mlp_unit_info last = units[unit_count - 1];
        size_t substream_header_offset = last.offset + 4 + (last.has_major ? 28 : 0);
        uint16_t substream_header;
        size_t end;
        size_t data_offset;
        size_t body_length;
        if (substream_header_offset + 2 > size) { free(units); free(buffer); return DVDA_FORMATS_INVALID; }
        substream_header = read_be16(buffer + substream_header_offset);
        end = (size_t)(substream_header & 0x0fffU) * 2U;
        data_offset = substream_header_offset + 2;
        if (end < 2 || data_offset > size || end > size - data_offset) { free(units); free(buffer); return DVDA_FORMATS_INVALID; }
        body_length = end - 2;
        if (!ends_with_eos(buffer + data_offset, body_length)) {
            uint8_t *grown;
            size_t insertion_offset = data_offset + body_length;
            size_t new_size;
            size_t new_body_length = body_length + 4;
            uint16_t timing;
            uint16_t new_length_words;
            unsigned int parity_nibble;
            uint16_t old_header;
            if (size > SIZE_MAX - 4) { free(units); free(buffer); return DVDA_FORMATS_ALLOC; }
            new_size = size + 4;
            grown = (uint8_t *)malloc(new_size);
            if (grown == NULL) { free(units); free(buffer); return DVDA_FORMATS_ALLOC; }
            if (insertion_offset > 0) memcpy(grown, buffer, insertion_offset);
            grown[insertion_offset + 0] = 0xd2;
            grown[insertion_offset + 1] = 0x34;
            grown[insertion_offset + 2] = 0xd2;
            grown[insertion_offset + 3] = 0x34;
            if (size > insertion_offset + 2)
                memcpy(grown + insertion_offset + 6, buffer + insertion_offset + 2,
                       size - insertion_offset - 2);
            grown[data_offset + new_body_length] = (uint8_t)(parity_value(grown + data_offset, new_body_length) ^ 0xa9U);
            grown[data_offset + new_body_length + 1] = checksum8_value(grown + data_offset, new_body_length + 1);
            substream_header = (uint16_t)((substream_header & 0xf000U) |
                (((substream_header & 0x0fffU) + 2U) & 0x0fffU));
            write_be16(grown + substream_header_offset, substream_header);
            timing = read_be16(grown + last.offset + 2);
            new_length_words = (uint16_t)(last.length / 2 + 2);
            parity_nibble = (unsigned int)timing ^ (unsigned int)new_length_words;
            parity_nibble ^= (unsigned int)((substream_header >> 8) & 0xffU);
            parity_nibble ^= (unsigned int)(substream_header & 0xffU);
            parity_nibble ^= parity_nibble >> 8;
            parity_nibble ^= parity_nibble >> 4;
            parity_nibble &= 0x0fU;
            old_header = read_be16(grown + last.offset);
            write_be16(grown + last.offset, (uint16_t)(((parity_nibble ^ 0x0fU) << 12) | (new_length_words & 0x0fffU)));
            alignment->inserted_end_of_stream = 1;
            alignment->old_header = (int32_t)old_header;
            alignment->new_header = (int32_t)read_be16(grown + last.offset);
            free(buffer);
            buffer = grown;
            size = new_size;
        }
    }
    free(units);
    *aligned_data = buffer;
    *aligned_size = size;
    return 0;
}

DVDA_FORMATS_API int dvda_formats_pcm_compare_files(
    const char *utf8_source_path, const char *utf8_decoded_path,
    uint32_t bytes_per_sample_frame, uint32_t max_trailing_zero_frames,
    dvda_pcm_comparison *comparison)
{
    FILE *source;
    FILE *decoded;
    int64_t source_size;
    int64_t decoded_size;
    int64_t extra;
    int padding_allowed;
    uint8_t left[128 * 1024];
    uint8_t right[128 * 1024];
    uint64_t offset = 0;
    if (comparison == NULL || utf8_source_path == NULL || utf8_decoded_path == NULL ||
        (max_trailing_zero_frames > 0 && bytes_per_sample_frame == 0)) return DVDA_FORMATS_ARGUMENT;
    memset(comparison, 0, sizeof(*comparison));
#ifdef _WIN32
    source = open_utf8(utf8_source_path, L"rb");
    decoded = open_utf8(utf8_decoded_path, L"rb");
#else
    source = open_utf8(utf8_source_path, "rb");
    decoded = open_utf8(utf8_decoded_path, "rb");
#endif
    if (source == NULL || decoded == NULL) {
        if (source != NULL) fclose(source);
        if (decoded != NULL) fclose(decoded);
        return DVDA_FORMATS_IO;
    }
    if (_fseeki64(source, 0, SEEK_END) != 0 || (source_size = _ftelli64(source)) < 0 ||
        _fseeki64(decoded, 0, SEEK_END) != 0 || (decoded_size = _ftelli64(decoded)) < 0 ||
        _fseeki64(source, 0, SEEK_SET) != 0 || _fseeki64(decoded, 0, SEEK_SET) != 0) {
        fclose(source); fclose(decoded); return DVDA_FORMATS_IO;
    }
    comparison->source_bytes = (uint64_t)source_size;
    comparison->decoded_bytes = (uint64_t)decoded_size;
    extra = decoded_size - source_size;
    padding_allowed = max_trailing_zero_frames > 0 && extra > 0 &&
        source_size % bytes_per_sample_frame == 0 && extra % bytes_per_sample_frame == 0 &&
        extra <= (int64_t)bytes_per_sample_frame * max_trailing_zero_frames;
    if (extra != 0 && !padding_allowed) {
        comparison->reason_code = 1;
        fclose(source); fclose(decoded); return 0;
    }
    while (offset < (uint64_t)source_size) {
        size_t wanted = (size_t)((uint64_t)sizeof(left) < (uint64_t)source_size - offset ?
            sizeof(left) : (uint64_t)source_size - offset);
        size_t left_read = fread(left, 1, wanted, source);
        size_t right_read = fread(right, 1, wanted, decoded);
        size_t index;
        if (left_read != wanted || right_read != wanted) {
            comparison->reason_code = 4;
            comparison->first_mismatch_offset = offset;
            fclose(source); fclose(decoded); return 0;
        }
        for (index = 0; index < wanted; ++index) {
            if (left[index] != right[index]) {
                comparison->reason_code = 2;
                comparison->first_mismatch_offset = offset + index;
                fclose(source); fclose(decoded); return 0;
            }
        }
        offset += wanted;
    }
    if (padding_allowed) {
        uint64_t remaining = (uint64_t)extra;
        while (remaining > 0) {
            size_t wanted = remaining < sizeof(right) ? (size_t)remaining : sizeof(right);
            size_t read_count = fread(right, 1, wanted, decoded);
            size_t index;
            if (read_count != wanted) {
                comparison->reason_code = 4;
                comparison->first_mismatch_offset = (uint64_t)source_size + ((uint64_t)extra - remaining);
                fclose(source); fclose(decoded); return 0;
            }
            for (index = 0; index < wanted; ++index) {
                if (right[index] != 0) {
                    comparison->reason_code = 3;
                    comparison->first_mismatch_offset = (uint64_t)source_size + ((uint64_t)extra - remaining) + index;
                    fclose(source); fclose(decoded); return 0;
                }
            }
            remaining -= wanted;
        }
        comparison->trailing_zero_bytes = (uint64_t)extra;
    }
    comparison->match = 1;
    fclose(source); fclose(decoded);
    return 0;
}

DVDA_FORMATS_API void dvda_formats_free(void *pointer)
{
    free(pointer);
}
