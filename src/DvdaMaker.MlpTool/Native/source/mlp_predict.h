#ifndef MLP_PREDICT_H
#define MLP_PREDICT_H
#include "mlp_bits.h"
/* Independent API; not the original descriptor ABI. Newest history first.
 * Coefficients are already scaled; qss applies only to output residuals.
 * This models the mathematics, not arbitrary x87 rounding modes. */
typedef struct mlp_predict_filter {
    unsigned changed;
    unsigned order;
    double coefficient[8];
} mlp_predict_filter;
typedef struct mlp_predict_state {
    float input[8];
    float residual[4];
} mlp_predict_state;
enum { MLP_PREDICT_OK = 0, MLP_PREDICT_FALLBACK = 1,
       MLP_PREDICT_INVALID = -1 };
/* count: even, 2..160; qss: 0..15; input: signed 24-bit.
 * b may be NULL. Input/output must not overlap. Invalid calls leave state,
 * descriptors and output unchanged. Fallback resets the complete block. */
MLP_BITS_API int mlp_predict_block(mlp_predict_filter *a,
    mlp_predict_filter *b, mlp_predict_state *state, unsigned qss,
    const int32_t *input, size_t count, int32_t *output,
    unsigned update_flag);
#endif