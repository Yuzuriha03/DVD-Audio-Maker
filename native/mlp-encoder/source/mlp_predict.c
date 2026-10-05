#include "mlp_predict.h"
#include <math.h>
#include <string.h>

static void push(float *history, unsigned count, float value)
{
    unsigned i;
    for (i = count - 1; i != 0; --i) history[i] = history[i - 1];
    history[0] = value;
}

static int valid_filter(const mlp_predict_filter *filter, unsigned limit)
{
    unsigned i;
    if (filter->order > limit) return 0;
    for (i = 0; i < 8; ++i)
        if (!isfinite(filter->coefficient[i])) return 0;
    return 1;
}

static int32_t sar(int32_t value, unsigned shift)
{
    int64_t divisor = INT64_C(1) << shift;
    int64_t v = value;
    return (int32_t)(v >= 0 ? v / divisor : -((-v + divisor - 1) / divisor));
}

int mlp_predict_block(mlp_predict_filter *a, mlp_predict_filter *b,
    mlp_predict_state *state, unsigned qss, const int32_t *input,
    size_t count, int32_t *output, unsigned update_flag)
{
    mlp_predict_state initial, work;
    int32_t temporary[160];
    size_t n;
    unsigned k;
    int fallback = 0;
    double divisor, bound;
    if (!a || !state || !input || !output || qss > 15 ||
        count < 2 || count > 160 || (count & 1) ||
        !valid_filter(a, 8) || (b && !valid_filter(b, 4)))
        return MLP_PREDICT_INVALID;
    for (n = 0; n < count; ++n)
        if (input[n] < -8388608 || input[n] > 8388607)
            return MLP_PREDICT_INVALID;
    for (k = 0; k < 8; ++k)
        if (!isfinite(state->input[k])) return MLP_PREDICT_INVALID;
    for (k = 0; k < 4; ++k)
        if (!isfinite(state->residual[k])) return MLP_PREDICT_INVALID;
    initial = work = *state;
    divisor = (double)(UINT32_C(1) << qss);
    bound = 8388608.0 / divisor;
    for (n = 0; n < count; ++n) {
        double prediction = 0.0, residual, result;
        for (k = 0; k < 8; ++k)
            prediction += a->coefficient[k] * (double)work.input[k];
        if (b)
            for (k = 4; k != 0; --k)
                prediction += b->coefficient[k - 1] *
                              (double)work.residual[k - 1];
        residual = b ? (double)input[n] - floor(prediction)
                     : -floor(prediction - (double)input[n]);
        result = ceil(residual / divisor);
        /* Do not emulate undefined legacy _ftol overflow. */
        if (!isfinite(residual) || residual < -2147483647.0 ||
            residual > 2147483647.0 || result < -bound || result >= bound) {
            fallback = 1;
            break;
        }
        temporary[n] = (int32_t)result;
        push(work.input, 8, (float)input[n]);
        if (b) push(work.residual, 4, (float)residual);
    }
    if (fallback) {
        work = initial;
        for (n = 0; n < count; ++n) {
            temporary[n] = sar(input[n], qss);
            push(work.input, 8, (float)input[n]);
            if (b) push(work.residual, 4, (float)input[n]);
        }
        a->changed = update_flag == 0;
        a->order = 1;
        a->coefficient[0] = 0.0;
        if (b) {
            b->changed = update_flag == 0;
            b->order = 0;
        }
    }
    memcpy(output, temporary, count * sizeof(*output));
    *state = work;
    return fallback ? MLP_PREDICT_FALLBACK : MLP_PREDICT_OK;
}