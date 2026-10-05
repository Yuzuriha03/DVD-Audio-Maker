#include "mlp_parameters.h"
#include <math.h>
#include <string.h>

static unsigned trailing(uint32_t value)
{
    unsigned n = 0;
    if (!value) return 32;
    while (!(value & 1u)) { value >>= 1; ++n; }
    return n;
}
static int filter(mlp_bits *w, const mlp_wire_filter *f, unsigned q)
{
    int32_t cf[8];
    uint32_t combined = 0;
    unsigned i, l = 16, t, bits, k;
    if (f->order > 8 || f->precision > 15 || q < 8 || q > 15)
        return MLP_BITS_INVALID;
    mlp_bits_put(w, f->order, 4);
    if (!f->order) return 0;
    for (i = 0; i < f->order; ++i) {
        double scaled = trunc(f->coefficient[i] * 65536.0);
        if (!isfinite(scaled) || scaled < INT32_MIN || scaled > INT32_MAX)
            return MLP_BITS_INVALID;
        cf[i] = (int32_t)scaled;
        combined |= (uint32_t)cf[i];
        while (l < 31 && ((int64_t)cf[i] < -(INT64_C(1) << l) ||
                          (int64_t)cf[i] >= (INT64_C(1) << l))) ++l;
    }
    t = 16 - f->precision;
    if (combined) {
        /* Original search starts at t, not at bit zero. */
        while (t < 32 && !(combined & (UINT32_C(1) << t))) ++t;
    }
    if (t > 23 - q) t = 23 - q;
    bits = l + 1 - t;
    k = q + t - 16;
    if (!bits || bits > 16 || k > 7) return MLP_BITS_INVALID;
    mlp_bits_put(w, q, 4); mlp_bits_put(w, bits, 5); mlp_bits_put(w, k, 3);
    for (i = 0; i < f->order; ++i)
        mlp_bits_put(w, ((uint32_t)cf[i] >> t) & ((1u << bits) - 1), bits);
    return 0;
}
static int state(mlp_bits *w, const mlp_wire_state *s)
{
    unsigned i, e = 0, p, bits;
    uint32_t combined = 0;
    if (s->count > 8) return MLP_BITS_INVALID;
    for (i = 0; i < s->count; ++i) {
        int64_t value = s->value[i];
        combined |= (uint32_t)value;
        while (e < 31 && (value < -(INT64_C(1) << e) ||
                          value >= (INT64_C(1) << e))) ++e;
    }
    p = combined ? trailing(combined) : 0;
    if (p > 15) p = 15;
    bits = e + 1 - p;
    if (!bits || bits > 15) return MLP_BITS_INVALID;
    mlp_bits_put(w, bits, 4); mlp_bits_put(w, p, 4);
    for (i = 0; i < s->count; ++i)
        mlp_bits_put(w, ((uint32_t)s->value[i] >> p) & ((1u << bits) - 1), bits);
    return 0;
}
static int matrix(mlp_bits *w, const mlp_parameters *c)
{
    unsigned index = c->matrix_count, i;
    if (index > 15) return MLP_BITS_INVALID;
    mlp_bits_put(w, index, 4);
    while (index) {
        const mlp_matrix_primitive *m = &c->matrix[--index];
        uint32_t combined = 0;
        unsigned t;
        if (m->target > 15) return MLP_BITS_INVALID;
        for (i = 0; i < c->maximum_channel + 3; ++i) {
            if (m->coefficient[i] < -32768 || m->coefficient[i] > 32767)
                return MLP_BITS_INVALID;
            combined |= (uint32_t)m->coefficient[i];
        }
        t = trailing(combined); if (t > 14) t = 14;
        mlp_bits_put(w, m->target, 4); mlp_bits_put(w, 14 - t, 4);
        mlp_bits_put(w, m->bypass != 0, 1);
        for (i = 0; i < c->maximum_channel + 3; ++i) {
            uint32_t value = (uint16_t)m->coefficient[i];
            mlp_bits_put(w, value != 0, 1);
            if (value) mlp_bits_put(w, value >> t, 16 - t);
        }
    }
    return 0;
}
int mlp_parameters_put(mlp_bits *writer, const mlp_parameters *c,
    const mlp_parameters *old, int restart, int initial)
{
    /* Enough for 15 primitives, 16 channels, filters and 8-state histories. */
    uint32_t storage[2048];
    mlp_bits local;
    unsigned i, changed, eligible = initial || !restart;
    size_t required, n;
    if (!writer || !c || !old || writer->finished ||
        writer->count > writer->capacity || writer->pending_bits > 15 ||
        (!writer->words && writer->capacity) ||
        c->minimum_channel > c->maximum_channel || c->maximum_channel > 15)
        return MLP_BITS_INVALID;
    mlp_bits_init(&local, storage, 2048);
    local.pending = writer->pending; local.pending_bits = writer->pending_bits;
    mlp_bits_put(&local, 0, 1);
    changed = !initial && ((c->flags | old->flags) & 2);
    mlp_bits_put(&local, changed != 0, 1);
    if (changed) {
        unsigned skip = restart ? 8 : 0;
        if (c->blocksize < skip || c->blocksize - skip > 511)
            return MLP_BITS_INVALID;
        mlp_bits_put(&local, c->blocksize - skip, 9);
    }
    changed = eligible && c->matrix_changed;
    mlp_bits_put(&local, changed != 0, 1);
    if (changed && matrix(&local, c)) return MLP_BITS_INVALID;
    changed = 0;
    for (i = 0; i <= c->maximum_channel; ++i) {
        if (c->output_shift[i] < -8 || c->output_shift[i] > 7 || c->qss[i] > 15)
            return MLP_BITS_INVALID;
        if (c->output_shift[i] != old->output_shift[i]) changed = eligible;
    }
    mlp_bits_put(&local, changed, 1);
    if (changed) for (i = 0; i <= c->maximum_channel; ++i)
        mlp_bits_put(&local, (unsigned)c->output_shift[i] & 15, 4);
    changed = 0;
    for (i = 0; i <= c->maximum_channel; ++i)
        if (c->qss[i] != old->qss[i]) changed = eligible;
    mlp_bits_put(&local, changed, 1);
    if (changed) for (i = 0; i <= c->maximum_channel; ++i)
        mlp_bits_put(&local, c->qss[i], 4);
    for (i = c->minimum_channel; i <= c->maximum_channel; ++i) {
        const mlp_coding_params *p = &c->coding[i], *previous = &old->coding[i];
        unsigned new_a = !initial && c->a[i].changed;
        unsigned new_b = !initial && c->b[i].changed;
        unsigned new_offset = p->offset != previous->offset;
        unsigned q = 8;
        if (p->mode < 0 || p->mode > 3 || p->total_width < 0 ||
            p->total_width > 31 || p->offset < -16384 || p->offset > 16383)
            return MLP_BITS_INVALID;
        changed = new_a || new_b || new_offset || p->mode != previous->mode ||
                  p->total_width != previous->total_width;
        mlp_bits_put(&local, changed, 1);
        if (!changed) continue;
        if (c->a[i].precision > q) q = c->a[i].precision;
        if (c->b[i].precision > q) q = c->b[i].precision;
        mlp_bits_put(&local, new_a != 0, 1);
        if (new_a) {
            if (filter(&local, &c->a[i], q)) return MLP_BITS_INVALID;
            if (c->a[i].order) mlp_bits_put(&local, 0, 1);
        }
        mlp_bits_put(&local, new_b != 0, 1);
        if (new_b) {
            if (filter(&local, &c->b[i], q)) return MLP_BITS_INVALID;
            if (c->b[i].order) {
                mlp_bits_put(&local, c->state[i].changed != 0, 1);
                if (c->state[i].changed && state(&local, &c->state[i]))
                    return MLP_BITS_INVALID;
            }
        }
        mlp_bits_put(&local, new_offset, 1);
        if (new_offset) mlp_bits_put(&local, (uint32_t)p->offset & 32767, 15);
        mlp_bits_put(&local, (uint32_t)p->mode, 2);
        mlp_bits_put(&local, (uint32_t)p->total_width, 5);
    }
    required = local.count;
    n = required + (local.pending_bits != 0);
    if (n > writer->capacity - writer->count) return MLP_BITS_FULL;
    if (required)
        memcpy(writer->words + writer->count, storage, required * sizeof(*storage));
    writer->count += required;
    writer->pending = local.pending; writer->pending_bits = local.pending_bits;
    return MLP_BITS_OK;
}