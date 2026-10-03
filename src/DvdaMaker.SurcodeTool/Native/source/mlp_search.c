#include "mlp_search.h"
#include <math.h>
#include <string.h>

unsigned mlp_search_rand(uint32_t *state)
{
    if (!state) return 0;
    *state = *state*UINT32_C(214013)+UINT32_C(2531011);
    return (*state >> 16) & 32767;
}

int mlp_search_iir_random(uint32_t *state, unsigned requested,
    unsigned maximum, unsigned attempts, mlp_wire_filter *result)
{
    uint32_t rng;
    unsigned trial;
    if (!state || !result || requested > 4 || !maximum || maximum > 4 ||
        !attempts) return -1;
    rng = *state;
    for (trial = 0; trial < attempts; ++trial) {
        mlp_wire_filter f = {0};
        double a[8] = {0}, k[8] = {0}, old[8], scale;
        unsigned order = requested, i, m;
        if (!order) {
            double choice = mlp_search_rand(&rng)*(1.0/327.68);
            order = choice < 5 ? 1 : choice < 20 ? 2 : choice < 50 ? 3 : 4;
            if (order > maximum) order = maximum;
        }
        f.changed = 1; f.order = order; f.precision = order+4;
        scale = (double)(1u << f.precision);
        for (i = 0; i < order; ++i) {
            /* Original constant 10018518: bits 3f00002000400080. */
            double u = mlp_search_rand(&rng)*0x1.0002000400080p-15;
            k[i] = i < order-1 ? (u-0.5)*1.9 : (u+0.05)*0.9;
            if (i == order-1 && k[i] < 0.5) k[i] -= 1;
        }
        for (m = 0; m < 8; ++m) {
            memcpy(old,a,sizeof(old));
            for (i = 0; i < m; ++i) a[i] = old[i]+k[m]*old[m-1-i];
            a[m] = k[m];
        }
        for (i = 0; i < order; ++i) f.coefficient[i] = floor(a[i]*scale+0.5)/scale;
        if (mlp_search_iir_stable(f.coefficient,order) == 1) {
            *state = rng; *result = f; return 0;
        }
    }
    return -1;
}

int mlp_search_iir_stable(const double coefficient[4], unsigned order)
{
    double a[4];
    unsigned i, m;
    if (!coefficient || !order || order > 4) return -1;
    for (i = 0; i < order; ++i) {
        if (!isfinite(coefficient[i])) return -1;
        a[i] = coefficient[i];
    }
    if (fabs(a[order-1]) < 0.5) return 0;
    for (m = order; m > 0; --m) {
        unsigned remaining = m-1;
        double k = a[remaining], inverse;
        if (!isfinite(k) || fabs(k) > 0.98) return 0;
        inverse = 1/(1-k*k);
        for (i = 0; i < remaining/2; ++i) {
            double left = a[i], right = a[remaining-1-i];
            a[i] = (left-right*k)*inverse;
            a[remaining-1-i] = (right-left*k)*inverse;
        }
        if (remaining & 1) a[remaining/2] /= 1+k;
    }
    return 1;
}

int mlp_search_correlation_extended(const int32_t *pcm, const size_t *lengths,
    size_t blocks, unsigned order, double result[50])
{
    double r[50] = {0}, history[49] = {0}, y[209], window[160];
    size_t b, n, offset = 0;
    unsigned k;
    if (!pcm || !lengths || !result || !blocks || order > 49) return -1;
    for (b = 0; b < blocks; ++b) {
        if (lengths[b] < 8 || lengths[b] > 160 || (lengths[b] & 1) ||
            offset > SIZE_MAX - lengths[b]) return -1;
        for (n = 0; n < lengths[b]; ++n)
            if (pcm[offset+n] < -8388608 || pcm[offset+n] > 8388607) return -1;
        offset += lengths[b];
    }
    for (n = 0; n < 160; ++n) {
        /* Preserve 1000ad60's multiplication and subtraction order. */
        double t = (double)(n+1) * 0x1.970e4f80cb872p-8;
        window[n] = (3.0-(t+t))*t*t;
    }
    offset = 0;
    for (b = 0; b < blocks; ++b) {
        size_t step = 160 / lengths[b];
        memcpy(y, history, sizeof(history));
        for (n = 0; n < lengths[b]; ++n) {
            double w = b == 0 ? window[n*step] :
                (b == blocks-1 ? window[159-n*step] : 1.0);
            y[49+n] = (double)(float)pcm[offset+n] * w;
        }
        /* 10008790 accumulates each lag within this AU, then adds the
         * subtotal to the interval. A global per-sample sum changes
         * rounding and can change quantized prediction coefficients. */
        for (k = 0; k <= order; ++k) {
            double sum = 0;
            for (n = 0; n < lengths[b]; ++n) sum += y[49+n]*y[49+n-k];
            r[k] += sum;
        }
        memcpy(history, y+lengths[b], sizeof(history));
        offset += lengths[b];
    }
    memcpy(result, r, sizeof(r));
    return 0;
}

