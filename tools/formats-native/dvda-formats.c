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

/* AV-style CRC-16, polynomial 0x2d. Immutable tables keep
   checksum calls reentrant and avoid rebuilding 256 entries per access unit. */
static const uint16_t crc16_table[256] = {
    0x0000U, 0x2d00U, 0x5a00U, 0x7700U, 0xb400U, 0x9900U, 0xee00U, 0xc300U,
    0x6801U, 0x4501U, 0x3201U, 0x1f01U, 0xdc01U, 0xf101U, 0x8601U, 0xab01U,
    0xd002U, 0xfd02U, 0x8a02U, 0xa702U, 0x6402U, 0x4902U, 0x3e02U, 0x1302U,
    0xb803U, 0x9503U, 0xe203U, 0xcf03U, 0x0c03U, 0x2103U, 0x5603U, 0x7b03U,
    0xa005U, 0x8d05U, 0xfa05U, 0xd705U, 0x1405U, 0x3905U, 0x4e05U, 0x6305U,
    0xc804U, 0xe504U, 0x9204U, 0xbf04U, 0x7c04U, 0x5104U, 0x2604U, 0x0b04U,
    0x7007U, 0x5d07U, 0x2a07U, 0x0707U, 0xc407U, 0xe907U, 0x9e07U, 0xb307U,
    0x1806U, 0x3506U, 0x4206U, 0x6f06U, 0xac06U, 0x8106U, 0xf606U, 0xdb06U,
    0x400bU, 0x6d0bU, 0x1a0bU, 0x370bU, 0xf40bU, 0xd90bU, 0xae0bU, 0x830bU,
    0x280aU, 0x050aU, 0x720aU, 0x5f0aU, 0x9c0aU, 0xb10aU, 0xc60aU, 0xeb0aU,
    0x9009U, 0xbd09U, 0xca09U, 0xe709U, 0x2409U, 0x0909U, 0x7e09U, 0x5309U,
    0xf808U, 0xd508U, 0xa208U, 0x8f08U, 0x4c08U, 0x6108U, 0x1608U, 0x3b08U,
    0xe00eU, 0xcd0eU, 0xba0eU, 0x970eU, 0x540eU, 0x790eU, 0x0e0eU, 0x230eU,
    0x880fU, 0xa50fU, 0xd20fU, 0xff0fU, 0x3c0fU, 0x110fU, 0x660fU, 0x4b0fU,
    0x300cU, 0x1d0cU, 0x6a0cU, 0x470cU, 0x840cU, 0xa90cU, 0xde0cU, 0xf30cU,
    0x580dU, 0x750dU, 0x020dU, 0x2f0dU, 0xec0dU, 0xc10dU, 0xb60dU, 0x9b0dU,
    0x8016U, 0xad16U, 0xda16U, 0xf716U, 0x3416U, 0x1916U, 0x6e16U, 0x4316U,
    0xe817U, 0xc517U, 0xb217U, 0x9f17U, 0x5c17U, 0x7117U, 0x0617U, 0x2b17U,
    0x5014U, 0x7d14U, 0x0a14U, 0x2714U, 0xe414U, 0xc914U, 0xbe14U, 0x9314U,
    0x3815U, 0x1515U, 0x6215U, 0x4f15U, 0x8c15U, 0xa115U, 0xd615U, 0xfb15U,
    0x2013U, 0x0d13U, 0x7a13U, 0x5713U, 0x9413U, 0xb913U, 0xce13U, 0xe313U,
    0x4812U, 0x6512U, 0x1212U, 0x3f12U, 0xfc12U, 0xd112U, 0xa612U, 0x8b12U,
    0xf011U, 0xdd11U, 0xaa11U, 0x8711U, 0x4411U, 0x6911U, 0x1e11U, 0x3311U,
    0x9810U, 0xb510U, 0xc210U, 0xef10U, 0x2c10U, 0x0110U, 0x7610U, 0x5b10U,
    0xc01dU, 0xed1dU, 0x9a1dU, 0xb71dU, 0x741dU, 0x591dU, 0x2e1dU, 0x031dU,
    0xa81cU, 0x851cU, 0xf21cU, 0xdf1cU, 0x1c1cU, 0x311cU, 0x461cU, 0x6b1cU,
    0x101fU, 0x3d1fU, 0x4a1fU, 0x671fU, 0xa41fU, 0x891fU, 0xfe1fU, 0xd31fU,
    0x781eU, 0x551eU, 0x221eU, 0x0f1eU, 0xcc1eU, 0xe11eU, 0x961eU, 0xbb1eU,
    0x6018U, 0x4d18U, 0x3a18U, 0x1718U, 0xd418U, 0xf918U, 0x8e18U, 0xa318U,
    0x0819U, 0x2519U, 0x5219U, 0x7f19U, 0xbc19U, 0x9119U, 0xe619U, 0xcb19U,
    0xb01aU, 0x9d1aU, 0xea1aU, 0xc71aU, 0x041aU, 0x291aU, 0x5e1aU, 0x731aU,
    0xd81bU, 0xf51bU, 0x821bU, 0xaf1bU, 0x6c1bU, 0x411bU, 0x361bU, 0x1b1bU,
};

