#ifndef MLP_ENTROPY_H
#define MLP_ENTROPY_H
#include "mlp_cost.h"
typedef struct mlp_codeword { uint32_t value; unsigned width; } mlp_codeword;
/* New API. x is the already shifted residual; never shift it by qss again.
 * lsbs-qss in 0..24; resulting codeword must fit the supported 31-bit path. */
MLP_BITS_API int mlp_entropy_code(int32_t x, const mlp_coding_params *params,
                                 unsigned qss, mlp_codeword *result);
MLP_BITS_API int mlp_entropy_put(mlp_bits *bits, int32_t x,
                                const mlp_coding_params *params, unsigned qss);
#endif