int mlp_search_correlation(const int32_t *pcm, const size_t *lengths,
    size_t blocks, unsigned order, double result[9])
{
    double r[50];
    if (!result || order > 8 ||
        mlp_search_correlation_extended(pcm,lengths,blocks,order,r)) return -1;
    memcpy(result,r,9*sizeof(double));
    return 0;
}

/* Symmetric scratch has explicit space for negative indices. */
static int reflect(double r[9], unsigned order, double out[8])
{
    double storage[17], *s = storage+8;
    unsigned m, j;
    for (j = 0; j < 9; ++j) s[j] = s[-(int)j] = r[j];
    for (m = 1; m <= order; ++m) {
        double k, kk, v, e;
        if (r[0] <= 0 || !isfinite(r[0])) return -1;
        k = -s[m]/r[0];
        if (!isfinite(k)) return -1;
        if (k > 1) k = 1;
        if (k < -1) k = -1;
        out[m-1] = k; kk = k*k;
        for (j = order-m; j > 0; --j) {
            double a = s[m+j], b = s[(int)m-(int)j], c = r[j];
            double twice = c*k + c*k;
            s[m+j] = b*kk + a + twice;
            s[(int)m-(int)j] = a*kk + b + twice;
            r[j] = (1+kk)*c + (a+b)*k;
        }
        v = s[m]; e = r[0];
        s[m] = kk*v + k*e + k*e + v;
        r[0] = k*v + k*v + (1+kk)*e;
        if (!isfinite(r[0]) || r[0] < 0) return -1;
    }
    return 0;
}

static int valid_correlation(const double *r, unsigned order)
{
    unsigned i;
    if (!r || order > 8 || r[0] < 0) return 0;
    for (i = 0; i < 9; ++i) if (!isfinite(r[i])) return 0;
    return 1;
}

int mlp_search_iir_evaluate(const double *correlation, unsigned lag,
    const double *coefficient, unsigned order, unsigned max_fir_order,
    double reflection[8], double *score)
{
    double impulse[50] = {1}, h[50] = {0}, r[9] = {0}, k[8] = {0}, value;
    unsigned aorder, tail, n, j;
    if (!correlation || !reflection || !score || order > 4 ||
        max_fir_order > 8 || lag > 49 || (order && !coefficient)) return -1;
    aorder = max_fir_order < 8-order ? max_fir_order : 8-order;
    if (lag < aorder || correlation[0] < 0) return -1;
    for (n = 0; n <= lag; ++n) if (!isfinite(correlation[n])) return -1;
    for (n = 0; n < order; ++n) if (!isfinite(coefficient[n])) return -1;
    tail = lag-aorder;
    for (n = 1; n <= tail; ++n) {
        unsigned limit = n < order ? n : order;
        for (j = 1; j <= limit; ++j)
            impulse[n] -= impulse[n-j]*coefficient[j-1];
        if (!isfinite(impulse[n])) return -1;
    }
    for (j = 0; j <= tail; ++j)
        for (n = 0; n <= tail-j; ++n) h[j] += impulse[n+j]*impulse[n];
    for (n = 0; n <= aorder; ++n) {
        r[n] = h[0]*correlation[n];
        for (j = 1; j <= tail; ++j) {
            unsigned index = n > j ? n-j : j-n;
            r[n] += (correlation[index]+correlation[n+j])*h[j];
        }
        if (!isfinite(r[n])) return -1;
    }
    if (reflect(r, aorder, k)) return -1;
    value = h[0]+h[0]+r[0];
    if (!isfinite(value)) return -1;
    memcpy(reflection, k, sizeof(k)); *score = value;
    return 0;
}