/* AV-style CRC-8, polynomial 0x63. Immutable tables keep
   checksum calls reentrant and avoid rebuilding 256 entries per access unit. */
static const uint8_t crc8_table[256] = {
    0x00U, 0x63U, 0xc6U, 0xa5U, 0xefU, 0x8cU, 0x29U, 0x4aU,
    0xbdU, 0xdeU, 0x7bU, 0x18U, 0x52U, 0x31U, 0x94U, 0xf7U,
    0x19U, 0x7aU, 0xdfU, 0xbcU, 0xf6U, 0x95U, 0x30U, 0x53U,
    0xa4U, 0xc7U, 0x62U, 0x01U, 0x4bU, 0x28U, 0x8dU, 0xeeU,
    0x32U, 0x51U, 0xf4U, 0x97U, 0xddU, 0xbeU, 0x1bU, 0x78U,
    0x8fU, 0xecU, 0x49U, 0x2aU, 0x60U, 0x03U, 0xa6U, 0xc5U,
    0x2bU, 0x48U, 0xedU, 0x8eU, 0xc4U, 0xa7U, 0x02U, 0x61U,
    0x96U, 0xf5U, 0x50U, 0x33U, 0x79U, 0x1aU, 0xbfU, 0xdcU,
    0x64U, 0x07U, 0xa2U, 0xc1U, 0x8bU, 0xe8U, 0x4dU, 0x2eU,
    0xd9U, 0xbaU, 0x1fU, 0x7cU, 0x36U, 0x55U, 0xf0U, 0x93U,
    0x7dU, 0x1eU, 0xbbU, 0xd8U, 0x92U, 0xf1U, 0x54U, 0x37U,
    0xc0U, 0xa3U, 0x06U, 0x65U, 0x2fU, 0x4cU, 0xe9U, 0x8aU,
    0x56U, 0x35U, 0x90U, 0xf3U, 0xb9U, 0xdaU, 0x7fU, 0x1cU,
    0xebU, 0x88U, 0x2dU, 0x4eU, 0x04U, 0x67U, 0xc2U, 0xa1U,
    0x4fU, 0x2cU, 0x89U, 0xeaU, 0xa0U, 0xc3U, 0x66U, 0x05U,
    0xf2U, 0x91U, 0x34U, 0x57U, 0x1dU, 0x7eU, 0xdbU, 0xb8U,
    0xc8U, 0xabU, 0x0eU, 0x6dU, 0x27U, 0x44U, 0xe1U, 0x82U,
    0x75U, 0x16U, 0xb3U, 0xd0U, 0x9aU, 0xf9U, 0x5cU, 0x3fU,
    0xd1U, 0xb2U, 0x17U, 0x74U, 0x3eU, 0x5dU, 0xf8U, 0x9bU,
    0x6cU, 0x0fU, 0xaaU, 0xc9U, 0x83U, 0xe0U, 0x45U, 0x26U,
    0xfaU, 0x99U, 0x3cU, 0x5fU, 0x15U, 0x76U, 0xd3U, 0xb0U,
    0x47U, 0x24U, 0x81U, 0xe2U, 0xa8U, 0xcbU, 0x6eU, 0x0dU,
    0xe3U, 0x80U, 0x25U, 0x46U, 0x0cU, 0x6fU, 0xcaU, 0xa9U,
    0x5eU, 0x3dU, 0x98U, 0xfbU, 0xb1U, 0xd2U, 0x77U, 0x14U,
    0xacU, 0xcfU, 0x6aU, 0x09U, 0x43U, 0x20U, 0x85U, 0xe6U,
    0x11U, 0x72U, 0xd7U, 0xb4U, 0xfeU, 0x9dU, 0x38U, 0x5bU,
    0xb5U, 0xd6U, 0x73U, 0x10U, 0x5aU, 0x39U, 0x9cU, 0xffU,
    0x08U, 0x6bU, 0xceU, 0xadU, 0xe7U, 0x84U, 0x21U, 0x42U,
    0x9eU, 0xfdU, 0x58U, 0x3bU, 0x71U, 0x12U, 0xb7U, 0xd4U,
    0x23U, 0x40U, 0xe5U, 0x86U, 0xccU, 0xafU, 0x0aU, 0x69U,
    0x87U, 0xe4U, 0x41U, 0x22U, 0x68U, 0x0bU, 0xaeU, 0xcdU,
    0x3aU, 0x59U, 0xfcU, 0x9fU, 0xd5U, 0xb6U, 0x13U, 0x70U,
};


