#ifndef MLP_SCALE_H
#define MLP_SCALE_H
#include "mlp_bits.h"
#define MLP_SCALE_BLOCKS 32
typedef struct mlp_scale_plan {
    unsigned scale_count[6],shift[6],qss[MLP_SCALE_BLOCKS][6];
    unsigned maximum_shift,maximum_bits;
} mlp_scale_plan;
/* Planning prefix of 10008140, stopping before matrix construction.
 * summary is the per-block OR of absolute signed24 samples, stride 6.
 * required_headroom corresponds to descriptor+8dc and candidates to +818.
 * The legacy local array supports at most 32 blocks. Atomic on error. */
MLP_BITS_API int mlp_scale_analyze(const uint32_t *summary, unsigned blocks,
    unsigned channels, unsigned candidates, const unsigned required_headroom[6],
    mlp_scale_plan *plan);
#endif