int mlp_search_reflection(const double correlation[9], unsigned order,
    double reflection[8], double *error)
{
    double r[9], k[8] = {0};
    if (!reflection || !error || !valid_correlation(correlation, order)) return -1;
    memcpy(r, correlation, sizeof(r));
    if (reflect(r, order, k)) return -1;
    memcpy(reflection, k, sizeof(k)); *error = r[0];
    return 0;
}

int mlp_search_pool_init(mlp_search_pool *pool, uint32_t seed)
{
    static const unsigned precision[6] = {3,4,5,8,8,5};
    static const double coefficient[6][2] = {
        {-0.875,0}, {-0.9375,0}, {-0.96875,0},
        {-1.921875,0.92578125}, {-1.78125,0.8125}, {-1.34375,0.5625}
    };
    mlp_search_pool work = {0};
    unsigned i;
    if (!pool) return -1;
    work.count = 18; work.rng = seed;
    for (i = 0; i < work.count; ++i) {
        mlp_search_entry *e = &work.entry[i];
        if (i < 6) {
            e->filter.changed = 1; e->filter.order = i < 3 ? 1 : 2;
            e->filter.precision = precision[i];
            memcpy(e->filter.coefficient,coefficient[i],sizeof(coefficient[i]));
            e->fixed = e->no_perturb = 1;
        } else {
            unsigned relative = i-6;
            unsigned order = relative > 6 ? 4 : relative > 2 ? 3 : (relative > 0)+1;
            if (mlp_search_iir_random(&work.rng,order,4,10000,&e->filter)) return -1;
        }
    }
    *pool = work;
    return 0;
}

unsigned mlp_search_pool_replacement(const mlp_search_pool *pool, unsigned order)
{
    unsigned i, retained = 0, replacement = 0;
    uint32_t saved_age = 0;
    if (!pool || !pool->count || pool->count > MLP_SEARCH_POOL_MAX) return 0;
    for (i = 0; i < pool->count; ++i) {
        uint32_t age = pool->clock-pool->entry[retained].timestamp;
        if (!pool->entry[i].fixed && pool->entry[i].filter.order == order &&
            age >= saved_age) {
            retained = replacement = i; saved_age = age;
        }
    }
    return replacement;
}

int mlp_search_pool_select(mlp_search_pool *pool, const double *correlation,
    unsigned lag, unsigned maximum_b, unsigned maximum_a,
    unsigned perturbations, mlp_wire_filter *b, double reflection[8],
    double *relative_error)
{
    mlp_search_pool work;
    mlp_wire_filter fresh, chosen;
    double best_k[8] = {0}, trial_k[8], best = 0, weight = 1, value;
    unsigned i, selected = 0, found = 0;
    if (!pool || !b || !reflection || !relative_error || !correlation ||
        !pool->count || pool->count > MLP_SEARCH_POOL_MAX ||
        !maximum_b || maximum_b > 4 || maximum_a > 8 || lag > 49 ||
        correlation[0] <= 0 || perturbations > 100000) return -1;
    work = *pool;
    for (i = 0; i < work.count; ++i) {
        mlp_wire_filter *f = &work.entry[i].filter;
        double w;
        if (f->order > 4 || f->precision > 15) return -1;
        if (f->order > maximum_b) continue;
        if (mlp_search_iir_evaluate(correlation,lag,f->coefficient,f->order,
                                   maximum_a,trial_k,&value)) return -1;
        w = f->order ? 1.02+0.005*f->order : 1;
        if (!found || w*value < weight*best) {
            found = 1; selected = i; best = value; weight = w;
            memcpy(best_k,trial_k,sizeof(best_k));
        }
    }
    if (!found || mlp_search_iir_random(&work.rng,0,maximum_b,10000,&fresh) ||
        mlp_search_iir_evaluate(correlation,lag,fresh.coefficient,fresh.order,
                               maximum_a,trial_k,&value)) return -1;
    if ((1.02+0.005*fresh.order)*value < weight*best) {
        /* Machine code 100093d0 reads retained, not current scanned i.
         * Deliberately preserve the legacy quirk and slot-zero fallback. */
        selected = mlp_search_pool_replacement(&work,fresh.order);
        work.entry[selected].filter = fresh;
        work.entry[selected].timestamp = work.clock;
        best = value; weight = 1.02+0.005*fresh.order;
        memcpy(best_k,trial_k,sizeof(best_k));
    }
    if (!work.entry[selected].no_perturb) {
        for (i = 0; i < perturbations; ++i) {
            double coefficients[4] = {0};
            unsigned j, changed = 0;
            mlp_wire_filter *f = &work.entry[selected].filter;
            for (j = 0; j < f->order; ++j) {
                int delta = (int)(mlp_search_rand(&work.rng)/10922)-1;
                coefficients[j] = f->coefficient[j]+delta*ldexp(1.0,-(int)f->precision);
                changed |= delta != 0;
            }
            if (!changed || mlp_search_iir_stable(coefficients,f->order) != 1) continue;
            if (mlp_search_iir_evaluate(correlation,lag,coefficients,f->order,
                                       maximum_a,trial_k,&value)) return -1;
            if (value < best) {
                for (j = 0; j < f->order; ++j) f->coefficient[j] = coefficients[j];
                best = value; memcpy(best_k,trial_k,sizeof(best_k));
            }
        }
    }
    if (mlp_search_iir_evaluate(correlation,lag,NULL,0,maximum_a,trial_k,&value)) return -1;
    chosen = work.entry[selected].filter;
    if (value < weight*best) {
        memset(&chosen,0,sizeof(chosen)); best = value;
        memcpy(best_k,trial_k,sizeof(best_k));
    }
    value = best/correlation[0];
    if (!isfinite(value)) return -1;
    ++work.clock;
    *pool = work; *b = chosen;
    memcpy(reflection,best_k,sizeof(best_k)); *relative_error = value;
    return 0;
}

