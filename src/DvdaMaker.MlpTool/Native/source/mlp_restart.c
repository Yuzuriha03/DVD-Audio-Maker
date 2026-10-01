#include "mlp_restart.h"
#include <string.h>

static unsigned checksum(const mlp_bits *writer)
{
    uint32_t crc = 0x31ea;
    size_t i;
    unsigned bit;
    for (i = 1; i < writer->count; ++i) {
        uint32_t t = crc >> 9;
        crc = ((((crc & 0x1ffu) << 15) ^ t) << 1) ^ writer->words[i] ^ t;
    }
    for (bit = 0; bit < 17; ++bit) {
        if (crc & 0x01000000u) crc ^= 0x011d0000u;
        crc <<= 1;
    }
    if (writer->pending_bits)
        crc ^= (uint32_t)writer->pending << (17u - writer->pending_bits);
    for (bit = 0; bit < writer->pending_bits; ++bit) {
        if (crc & 0x01000000u) crc ^= 0x011d0000u;
        crc <<= 1;
    }
    return crc >> 17;
}

int mlp_restart_put(mlp_bits *writer, const mlp_restart *h, int primary)
{
    mlp_bits local;
    uint32_t words[16];
    unsigned i;
    size_t total;
    if (!writer || !h || writer->finished || writer->count ||
        writer->pending_bits || writer->pending ||
        (!writer->words && writer->capacity) ||
        h->timing > 65535 || h->minimum_channel > h->maximum_channel ||
        h->maximum_channel > 15 || h->dither_shift > 15 ||
        h->seed > 0x7fffff || h->maximum_lsbs > 31 ||
        h->maximum_bits > 31 || h->lossless_check > 255)
        return MLP_BITS_INVALID;
    for (i = 0; i <= h->maximum_channel; ++i)
        if (!primary && h->assignment[i] > 63) return MLP_BITS_INVALID;
    total = 123u + 6u * (h->maximum_channel + 1u);
    if (writer->capacity < (total + 15u) / 16u) return MLP_BITS_FULL;
    mlp_bits_init(&local, words, 16);
    /* All values and worst-case capacity validated above. */
    mlp_bits_put(&local, 3, 2);
    mlp_bits_put(&local, 0x31ea, 14);
    mlp_bits_put(&local, h->timing, 16);
    mlp_bits_put(&local, h->minimum_channel, 4);
    mlp_bits_put(&local, h->maximum_channel, 4);
    mlp_bits_put(&local, h->maximum_channel, 4);
    mlp_bits_put(&local, h->dither_shift, 4);
    mlp_bits_put(&local, h->seed >> 16, 7);
    mlp_bits_put(&local, h->seed & 65535u, 16);
    mlp_bits_put(&local, h->maximum_shift & 15u, 4);
    mlp_bits_put(&local, h->maximum_lsbs, 5);
    mlp_bits_put(&local, 33u * h->maximum_bits, 10);
    mlp_bits_put(&local, 0, 1);
    mlp_bits_put(&local, h->lossless_check, 8);
    mlp_bits_put(&local, 0, 16);
    for (i = 0; i <= h->maximum_channel; ++i)
        mlp_bits_put(&local, primary ? i : h->assignment[i], 6);
    mlp_bits_put(&local, checksum(&local), 8);
    memcpy(writer->words, words, local.count * sizeof(*words));
    writer->count = local.count;
    writer->pending = local.pending;
    writer->pending_bits = local.pending_bits;
    return MLP_BITS_OK;
}