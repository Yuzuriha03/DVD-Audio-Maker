#include "mlp_cost.h"
#ifdef _WIN32
__declspec(dllexport)
#endif
int mlp_entropy_reference(const int32_t *samples, size_t count, int base,
    const mlp_coding_params *previous, int first8, int limit,
    mlp_coding_params *result)
{
    return mlp_cost_select(samples, count, base, previous, first8, limit, result);
}