int mlp_search_joint(mlp_search_pool *pool, const double *correlation,
    unsigned lag, unsigned maximum_b, unsigned maximum_a,
    unsigned perturbations, double probability,
    mlp_wire_filter *a_result, mlp_wire_filter *b_result)
{
    mlp_search_pool work;
    mlp_wire_filter a = {0}, b;
    double r[50] = {0}, k[8], direct[8] = {0}, old[8];
    double relative, ratio, rho, factor = 0.25, feedback = 0, scale;
    uint32_t magnitude = 65536;
    unsigned i, m, order = 1, q = 3, threshold = 15, limit;
    if (!pool || !correlation || !a_result || !b_result || lag > 49 ||
        !isfinite(probability) || probability < 0 || probability > 1) return -1;
    work = *pool;
    memcpy(r,correlation,(lag+1)*sizeof(double)); r[0] += 10;
    if (mlp_search_pool_select(&work,r,lag,maximum_b,maximum_a,perturbations,
                              &b,k,&relative)) return -1;
    (void)relative; /* Order decision uses reflection product, not J/R0. */
    limit = 8-b.order; ratio = 1-k[0]*k[0];
    while (order < limit && ratio > ldexp(1.0,-2*(int)threshold)) {
        ++order; --threshold; ratio *= 1-k[order-1]*k[order-1];
    }
    while (order && k[order-1]*k[order-1] < 0.03) --order;
    for (i = order; i < 8; ++i) k[i] = 0;
    rho = k[0];
    for (m = 0; m < 8; ++m) {
        memcpy(old,direct,sizeof(old));
        for (i = 0; i < m; ++i) direct[i] = old[i]+k[m]*old[m-1-i];
        direct[m] = k[m];
    }
    for (i = 0; i < order+b.order; ++i) {
        double v = trunc(65536*fabs(i < order ? direct[i] : b.coefficient[i-order]));
        if (!isfinite(v) || v > INT32_MAX) return -1;
        if (v > magnitude) magnitude = (uint32_t)v;
    }
    if ((magnitude & UINT32_C(0xffffe000)) < UINT32_C(0x07ffc000)) {
        while (factor > ratio) {
            ++q; factor *= 0.25;
            if (q > 15) return -1;
            if ((magnitude >> (16-q)) >= 0x3ffe) break;
        }
    }
    scale = (double)(1u << q);
    for (i = 8; i > 0; --i) {
        double v = feedback*rho+scale*direct[i-1], z = floor(v+0.5);
        feedback = z-v; a.coefficient[i-1] = -z/scale;
    }
    while (order && a.coefficient[order-1] == 0) --order;
    a.order = order; a.precision = q; a.changed = order > 0;
    /* Preserve condition order: rand is consumed before magnitude test. */
    if (order+b.order < 7 && b.order > 0 && b.order < 4 && probability > 0 &&
        mlp_search_rand(&work.rng) < probability*32767 &&
        (magnitude >> (16-q)) < 5000) {
        for (i = a.order; i > 0; --i) a.coefficient[i] += 2*a.coefficient[i-1];
        a.coefficient[0] -= 2; ++a.order;
        for (i = b.order; i > 0; --i) b.coefficient[i] += 0.5*b.coefficient[i-1];
        b.coefficient[0] += 0.5; ++b.order; ++b.precision;
    }
    *pool = work; *a_result = a; *b_result = b;
    return 0;
}

