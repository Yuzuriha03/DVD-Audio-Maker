#ifndef MLP_SUBSTREAM_H
#define MLP_SUBSTREAM_H
#include "mlp_restart.h"
#include "mlp_parameters.h"
typedef struct mlp_substream {
    const mlp_restart *restart; /* NULL for a non-restart block */
    const mlp_parameters *initial, *main, *previous;
    const int32_t *residual; /* sample-major, already qss-shifted */
    const uint8_t *bypass;
    size_t count, stride;
    unsigned bypass_bits;
    int primary, end_markers;
    int single_restart; /* 0/1; explicit one-block restart, count>=8, flags&2,
                        * main.blocksize==count. initial is unused. */
} mlp_substream;
/* Empty writer required; atomic on failure. Limited to 1..160 samples and
 * 16 channels. Builds a substream, NOT a complete MLP access unit/file. */
MLP_BITS_API int mlp_substream_put(mlp_bits *writer, const mlp_substream *input);
#endif
