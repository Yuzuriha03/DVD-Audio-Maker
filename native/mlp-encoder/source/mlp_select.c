#include "mlp_cost.h"
#include <limits.h>

static int floor_shift(int value, unsigned shift)
{
    int divisor = 1 << shift;
    return value >= 0 ? value / divisor : -((-value + divisor - 1) / divisor);
}
static int overhead(const mlp_coding_params *old, int base, int mode, int width, int offset)
{
    return old->mode == mode && old->total_width - base == width &&
           old->offset == offset ? 0 : 10;
}
static void consider(int cost, int mode, int width, int *best,
                     int *best_mode, int *best_width)
{
    if (cost < *best) {
        *best = cost;
        *best_mode = mode;
        *best_width = width;
    }
}

int mlp_cost_select(const int32_t *samples, size_t count, int base,
                    const mlp_coding_params *previous, int first8, int limit,
                    mlp_coding_params *result)
{
    static const int lower[3] = {-18, -16, -15};
    static const int upper[3] = {17, 15, 14};
    mlp_cost_stats stats;
    mlp_coding_params selected;
    int best = INT_MAX, best_mode = 0, best_width = 0;
    int lo, hi, s, n, qlo, qhi, mode, width, power, offset, cost;
    if (!previous || !result || !count || count > 160 ||
        (first8 && count != 8) || base < 0 || base > 24 ||
        previous->mode < 0 || previous->mode > 3 ||
        previous->total_width < 0 || previous->total_width > 48 ||
        previous->offset < -16384 || previous->offset > 16383 ||
        limit < 0 || limit > 32) return MLP_BITS_INVALID;
    if (mlp_cost_analyze(samples, count, &stats) != 0) return MLP_BITS_INVALID;
    lo = stats.minimum; hi = stats.maximum; s = (int)stats.shift; n = (int)count;
    if (lo == hi && lo >= -16384 && lo < 16384) {
        selected.mode = 0; selected.total_width = base; selected.offset = lo;
        *result = selected;
        return MLP_BITS_OK;
    }
    qlo = floor_shift(lo * 2, stats.shift);
    qhi = floor_shift(hi * 2, stats.shift);
    for (mode = 1; mode <= 3; ++mode) {
        uint32_t packed = stats.packed_cost[mode - 1];
        int a = qlo - lower[mode - 1], b = upper[mode - 1] - qhi;
        cost = n * (s + 2) + (int)(packed & 0x3ffu) +
               overhead(previous, base, mode, s + 2, 0);
        consider(cost, mode, s + 2, &best, &best_mode, &best_width);
        cost = n * (s + 1) + (int)((packed >> 10) & 0x3ffu) +
               overhead(previous, base, mode, s + 1, 0);
        consider(cost, mode, s + 1, &best, &best_mode, &best_width);
        if (a >= 0 && b >= 0 && s + 9 - a / 2 <= limit && s + 9 - b / 2 <= limit) {
            cost = n * s + (int)(packed >> 20) + overhead(previous, base, mode, s, 0);
            consider(cost, mode, s, &best, &best_mode, &best_width);
        }
    }
    width = 0; power = 1;
    while (power <= hi - lo) { power *= 2; ++width; }
    offset = previous->offset;
    if (lo - offset < -(power / 2) || hi - offset >= power / 2) {
        int midpoint = -floor_shift(-(lo + hi), 1);
        int zero_width = width;
        while (lo < -(power / 2) || hi >= power / 2) { power *= 2; ++zero_width; }
        if (midpoint >= -16384 && midpoint < 16384 &&
            zero_width - width >= (first8 ? 4 : 1)) offset = midpoint;
        else { offset = 0; width = zero_width; }
    }
    cost = n * width + overhead(previous, base, 0, width, offset);
    if (offset != previous->offset && offset != 0) cost += first8 ? 30 : 15;
    if (cost < best) {
        selected.mode = 0; selected.total_width = base + width;
        selected.offset = width != 24 ? offset : 0;
    } else {
        selected.mode = best_mode; selected.total_width = base + best_width;
        selected.offset = 0;
    }
    *result = selected;
    return MLP_BITS_OK;
}