int mlp_search_feedback_state(const int32_t pcm[40],
    const mlp_wire_filter *a, const mlp_wire_filter *b,
    unsigned attempts, int32_t state[4], unsigned *quantization)
{
    float e[40] = {0}, h[40] = {0};
    double matrix[5][37], scratch[5], lambda = 1;
    int32_t s[4] = {0}, output[4] = {0};
    unsigned n, j, k, t, p, trial;
    if (!pcm || !a || !b || !state || !quantization || a->order > 8 ||
        !b->order || b->order > 4 || !attempts) return -1;
    p = b->order;
    for (n = 0; n < 40; ++n)
        if (pcm[n] < -8388608 || pcm[n] > 8388607) return -1;
    for (j = 0; j < a->order; ++j) if (!isfinite(a->coefficient[j])) return -1;
    for (j = 0; j < p; ++j) if (!isfinite(b->coefficient[j])) return -1;
    h[7] = 1;
    for (n = 8; n < 40; ++n) {
        double v = (float)pcm[n], impulse = 0;
        for (j = 0; j < a->order; ++j) v -= a->coefficient[j]*(float)pcm[n-1-j];
        for (j = 0; j < p; ++j) {
            v -= b->coefficient[j]*e[n-1-j];
            impulse -= b->coefficient[j]*h[n-1-j];
        }
        e[n] = (float)v; h[n] = (float)impulse;
        if (!isfinite(e[n]) || !isfinite(h[n])) return -1;
    }
    for (trial = 0; trial < attempts; ++trial) {
        double maximum = 0, bits;
        unsigned q, invalid = 0;
        memset(matrix,0,sizeof(matrix));
        for (k = 0; k < p; ++k) matrix[k][0] = b->coefficient[p-1-k];
        matrix[p][0] = e[8];
        for (t = 1; t < 32; ++t) {
            matrix[0][t] = h[7+t]*b->coefficient[p-1];
            for (k = 1; k < p; ++k)
                matrix[k][t] = h[7+t]*b->coefficient[p-1-k]+matrix[k-1][t-1];
            matrix[p][t] = e[8+t];
        }
        for (k = 0; k <= p; ++k) matrix[k][32+k] = lambda;
        for (k = 0; k <= p; ++k) {
            for (j = k; j <= p; ++j) {
                scratch[j] = 0;
                for (t = 0; t < 32+p; ++t) scratch[j] += matrix[j][t]*matrix[k][t];
            }
            if (!isfinite(scratch[k]) || scratch[k] <= 0) {
                /* Exact zero target error is valid, only at final row. */
                if (k != p || scratch[k] != 0) return -1;
            }
            for (j = k+1; j <= p; ++j) {
                double ratio = scratch[j]/scratch[k];
                scratch[j] = ratio;
                for (t = 0; t < 32+p; ++t) matrix[j][t] -= ratio*matrix[k][t];
            }
            for (j = k; j <= p; ++j) matrix[k][j] = scratch[j];
        }
        for (k = 0; k < p; ++k) if (matrix[k][k] > maximum) maximum = matrix[k][k];
        bits = log1p(matrix[p][p]/(512*maximum))/log(4.0);
        if (!isfinite(bits) || bits < 0) return -1;
        q = bits >= 15 ? 15 : (unsigned)bits;
        for (k = p; k > 0; --k) {
            double v = matrix[k-1][p], z;
            for (j = k; j < p; ++j) v -= s[j]*matrix[k-1][j];
            z = floor(ldexp(v,-(int)q)+0.5)*ldexp(1.0,(int)q);
            if (!isfinite(z)) return -1;
            if (z < -8388607 || z > 8388607) { invalid = 1; break; }
            s[k-1] = (int32_t)z;
        }
        if (!invalid) {
            for (k = 0; k < p; ++k) output[k] = s[p-1-k];
            memcpy(state,output,sizeof(output)); *quantization = q; return 0;
        }
        lambda *= 2;
        if (!isfinite(lambda)) return -1;
    }
    return -1;
}

