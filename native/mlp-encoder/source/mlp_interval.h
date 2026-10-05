#ifndef MLP_INTERVAL_H
#define MLP_INTERVAL_H
#include "mlp_bits.h"
#define MLP_INTERVAL_SLOTS 1265
typedef struct mlp_boundary_state {
    uint32_t position,index,segments;
} mlp_boundary_state;
/* 10007470, called by input queueing to create restart flag 2.
 * Zero-initialize state. cycle=descriptor+1c (0 uses preferred_span at +2c);
 * samples=descriptor+20. Legacy division and the fixed 32 comparison are
 * intentionally distinct. DWORD wrap precedes signed comparisons.
 * Returns 1 boundary, 0 ordinary AU, -1 invalid (state unchanged).
 * samples=0 only clears position, preserving index and segments. */
MLP_BITS_API int mlp_interval_boundary(mlp_boundary_state *state,
    unsigned cycle, unsigned preferred_span, unsigned samples);
typedef struct mlp_interval_slot {
    int32_t samples;
    uint32_t flags;
} mlp_interval_slot;
/* 10007750: cursor is exclusive end search position. Scan past ordinary
 * positive-length blocks until boundary flag 2 or nonpositive length.
 * On reaching write cursor, return 0 and retain preceding position.
 * An empty endpoint marks the preceding slot with flag 8.
 * Returns 1 boundary, 0 pending, -1 invalid (unchanged). */
MLP_BITS_API int mlp_interval_find(mlp_interval_slot slots[MLP_INTERVAL_SLOTS],
    unsigned write_cursor, unsigned *cursor);
#endif
