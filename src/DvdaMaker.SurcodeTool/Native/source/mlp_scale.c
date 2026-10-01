#include "mlp_scale.h"
static unsigned trailing(uint32_t value)
{
    unsigned n = 0;
    while (!(value&1)) { value >>= 1; ++n; }
    return n;
}
static unsigned headroom(uint32_t value)
{
    unsigned n = 0;
    while (!(value&0xc00000)) { value <<= 1; ++n; }
    return n;
}
int mlp_scale_analyze(const uint32_t *summary, unsigned blocks,
    unsigned channels, unsigned candidates, const unsigned required_headroom[6],
    mlp_scale_plan *plan)
{
    mlp_scale_plan work = {0};
    uint32_t combined[6] = {0},all = 0,v;
    unsigned low[6],high[6],dimension[6] = {0},minimum = 24,common,i,b;
    int budget = 6;
    if (!summary || !required_headroom || !plan || !blocks || blocks > MLP_SCALE_BLOCKS ||
        !channels || channels > 6 || candidates > channels) return -1;
    for (i = 0; i < candidates; ++i) if (required_headroom[i] > 31) return -1;
    for (b = 0; b < blocks; ++b) for (i = 0; i < channels; ++i) {
        v = summary[b*6+i];
        if (v > 0xffffff) return -1;
        combined[i] |= v; all |= v;
    }
    common = all ? trailing(all) : 0;
    for (i = 0; i < channels; ++i) {
        low[i] = combined[i] ? trailing(combined[i]) : common;
        high[i] = combined[i] ? headroom(combined[i]) : 24-common;
        if (high[i] < minimum) minimum = high[i];
    }
    work.maximum_bits = 25; v = all;
    while (!(v&0x800000) && work.maximum_bits) { v <<= 1; --work.maximum_bits; }
    for (i = 0; i < candidates; ++i) {
        int consumed;
        dimension[i] = required_headroom[i] > minimum ? required_headroom[i]-minimum : 0;
        consumed = (int)dimension[i]-(int)low[i];
        budget -= consumed > 0 ? consumed : 1;
    }
    if (!(all&1)) {
        common = 0;
        while ((all&0xf00000) && !(all&1)) { ++common; all >>= 1; }
        for (i = 0; i < channels; ++i) if (dimension[i] < common) dimension[i] = common;
    } else {
        for (i = 0; i < channels; ++i) {
            unsigned desired = (low[i] && high[i] < 3) || (budget > 0 && !high[i]);
            if (dimension[i] < desired) { dimension[i] = desired; if (low[i] < desired) --budget; }
        }
    }
    for (i = 0; i < channels; ++i) {
        work.shift[i] = dimension[i] < low[i] ? dimension[i] : low[i];
        work.scale_count[i] = dimension[i]-work.shift[i];
        if (work.maximum_shift < work.shift[i]) work.maximum_shift = work.shift[i];
        for (b = 0; b < blocks; ++b) if (!work.scale_count[i] && summary[b*6+i]) {
            unsigned q = trailing(summary[b*6+i])-work.shift[i];
            work.qss[b][i] = q > 15 ? 15 : q;
        }
    }
    *plan = work; return 0;
}
