#include "mlp_substream.h"
#include <stdlib.h>
#include <string.h>
static int samples(mlp_bits *w, const mlp_substream *s,
    const mlp_parameters *p, size_t first, size_t count)
{
    size_t n;
    unsigned ch;
    int rc;
    for (n = first; n < first + count; ++n) {
        if (s->bypass_bits) {
            rc = mlp_bits_put(w, s->bypass[n] >> (8 - s->bypass_bits), s->bypass_bits);
            if (rc) return rc;
        }
        for (ch = p->minimum_channel; ch <= p->maximum_channel; ++ch) {
            rc = mlp_entropy_put(w, s->residual[n * s->stride + ch],
                                 &p->coding[ch], p->qss[ch]);
            if (rc) return rc;
        }
    }
    return 0;
}
int mlp_substream_put(mlp_bits *writer, const mlp_substream *s)
{
    mlp_bits local;
    uint32_t *storage;
    size_t first = 0;
    int rc;
    if (!writer || !s || !s->main || !s->previous || !s->residual ||
        writer->count || writer->pending_bits || writer->pending || writer->finished ||
        (!writer->words && writer->capacity) || writer->capacity > SIZE_MAX / sizeof(uint32_t) ||
        !s->count || s->count > 160 || !s->stride || s->stride > 16 ||
        s->main->minimum_channel > s->main->maximum_channel ||
        s->main->maximum_channel >= s->stride || s->bypass_bits > 8 ||
        (s->bypass_bits && !s->bypass) ||
        (s->single_restart != 0 && s->single_restart != 1)) return MLP_BITS_INVALID;
    if (s->single_restart && (!s->restart || s->count < 8 ||
        s->main->blocksize != s->count || !(s->main->flags & 2)))
        return MLP_BITS_INVALID;
    if (s->restart && (s->restart->minimum_channel != s->main->minimum_channel ||
        s->restart->maximum_channel != s->main->maximum_channel)) return MLP_BITS_INVALID;
    if (s->restart && !s->single_restart && (!s->initial || s->count < 16 ||
        s->initial->minimum_channel != s->main->minimum_channel ||
        s->initial->maximum_channel != s->main->maximum_channel)) return MLP_BITS_INVALID;
    if (!writer->capacity) return MLP_BITS_FULL;
    storage = malloc(writer->capacity * sizeof(*storage));
    if (!storage) return MLP_BITS_FULL;
    mlp_bits_init(&local, storage, writer->capacity);
    if (s->restart) {
        rc = mlp_restart_put(&local, s->restart, s->primary);
        if (rc) goto done;
        if (s->single_restart) {
            rc = mlp_parameters_put(&local, s->main, s->previous, 0, 0);
            if (rc) goto done;
            rc = samples(&local, s, s->main, 0, s->count);
            if (rc) goto done;
            goto last_block;
        }
        rc = mlp_parameters_put(&local, s->initial, s->previous, 1, 1);
        if (rc) goto done;
        rc = samples(&local, s, s->initial, 0, 8);
        if (rc) goto done;
        rc = mlp_bits_put(&local, 2, 3); /* 0,1,0 */
        if (rc) goto done;
        first = 8;
    } else {
        rc = mlp_bits_put(&local, 2, 2); /* header present, no restart */
        if (rc) goto done;
    }
    rc = mlp_parameters_put(&local, s->main,
        s->restart ? s->initial : s->previous, s->restart != NULL, 0);
    if (rc) goto done;
    rc = samples(&local, s, s->main, first, s->count - first);
    if (rc) goto done;
last_block:
    rc = mlp_bits_put(&local, 1, 1);
    if (rc) goto done;
    rc = mlp_bits_seal(&local, s->end_markers);
    if (rc) goto done;
    memcpy(writer->words, storage, local.count * sizeof(*storage));
    writer->count = local.count; writer->finished = 1;
done:
    free(storage);
    return rc;
}
