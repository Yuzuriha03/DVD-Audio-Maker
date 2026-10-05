#ifndef MLP_MATRIX_H
#define MLP_MATRIX_H
#include "mlp_parameters.h"
typedef struct mlp_matrix_candidate {
    unsigned target;
    int32_t coefficient[8];
} mlp_matrix_candidate;
typedef struct mlp_downmix_design {
    mlp_matrix_candidate forward[2],inverse[2];
    unsigned permutation[6],post_shift[6],required_headroom[6];
} mlp_downmix_design;
/* 10005f20..100065a0: design two downmix rows from a 2x6 coefficient matrix.
 * Includes pivot/permutation, quantization, identity shortcut and original
 * 0.99/1.9 headroom gates. Precision 0..14; finite input magnitude <=16.
 * inverse[] retains original reverse row storage order. Atomic on error. */
MLP_BITS_API int mlp_matrix_design_downmix(const double coefficients[2][6],
    unsigned precision,mlp_downmix_design *output);

typedef struct mlp_downmix_plan {
    mlp_matrix_primitive forward[16], inverse[7];
    unsigned forward_count, inverse_count, bypass_bits;
    unsigned forward_noise_shift, inverse_noise_shift;
    unsigned remaining_scale[6], output_shift[6];
    int maximum_shift;
} mlp_downmix_plan;
/* 1000dc20/1000dd50: render the inverse prefix into two interleaved
 * float32 channels, in reverse primitive order. Input is signed24, stride
 * 2..6; count 0..160; qss 0..15. Extra int32 sources are converted to the
 * original float32 noise representation. Coefficients 0..3 address L/R and
 * the two noise sources; other coefficients must be zero, bypass disabled.
 * Includes the original double spill before floor and float32 spill after
 * each primitive. Return 1 for any intermediate signed24 overflow, 0 for
 * exact-range output, -1 for invalid input (output untouched). No clipping.
 * An overflowing intermediate may be used by subsequent primitives. */
MLP_BITS_API int mlp_matrix_render_stereo(const mlp_matrix_primitive *matrix,
    unsigned primitives, const unsigned qss[2], const int32_t *samples,
    size_t count, unsigned stride, const int32_t *extra0,
    const int32_t *extra1, float *stereo);
typedef struct mlp_downmix_check {
    uint32_t checksum;
    unsigned maximum_bits;
} mlp_downmix_check;
/* 100077f0: truncate float32, apply DWORD left shifts, accumulate the
 * channel-rotated checksum and magnitude width before optional signed24
 * clipping. channels 1..6, count 0..160, shifts 0..31. NULL output computes
 * only checks. The original conservatively flags -8388608 as width overflow
 * even though the optional clipped PCM retains it. Return 0/1 as original,
 * -1 atomically for invalid inputs or the original nonterminating INT_MIN
 * magnitude case. Float-to-int inputs must fit signed32. */
MLP_BITS_API int mlp_matrix_downmix_pcm(mlp_downmix_check *state,
    const float *samples, size_t count, unsigned channels,
    const unsigned *output_shift, int32_t *clipped);
typedef struct mlp_downmix_state {
    uint32_t forward_seed, inverse_seed;
    mlp_downmix_check check;
} mlp_downmix_state;
typedef struct mlp_downmix_block {
    int32_t transformed[960];
    uint8_t bypass[160]; /* Original high-bit wire packing. */
    float stereo[320];
    uint32_t forward_seed, inverse_seed; /* Seeds before this block. */
    unsigned flags; /* Includes original 0x200 / 0x400 results. */
} mlp_downmix_block;
/* 10007a40 + 100079a0: signed input shifts, two noise sequences, forward
 * prefix application, stereo rendering and persistent PCM checks. Forward
 * overflow clips that primitive and skips the remaining prefix, as original.
 * Flag 2 resets checks before this block but not RNG seeds. count 1..160,
 * channels 2..6, two inverse rows, signed16 wire coefficients, noise shifts
 * <=16. Original out-of-domain conversions return -1 atomically. Result 0
 * means completed; inspect flags for overflow, not an encoding success flag. */