static uint16_t checksum16_value(const uint8_t *data, size_t size)
{
    uint32_t crc = 0;
    size_t index;
    for (index = 0; index < size - 2; ++index)
        crc = crc16_table[(crc & 0xffU) ^ data[index]] ^ (crc >> 8);
    crc ^= (uint16_t)(data[size - 2] | ((uint16_t)data[size - 1] << 8));
    /* The wire stores this field little endian; return the numeric CRC value
       used by the Rust ABI. */
    return (uint16_t)crc;
}

static uint8_t checksum8_value(const uint8_t *data, size_t size)
{
    uint8_t crc = 0x3cU;
    size_t index;
    for (index = 0; index < size - 1; ++index)
        crc = crc8_table[crc ^ data[index]];
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
    uint8_t unit[0x0fffU * 2U];
    if (utf8_path == NULL || inspection == NULL) return DVDA_FORMATS_ARGUMENT;
#ifdef _WIN32
    file = open_utf8(utf8_path, L"rb");
#else
    file = open_utf8(utf8_path, "rb");
#endif
    if (file == NULL) return DVDA_FORMATS_IO;
    (void)setvbuf(file, NULL, _IOFBF, 1024U * 1024U);
    inspection_state_init(&state);
    for (;;) {
        size_t read_count = fread(header, 1, sizeof(header), file);
        uint16_t word;
        size_t length;
        if (read_count == 0) {
            if (ferror(file)) { fclose(file); return DVDA_FORMATS_IO; }
            break;
        }
        if (read_count != sizeof(header)) { fclose(file); return DVDA_FORMATS_INVALID; }
        word = read_be16(header);
        length = (size_t)(word & 0x0fffU) * 2U;
        if (length < 4) { fclose(file); return DVDA_FORMATS_INVALID; }
        memcpy(unit, header, sizeof(header));
        if (fread(unit + sizeof(header), 1, length - sizeof(header), file) != length - sizeof(header)) {
            int status = ferror(file) ? DVDA_FORMATS_IO : DVDA_FORMATS_INVALID;
            fclose(file);
            return status;
        }
        inspect_unit(&state, unit, length);
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
