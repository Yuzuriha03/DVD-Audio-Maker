#include "mlp_entropy.h"

static int64_t floor_div(int64_t value, int64_t divisor)
{
    return value >= 0 ? value / divisor : -((-value + divisor - 1) / divisor);
}
int mlp_entropy_code(int32_t x, const mlp_coding_params *params,
                      unsigned qss, mlp_codeword *result)
{
    unsigned b, width;
    uint64_t unit, central, code;
    int64_t u, quotient;
    mlp_codeword word;
    if (!params || !result || params->mode < 0 || params->mode > 3 ||
        qss > 15 || params->total_width < (int)qss ||
        params->total_width > 31 || params->offset < -16384 ||
        params->offset > 16383 || x < -8388608 || x > 8388607)
        return MLP_BITS_INVALID;
    b = (unsigned)params->total_width - qss;
    if (b > 24) return MLP_BITS_INVALID;
    unit = UINT64_C(1) << b;
    if (params->mode == 0) {
        u = (int64_t)x - params->offset + (int64_t)(unit / 2);
        if (u < 0 || (uint64_t)u >= unit) return MLP_BITS_INVALID;
        code = (uint64_t)u;
        width = b;
    } else {
        unsigned exponent = b + 3u - (unsigned)params->mode;
        central = UINT64_C(1) << exponent;
        u = (int64_t)x - params->offset + (int64_t)(central / 2);
        if (u < 0) {
            quotient = floor_div(u, (int64_t)unit);
            if (quotient < -7) return MLP_BITS_INVALID;
            width = b + (unsigned)(-quotient) + 2;
            code = unit | ((uint64_t)u & (unit - 1));
        } else if ((uint64_t)u >= central) {
            quotient = floor_div(u - (int64_t)central, (int64_t)unit);
            if (quotient > 6) return MLP_BITS_INVALID;
            width = b + (unsigned)quotient + 3;
            code = unit | (UINT64_C(1) << (width - 2)) | ((uint64_t)u & (unit - 1));
        } else {
            width = b + 4u - (unsigned)params->mode;
            code = (uint64_t)u + central;
        }
    }
    if (width > 31 || width > b + 9 || code >= (UINT64_C(1) << width))
        return MLP_BITS_INVALID;
    word.value = (uint32_t)code;
    word.width = width;
    *result = word;
    return MLP_BITS_OK;
}
int mlp_entropy_put(mlp_bits *bits, int32_t x,
                     const mlp_coding_params *params, unsigned qss)
{
    mlp_codeword word;
    int status = mlp_entropy_code(x, params, qss, &word);
    return status ? status : mlp_bits_put(bits, word.value, word.width);
}