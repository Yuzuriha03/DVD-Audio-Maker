#ifndef MLP_RATE_H
#define MLP_RATE_H
#include "mlp_bits.h"
typedef struct mlp_rate_state {
    uint16_t arrival, decode;
} mlp_rate_state;
/* 1000db90 initialization and 1000d900 update. Rate is the original
 * lower 15-bit field, NOT a caller-supplied bits/second value.
 * duration = ceil(words*256/rate); words are 16-bit AU words.
 * Returns 1 FIFO failure (flag 0x100), 0 success, -1 invalid/unchanged.
 * AUs above 0x300 words get size flag 0x2000, independently of FIFO.
 * This is the timing primitive, not a compression/restart scheduler. */
MLP_BITS_API void mlp_rate_init(mlp_rate_state *state);
MLP_BITS_API int mlp_rate_update(mlp_rate_state *state, unsigned samples,
    unsigned words, uint32_t rate_field, uint32_t *flags, uint16_t *arrival);
#endif