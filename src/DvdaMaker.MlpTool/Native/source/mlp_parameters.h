#ifndef MLP_PARAMETERS_H
#define MLP_PARAMETERS_H
#include "mlp_entropy.h"
/* Wire-level structures, independent of original memory layout. */
typedef struct mlp_matrix_primitive {
    unsigned target, bypass;
    int32_t coefficient[18];
} mlp_matrix_primitive;
typedef struct mlp_wire_filter {
    unsigned changed, precision, order;
    double coefficient[8];
} mlp_wire_filter;
typedef struct mlp_wire_state {
    unsigned changed, count;
    int32_t value[8];
} mlp_wire_state;
typedef struct mlp_parameters {
    unsigned minimum_channel, maximum_channel, flags, blocksize;
    unsigned matrix_changed, matrix_count;
    mlp_matrix_primitive matrix[15];
    int output_shift[16];
    unsigned qss[16];
    mlp_wire_filter a[16], b[16];
    mlp_wire_state state[16];
    mlp_coding_params coding[16];
} mlp_parameters;
/* old provides the actual comparison baseline: initial parameters when
 * writing restart's main block, previous main parameters otherwise.
 * Does not update the baseline. No alignment, samples or trailer appended.
 * On error the original writer and its buffer are untouched. */
MLP_BITS_API int mlp_parameters_put(mlp_bits *writer,
    const mlp_parameters *current, const mlp_parameters *old,
    int restart, int initial);
#endif