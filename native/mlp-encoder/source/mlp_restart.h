#ifndef MLP_RESTART_H
#define MLP_RESTART_H
#include "mlp_bits.h"
typedef struct mlp_restart {
    unsigned timing, minimum_channel, maximum_channel;
    unsigned dither_shift;
    uint32_t seed;
    unsigned maximum_shift, maximum_lsbs, maximum_bits;
    unsigned lossless_check;
    unsigned assignment[16];
} mlp_restart;
/* Writes outer flags 11, restart header and CRC into an empty writer.
 * primary!=0 uses identity assignments. No padding or sealing is added.
 * Invalid/capacity failures leave writer and backing buffer unchanged. */
MLP_BITS_API int mlp_restart_put(mlp_bits *writer,
    const mlp_restart *header, int primary);
#endif