#ifndef MLP_COST_H
#define MLP_COST_H
#include "mlp_bits.h"
typedef struct mlp_cost_stats {
    int32_t minimum;
    int32_t maximum;
    unsigned shift;
    uint32_t packed_cost[3];
} mlp_cost_stats;
/* New API; samples must lie in signed 24-bit range. Does not select a mode. */
MLP_BITS_API int mlp_cost_analyze(const int32_t *samples, size_t count,
                                 mlp_cost_stats *result);
/* q is the signed table index, -18..17; mode is 1..3. */
MLP_BITS_API int mlp_cost_table(unsigned mode, int q, uint32_t *value);
typedef struct mlp_coding_params {
    int32_t mode;
    int32_t total_width;
    int32_t offset;
} mlp_coding_params;
/* Select an already-sliced block (no implicit restart skip).
 * Supported count: 1..160; first8 requires exactly 8 samples.
 * limit only filters the normalized-width lookup candidates. */
MLP_BITS_API int mlp_cost_select(const int32_t *samples, size_t count,
                                int base, const mlp_coding_params *previous,
                                int first8, int limit, mlp_coding_params *result);
#endif