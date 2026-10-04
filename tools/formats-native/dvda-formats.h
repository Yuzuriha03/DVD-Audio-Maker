#ifndef DVDA_FORMATS_H
#define DVDA_FORMATS_H

#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
#define DVDA_FORMATS_API __declspec(dllexport)
#else
#define DVDA_FORMATS_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* Keep the ABI deliberately boring: fixed-width fields and a packed layout. */
#pragma pack(push, 1)
typedef struct dvda_mlp_inspection {
    uint64_t size;
    uint32_t access_unit_count;
    uint32_t major_sync_count;
    double major_sync_interval;
    int32_t major_sync_error_count;
    int32_t access_unit_parity_error_count;
    int32_t substream_error_count;
    int32_t has_end_of_stream;
    int32_t peak_bitrate_raw;
    int32_t extended_substream_info;
    int32_t sample_rate;
    int32_t is_valid;
    int32_t error_code;
} dvda_mlp_inspection;

typedef struct dvda_pcm_comparison {
    int32_t match;
    int32_t reason_code;
    uint64_t source_bytes;
    uint64_t decoded_bytes;
    uint64_t trailing_zero_bytes;
    uint64_t first_mismatch_offset;
} dvda_pcm_comparison;

typedef struct dvda_mlp_alignment {
    int32_t peak_changes;
    int32_t extended_changes;
    int32_t checksum_changes;
    int32_t inserted_end_of_stream;
    int32_t old_header;
    int32_t new_header;
} dvda_mlp_alignment;
#pragma pack(pop)

/* Return 0 on success. Negative values are stable error classes. */
DVDA_FORMATS_API int dvda_formats_mlp_inspect_buffer(
    const uint8_t *data, size_t size, dvda_mlp_inspection *inspection);
DVDA_FORMATS_API int dvda_formats_mlp_inspect_file(
    const char *utf8_path, dvda_mlp_inspection *inspection);

DVDA_FORMATS_API int dvda_formats_pcm_compare_files(
    const char *utf8_source_path, const char *utf8_decoded_path,
    uint32_t bytes_per_sample_frame, uint32_t max_trailing_zero_frames,
    dvda_pcm_comparison *comparison);

DVDA_FORMATS_API int dvda_formats_parse_pts(
    const uint8_t *data, size_t size, int64_t *value);
DVDA_FORMATS_API int dvda_formats_sample_rate(
    const uint8_t *major_sync, size_t size, int32_t *value);
DVDA_FORMATS_API int32_t dvda_formats_peak_bitrate_raw(int32_t sample_rate);
DVDA_FORMATS_API int dvda_formats_checksum16(
    const uint8_t *data, size_t size, uint16_t *value);
DVDA_FORMATS_API int dvda_formats_checksum8(
    const uint8_t *data, size_t size, uint8_t *value);
DVDA_FORMATS_API uint8_t dvda_formats_calculate_parity(
    const uint8_t *data, size_t size);

/* Align returns a newly allocated buffer. Release it with dvda_formats_free. */
DVDA_FORMATS_API int dvda_formats_mlp_align_buffer(
    const uint8_t *data, size_t size, uint8_t **aligned_data,
    size_t *aligned_size, dvda_mlp_alignment *alignment);
DVDA_FORMATS_API void dvda_formats_free(void *pointer);

#ifdef __cplusplus
}
#endif

#endif
