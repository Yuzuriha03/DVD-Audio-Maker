#include "mlp_bits.h"

/* Explicit x86 SAR semantics, without signed-shift implementation dependence. */
static uint32_t sar32(uint32_t value, unsigned shift)
{
    uint32_t result = value >> shift;
    if (value & UINT32_C(0x80000000))
        result |= UINT32_MAX << (32u - shift);
    return result;
}

int mlp_bits_init(mlp_bits *ctx, uint32_t *words, size_t capacity)
{
    if (!ctx || (!words && capacity)) return MLP_BITS_INVALID;
    ctx->words = words;
    ctx->capacity = capacity;
    ctx->count = 0;
    ctx->pending = 0;
    ctx->pending_bits = 0;
    ctx->finished = 0;
    return MLP_BITS_OK;
}

int mlp_bits_put(mlp_bits *ctx, uint32_t value, unsigned width)
{
    unsigned i;
    size_t required;
    if (!ctx || ctx->finished || width > 32) return MLP_BITS_INVALID;
    if (width < 32 && (value >> width)) return MLP_BITS_INVALID;
    required = (ctx->pending_bits + width) / 16u;
    if (required > ctx->capacity - ctx->count) return MLP_BITS_FULL;
    for (i = width; i > 0; --i) {
        ctx->pending = (uint16_t)((ctx->pending << 1) | ((value >> (i - 1)) & 1u));
        if (++ctx->pending_bits == 16) {
            ctx->words[ctx->count++] = ctx->pending;
            ctx->pending = 0;
            ctx->pending_bits = 0;
        }
    }
    return MLP_BITS_OK;
}

int mlp_bits_finish(mlp_bits *ctx)
{
    if (!ctx) return MLP_BITS_INVALID;
    if (ctx->pending_bits) {
        if (ctx->count == ctx->capacity) return MLP_BITS_FULL;
        ctx->words[ctx->count++] = (uint32_t)ctx->pending << (16u - ctx->pending_bits);
        ctx->pending = 0;
        ctx->pending_bits = 0;
    }
    ctx->finished = 1;
    return MLP_BITS_OK;
}

/* Full DWORD inputs and arithmetic shifts matter. */
uint32_t mlp_bits_checksum(const uint32_t *words, size_t count)
{
    uint32_t x = 0xa9u, c = 0xa2u;
    size_t i;
    unsigned bit;
    for (i = 0; i < count; ++i) {
        uint32_t t = sar32(c, 7);
        x ^= words[i];
        c = ((((c & 0x7fu) << 12) ^ t) << 4) ^ t ^ words[i];
    }
    for (bit = 0; bit < 15; ++bit) {
        if (c & 0x400000u) c ^= 0x58c000u;
        c <<= 1;
    }
    return (((x & 0xffu) << 8) ^ (x & 0xff00u)) | sar32(c, 15);
}

int mlp_bits_seal(mlp_bits *ctx, int end_markers)
{
    uint32_t checksum;
    size_t extra;
    if (!ctx || ctx->finished) return MLP_BITS_INVALID;
    extra = (ctx->pending_bits != 0) + (end_markers ? 2u : 0u) + 1u;
    if (extra > ctx->capacity - ctx->count) return MLP_BITS_FULL;
    if (mlp_bits_finish(ctx) != MLP_BITS_OK) return MLP_BITS_FULL;
    if (end_markers) {
        ctx->words[ctx->count++] = 0xd234u;
        ctx->words[ctx->count++] = 0xd234u;
    }
    checksum = mlp_bits_checksum(ctx->words, ctx->count);
    ctx->words[ctx->count++] = checksum;
    return MLP_BITS_OK;
}

int mlp_bits_to_bytes(const mlp_bits *ctx, uint8_t *out, size_t capacity, size_t *written)
{
    size_t i;
    if (!ctx || !written || !ctx->finished || (!out && ctx->count)) return MLP_BITS_INVALID;
    if (ctx->count > capacity / 2) return MLP_BITS_FULL;
    for (i = 0; i < ctx->count; ++i) {
        out[2*i] = (uint8_t)(ctx->words[i] >> 8);
        out[2*i+1] = (uint8_t)ctx->words[i];
    }
    *written = ctx->count * 2;
    return MLP_BITS_OK;
}