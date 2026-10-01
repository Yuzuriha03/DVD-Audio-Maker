#ifndef MLP_ENCODER_H
#define MLP_ENCODER_H
#include "mlp_stamp.h"
#ifdef _WIN32
#if defined(MLP_ENCODER_BUILD)
#define MLP_ENCODER_API __declspec(dllexport)
#elif defined(MLP_ENCODER_USE_DLL)
#define MLP_ENCODER_API __declspec(dllimport)
#else
#define MLP_ENCODER_API
#endif
#define MLP_ENCODER_CALL __cdecl
#else
#define MLP_ENCODER_API
#define MLP_ENCODER_CALL
#endif
#ifdef __cplusplus
extern "C" {
#endif
/* Synchronous pull API, independent of original DLLs or a filesystem.
 * One invocation owns all codec state. Callback data is borrowed only for
 * the call. Independent invocations can run on separate Windows threads.
 * Input samples are signed, left-aligned 24-bit values in int32_t,
 * interleaved in the selected layout. A callback may return 1..capacity
 * frames. Both callbacks return zero on success, nonzero to cancel/fail. */
typedef int (MLP_ENCODER_CALL *mlp_encoder_read)(void *opaque,int32_t *pcm,size_t capacity,size_t *frames);
typedef int (MLP_ENCODER_CALL *mlp_encoder_write)(void *opaque,const uint8_t *bytes,size_t count);
typedef struct mlp_encoder_config {
    uint32_t struct_size,abi_version,sample_rate,bits,channels,restart_interval;
    uint64_t frames;
    const mlp_stamp_record *metadata;size_t metadata_count;
} mlp_encoder_config;
typedef struct mlp_encoder_result {
    int32_t status;uint32_t access_units;
    uint64_t input_frames,encoded_frames,output_bytes;
    char error[192];
} mlp_encoder_result;
enum { MLP_ENCODER_ABI_VERSION=1,MLP_ENCODER_OK=0,MLP_ENCODER_INVALID=-1,
       MLP_ENCODER_MEMORY=-2,MLP_ENCODER_INPUT=-3,MLP_ENCODER_OUTPUT=-4,MLP_ENCODER_FAILED=-5 };
/* Format admission follows DVD-Audio: 16/20/24 bits; 44.1/48/88.2/96 kHz
 * for 1..6 channels, 176.4/192 kHz for 1..2. SurCode GUI restrictions are
 * not applied. The channel count selects the standard layout (see mlp_format).
 * Explicit metadata is mandatory: wall-clock updates affect original bytes.
 * Defaults: restart_interval=0 selects preferred span 8; original cycle,
 * scale/matrix, joint
 * prediction search, zero-tail padding and output timing are always active.
 * Oversized restart intervals may retry losslessly without prediction;
 * successful normal plans remain byte-identical. AU size/FIFO limits remain.
 * Results are final only on return 0. On error the host discards previously
 * received partial output. All allocations and FP state are restored.
 * Current full-file evidence is documented separately from API capability. */
MLP_ENCODER_API uint32_t MLP_ENCODER_CALL mlp_encoder_abi_version(void);
MLP_ENCODER_API int MLP_ENCODER_CALL mlp_encode_stream(const mlp_encoder_config *config,
    mlp_encoder_read read,void *input,mlp_encoder_write write,void *output,mlp_encoder_result *result);
/* Explicit DVD-Audio assignment 0..20; config.channels must match its count.
 * PCM callbacks use ascending WAVE speaker-mask order, not coded group order.
 * Keeps the original config and entrypoint ABI unchanged. */
MLP_ENCODER_API int MLP_ENCODER_CALL mlp_encode_stream_layout(const mlp_encoder_config *config,
    unsigned assignment,mlp_encoder_read read,void *input,mlp_encoder_write write,void *output,
    mlp_encoder_result *result);
/* Distinct group depths: group2_bits must be 16/20/24 and <= config.bits.
 * Assignment must contain two groups; each channel's low bits are validated. */
MLP_ENCODER_API int MLP_ENCODER_CALL mlp_encode_stream_depths(const mlp_encoder_config *config,
    unsigned assignment,unsigned group2_bits,mlp_encoder_read read,void *input,
    mlp_encoder_write write,void *output,mlp_encoder_result *result);
typedef struct mlp_encoder_groups_config {
    uint32_t struct_size;
    mlp_encoder_config primary;
    uint32_t assignment,group2_sample_rate,group2_bits;
    uint64_t group2_frames;
} mlp_encoder_groups_config;
/* Native-rate group inputs. Each input callback interleaves only that group's
 * speakers in DVD-Audio assignment order. Both groups must have equal duration.
 * Group 2 rate equals group 1, or is half of 88.2/96 kHz; depth <= group 1.
 * Half-rate samples remain exact on even reconstructed frames; intervening
 * samples use deterministic symmetric interpolation. Metadata retains the
 * declared native rate/depth for both groups. */
MLP_ENCODER_API int MLP_ENCODER_CALL mlp_encode_stream_groups(const mlp_encoder_groups_config *config,
    mlp_encoder_read read1,void *input1,mlp_encoder_read read2,void *input2,
    mlp_encoder_write write,void *output,mlp_encoder_result *result);
#ifdef __cplusplus
}
#endif
#endif
