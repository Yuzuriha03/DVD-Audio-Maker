#include "mlp_interval.h"

static int32_t signed32(uint32_t value)
{
    return value <= INT32_MAX ? (int32_t)value : (int32_t)((int64_t)value-INT64_C(4294967296));
}
int mlp_interval_boundary(mlp_boundary_state *state,
    unsigned cycle, unsigned preferred_span, unsigned samples)
{
    mlp_boundary_state work; int boundary;
    if (!state || cycle > INT32_MAX || !preferred_span || preferred_span > INT32_MAX || samples > 160) return -1;
    work = *state;
    if (!samples) { work.position = 0; *state = work; return 1; }
    if (!cycle) cycle = preferred_span;
    if (!work.segments || signed32(work.position) >= (int32_t)cycle) {
        work.segments = cycle/preferred_span;
        if (signed32(work.segments<<5) < (int32_t)cycle) ++work.segments;
        work.position = work.index = 0;
    }
    boundary = signed32(work.index*cycle) <= signed32(work.segments*work.position);
    work.index += boundary; ++work.position; *state = work;
    return boundary;
}

int mlp_interval_find(mlp_interval_slot slots[MLP_INTERVAL_SLOTS],
    unsigned write_cursor, unsigned *cursor)
{
    unsigned previous, next;
    if (!slots || !cursor || write_cursor >= MLP_INTERVAL_SLOTS ||
        *cursor >= MLP_INTERVAL_SLOTS) return -1;
    next = *cursor;
    do {
        previous = next;
        next = previous == MLP_INTERVAL_SLOTS-1 ? 0 : previous+1;
        if (next == write_cursor) { *cursor = previous; return 0; }
    } while (slots[next].samples > 0 && !(slots[next].flags&2));
    if (!slots[next].samples) slots[previous].flags |= 8;
    *cursor = next;
    return 1;
}
