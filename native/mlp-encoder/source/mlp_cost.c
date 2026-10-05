#include "mlp_cost.h"

/* Each table has 36 entries. Original zero pointers: 10018a30, 10018ac0,
 * 10018b50. Captured from .rdata; indexed here with q+18. */
#define PAIR(v) v, v
static const uint32_t tables[3][36] = {
    {
        PAIR(0x00901403u), PAIR(0x00801003u), PAIR(0x00701003u),
        PAIR(0x00600c03u), PAIR(0x00500c03u), PAIR(0x00400c03u),
        PAIR(0x00300c03u), PAIR(0x00300c03u), PAIR(0x00300c03u),
        PAIR(0x00300c03u), PAIR(0x00300c03u), PAIR(0x00300c03u),
        PAIR(0x00400c03u),
        PAIR(0x00500c03u), PAIR(0x00600c03u), PAIR(0x00701003u),
        PAIR(0x00801003u), PAIR(0x00901403u)
    },
    {
        PAIR(0x0ff01804u), PAIR(0x00901403u), PAIR(0x00801403u),
        PAIR(0x00701003u), PAIR(0x00601003u), PAIR(0x00500c02u),
        PAIR(0x00400c02u), PAIR(0x00300802u), PAIR(0x00200802u),
        PAIR(0x00200802u), PAIR(0x00300802u), PAIR(0x00400c02u),
        PAIR(0x00500c02u), PAIR(0x00601003u), PAIR(0x00701003u),
        PAIR(0x00801403u), PAIR(0x00901403u), PAIR(0x0ff01804u)
    },
    {
        0x0ff01804u, 0x0ff01804u, 0x0ff01804u, 0x00901804u,
        0x00901404u, 0x00801404u, 0x00801403u, 0x00701403u,
        0x00701003u, 0x00601003u, 0x00601003u, 0x00501003u,
        0x00500c03u, 0x00400c03u, 0x00400c01u, 0x00300c01u,
        0x00300401u, 0x00100401u, 0x00100401u, 0x00300401u,
        0x00300c01u, 0x00400c01u, 0x00400c03u, 0x00500c03u,
        0x00501003u, 0x00601003u, 0x00601003u, 0x00701003u,
        0x00701403u, 0x00801403u, 0x00801404u, 0x00901404u,
        0x00901804u, 0x0ff01804u, 0x0ff01804u, 0x0ff01804u
    }
};
#undef PAIR

/* Floor division emulates SAR for the supported signed 24-bit domain. */
static int32_t arithmetic_shift(int32_t value, unsigned shift)
{
    int64_t divisor = INT64_C(1) << shift;
    int64_t wide = value;
    return (int32_t)(wide >= 0 ? wide / divisor : -((-wide + divisor - 1) / divisor));
}

int mlp_cost_table(unsigned mode, int q, uint32_t *value)
{
    if (!value || mode < 1 || mode > 3 || q < -18 || q > 17)
        return MLP_BITS_INVALID;
    *value = tables[mode - 1][q + 18];
    return MLP_BITS_OK;
}

int mlp_cost_analyze(const int32_t *samples, size_t count, mlp_cost_stats *result)
{
    mlp_cost_stats stats = {0, 0, 0, {0, 0, 0}};
    size_t i;
    unsigned mode;
    if (!result || (!samples && count)) return MLP_BITS_INVALID;
    if (count) stats.minimum = stats.maximum = samples[0];
    for (i = 0; i < count; ++i) {
        if (samples[i] < -8388608 || samples[i] > 8388607) return MLP_BITS_INVALID;
        if (samples[i] < stats.minimum) stats.minimum = samples[i];
        if (samples[i] > stats.maximum) stats.maximum = samples[i];
    }
    while (arithmetic_shift(stats.minimum, stats.shift) < -9 ||
           arithmetic_shift(stats.maximum, stats.shift) > 8) ++stats.shift;
    for (i = 0; i < count; ++i) {
        /* Double before shifting, exactly as SHL EAX,1; SAR EAX,CL.
         * Doubling cannot overflow with the checked input domain. */
        int32_t q = arithmetic_shift(samples[i] * 2, stats.shift);
        if (q < -18 || q > 17) return MLP_BITS_INVALID;
        for (mode = 0; mode < 3; ++mode)
            stats.packed_cost[mode] += tables[mode][q + 18];
    }
    *result = stats;
    return MLP_BITS_OK;
}