MLP_BITS_API int mlp_matrix_process_downmix(mlp_downmix_state *state,
    const mlp_downmix_plan *plan, unsigned channels, const unsigned shifts[6],
    const unsigned qss[6], const int32_t *samples, size_t count,
    unsigned flags, mlp_downmix_block *output);
/* Full 10007cd0 constructor composition for the original two-candidate
 * downmix layout: forward/reverse prefixes, dither and output shifts.
 * Candidate selection and sample application remain separate. Atomic on
 * invalid input; output includes the original inactive dither scratch slot. */
MLP_BITS_API int mlp_matrix_downmix_plan(const mlp_matrix_candidate forward[2],
    const mlp_matrix_candidate inverse[2], unsigned candidates, unsigned channels,
    const unsigned scale_count[6], const unsigned shifts[6],
    const unsigned qss[6], const unsigned post_shift[6], mlp_downmix_plan *plan);
typedef struct mlp_matrix_analysis {
    unsigned channels;
    size_t samples;
    double last[6], difference[6], energy[6], covariance[36];
} mlp_matrix_analysis;
/* 1000e450/e540/e570, explicit streaming history and interval accumulator. */
MLP_BITS_API int mlp_matrix_analysis_init(mlp_matrix_analysis *state, unsigned channels);
MLP_BITS_API int mlp_matrix_analysis_add(mlp_matrix_analysis *state,
    const int32_t *samples, size_t count, unsigned stride);
/* Begin another interval while preserving the input and difference history. */
MLP_BITS_API int mlp_matrix_analysis_reset(mlp_matrix_analysis *state);
/* 1000e750: two passes, fixed prefix, largest-diagonal pivot, original
 * 100/1000 energy-ratio and 0.1 correlation gates. Matrices have stride 6.
 * Uses long double temporaries; does not promise whole-encoder x87 identity. */
MLP_BITS_API int mlp_matrix_decorrelate(const double covariance[36],
    const double ratio[6], unsigned channels, unsigned fixed, double threshold,
    unsigned order[6], double transform[36], double reduced[36]);
/* Selection portion of 1000ea80, with an empty primitive prefix.
 * Reverse pivot order, original benefit/precision gates, up to 6 entries.
 * Sample execution and overflow rollback are separate. Atomic on error. */
MLP_BITS_API int mlp_matrix_select(const double covariance[36],
    const double ratio[6], unsigned channels, unsigned fixed, size_t samples,
    const unsigned qss[6], const unsigned scale_count[6], int allow_bypass,
    mlp_matrix_primitive matrix[6], unsigned *primitives, unsigned *bypass_bits);
/* 1000ea80 selection with an existing downmix prefix. Input count/bypass
 * fields and active records are preserved; candidates append up to the
 * original six-record descriptor capacity. Covariance/ratio describe PCM
 * AFTER applying the prefix. No samples are changed and no prefix is
 * applied here. All failures are atomic. */
MLP_BITS_API int mlp_matrix_select_append(const double covariance[36],
    const double ratio[6], unsigned channels, unsigned fixed, size_t samples,
    const unsigned qss[6], const unsigned scale_count[6], int allow_bypass,
    mlp_matrix_primitive matrix[6], unsigned *primitives, unsigned *bypass_bits);
/* 10007c30: fill extra-source coefficients, never increase active count.
 * array needs primitives+1 slots: missing targets write the legacy scratch
 * slot at [primitives]. Only the first matching target is changed.
 * dimensions are 0..31, coefficient arithmetic wraps as x86 DWORD. */
MLP_BITS_API int mlp_matrix_add_dither(mlp_matrix_primitive *matrix,
    unsigned primitives, unsigned targets, unsigned extra_source,
    const unsigned dimensions[6], unsigned *noise_shift);
/* 1000dbb0: two deterministic noise sources. Explicit seed and buffers,
 * count 0..160; shifts 0..31. Atomic on invalid input.
 * This low-level DWORD API accepts seeds beyond the 23-bit wire field. */