int mlp_search_fir(const double correlation[9], unsigned order,
    mlp_wire_filter *result)
{
    double r[9], k[8] = {0}, a[8] = {0}, old[8];
    double original, rho, factor = 0.25, feedback = 0, scale;
    uint32_t maximum = 65536;
    unsigned i, m, q = 3;
    mlp_wire_filter f = {0};
    if (!result || !valid_correlation(correlation, order)) return -1;
    memcpy(r, correlation, sizeof(r)); original = r[0]; r[0] += 10;
    if (reflect(r, order, k)) return -1;
    rho = k[0];
    while (order && k[order-1]*k[order-1] < 0.03) --order;
    for (i = order; i < 8; ++i) k[i] = 0;
    for (m = 0; m < 8; ++m) {
        memcpy(old, a, sizeof(old));
        for (i = 0; i < m; ++i) a[i] = old[i] + k[m]*old[m-1-i];
        a[m] = k[m];
    }
    for (i = 0; i < order; ++i) {
        double magnitude = trunc(65536*fabs(a[i]));
        if (!isfinite(magnitude) || magnitude > INT32_MAX) return -1;
        if (magnitude > maximum) maximum = (uint32_t)magnitude;
    }
    if ((maximum & UINT32_C(0xffffe000)) < UINT32_C(0x07ffc000)) {
        while (factor*original > r[0]) {
            ++q; factor *= 0.25;
            if (q > 15) return -1;
            if ((maximum >> (16-q)) >= 0x3ffe) break;
        }
    }
    scale = (double)(1u << q);
    for (i = 8; i > 0; --i) {
        double v = feedback*rho + scale*a[i-1];
        double z = floor(v+0.5);
        feedback = z-v; f.coefficient[i-1] = -z/scale;
    }
    while (order && f.coefficient[order-1] == 0) --order;
    f.order = order; f.precision = q; f.changed = order > 0;
    *result = f;
    return 0;
}
int mlp_search_interval(mlp_search_pool *pool,
    const int32_t *pcm, const size_t *lengths, size_t blocks,
    unsigned maximum_a, unsigned maximum_b, unsigned lag, unsigned mode_flags,
    mlp_search_plan *plan)
{
    mlp_search_plan work = {0}; mlp_search_pool candidates;
    double correlation[50] = {0}; size_t n,total = 0;
    unsigned order,quantization; int joint = maximum_b && !(mode_flags&2);
    if (!pcm || !lengths || !plan || !blocks || blocks > 128 ||
        maximum_a > 8 || maximum_b > 4 || lag > 49 || (joint && (!pool || lag < 8))) return -1;
    for (n = 0; n < blocks; ++n) {
        if (lengths[n] < 8 || lengths[n] > 160 || (lengths[n]&1)) return -1;
        total += lengths[n];
    }
    if (joint) {
        if (total < 40) return -1;
        candidates = *pool;
        if (mlp_search_correlation_extended(pcm,lengths,blocks,lag,correlation) ||
            mlp_search_joint(&candidates,correlation,lag,maximum_b,maximum_a,16,0.001,&work.a,&work.b)) return -1;
        if (work.b.order) {
            if (mlp_search_feedback_state(pcm,&work.a,&work.b,64,work.state.value,&quantization)) return -1;
            work.state.count = work.b.order; work.state.changed = 1;
        }
    } else {
        order = (mode_flags&2) ? 4 : 8;
        if (maximum_a < order) order = maximum_a;
        if (mlp_search_correlation_extended(pcm,lengths,blocks,order,correlation) ||
            mlp_search_fir(correlation,order,&work.a)) return -1;
    }
    work.a.changed = work.b.changed = 1;
    if (joint) *pool = candidates;
    *plan = work; return 0;
}
