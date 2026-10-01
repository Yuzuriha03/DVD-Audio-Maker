#include "mlp_rate.h"

void mlp_rate_init(mlp_rate_state *state)
{
    if (state) { state->arrival = 0x8000; state->decode = 0; }
}

int mlp_rate_update(mlp_rate_state *state, unsigned samples,
    unsigned words, uint32_t rate_field, uint32_t *flags, uint16_t *arrival)
{
    int32_t start, end, lower, duration;
    unsigned rate = rate_field & 0x7fff;
    uint32_t result;
    int failure;
    if (!state || !flags || !arrival || !samples || samples > 160 ||
        !words || words > 0xfff || !rate) return -1;
    start = state->arrival;
    lower = (int32_t)state->decode - (int32_t)samples*90;
    if (start > (int32_t)state->decode + 0x4000) start -= 0x10000;
    if (start < lower) start = lower;
    duration = (int32_t)((words*256 + rate-1)/rate);
    end = start + duration;
    failure = end >= state->decode;
    result = *flags;
    if (failure) { result |= 0x100; end = state->decode; }
    if (words > 0x300) result |= 0x2000;
    *arrival = (uint16_t)start;
    state->arrival = (uint16_t)end;
    state->decode = (uint16_t)(state->decode + samples);
    *flags = result;
    return failure;
}