MLP_BITS_API int mlp_matrix_noise(uint32_t *seed, size_t count, unsigned shift,
    int32_t *first, int32_t *second);
/* Forward core of 10007cd0. scale_count supplies original param_3,
 * shifts supplies param_4. Candidate target scale counts are consumed.
 * Explicit x86 DWORD wrap and SAR, rather than signed C overflow.
 * Output maximum 15 primitives / 8 bypass bits. Atomic on invalid input.
 * Does not perform candidate SEARCH, inverse construction or 10007c30. */
MLP_BITS_API int mlp_matrix_build_forward(const mlp_matrix_candidate *candidate,
    unsigned candidates, unsigned channels, const unsigned scale_count[6],
    const unsigned shifts[6], mlp_matrix_primitive output[15],
    unsigned *primitives, unsigned *bypass_bits, int signs[6], unsigned final_shifts[6]);
/* Reverse core, processing candidate order then emitting reverse order.
 * common dimension is max(shifts[0],shifts[1]) exactly as original.
 * Caller supplies ORIGINAL inverse candidates; not inferred by inversion.
 * channels matches original active candidate count (1..6).
 * Output has no bypass: inverse scaling helper 10007c30 is separate. */
MLP_BITS_API int mlp_matrix_build_reverse(const mlp_matrix_candidate *candidate,
    unsigned channels, const int signs[6], const unsigned shifts[6],
    mlp_matrix_primitive output[6], unsigned final_shifts[6]);
/* 1000de70/1000e1d0 independent sample-major signed24 matrix execution.
 * coefficients are original fixed-point units (1/16384); channels <=6,
 * two extra sources are supplied separately. Forward processes primitives
 * in listed order, inverse in reverse order, consuming bypass bits LIFO.
 * qss limits target granularity. Bypass requires target coefficient -32768
 * and qss=0; ordinary requires -16384. Explicit 8-bit bypass capacity.
 * Returns 0 exact, 1 clipped/out-of-range, -1 invalid (atomic).
 * This is execution only, not original automatic matrix selection. */
MLP_BITS_API int mlp_matrix_apply(const mlp_matrix_primitive *matrix,
    unsigned primitives, unsigned channels, const unsigned *qss,
    int32_t *samples, size_t count, const int32_t *extra0,
    const int32_t *extra1, uint8_t *bypass, int inverse, int clip);
/* Execute a selected interval, truncating at the first overflowing primitive
 * anywhere in it. Snapshot rollback preserves input exactly; no clipping.
 * Empty prefix, no extra sources. count may exceed one AU. Output bypass is
 * normalized to low bits. Return 0 unchanged prefix, 1 truncated, -1 atomic
 * error. This independent orchestration uses the original prefix policy. */
MLP_BITS_API int mlp_matrix_apply_interval(const mlp_matrix_primitive *matrix,
    unsigned *primitives, unsigned channels, const unsigned qss[6],
    int32_t *samples, size_t count, uint8_t *bypass, unsigned *bypass_bits);
/* Same prefix rollback, with per-block QSS and explicit lengths (1..160).
 * Up to 128 blocks; each QSS row has stride 6. */
MLP_BITS_API int mlp_matrix_apply_blocks(const mlp_matrix_primitive *matrix,
    unsigned *primitives, unsigned channels, const unsigned qss[][6],
    const unsigned *lengths, unsigned blocks, int32_t *samples,
    uint8_t *bypass, unsigned *bypass_bits);
/* Apply only records at/after prefix. Samples and normalized low bypass bits
 * already contain the prefix result. Preserve that result during overflow
 * rollback, and return total retained count/bypass width. Prefix dither is
 * allowed because it is not executed again; appended records have no noise.
 * This is the application half of 1000ea80, with explicit snapshots. */
MLP_BITS_API int mlp_matrix_apply_suffix_blocks(const mlp_matrix_primitive *matrix,
    unsigned prefix, unsigned *primitives, unsigned channels,
    const unsigned qss[][6], const unsigned *lengths, unsigned blocks,
    int32_t *samples, uint8_t *bypass, unsigned *bypass_bits);
#endif
