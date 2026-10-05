#ifndef MLP_SEARCH_H
#define MLP_SEARCH_H
#include "mlp_parameters.h"
/* Independent ABI. Source: 10008790/10008900/10008a10/10009f50.
 * Double arithmetic models mlpencoder math, not exact x87 execution.
 * Blocks are contiguous mono signed24 PCM, lengths 8..160, even.
 * A single block uses only the rising window, as in the original.
 * Invalid calls leave outputs unchanged. */
MLP_BITS_API int mlp_search_correlation(const int32_t *pcm,
    const size_t *lengths, size_t blocks, unsigned order, double result[9]);
MLP_BITS_API int mlp_search_correlation_extended(const int32_t *pcm,
    const size_t *lengths, size_t blocks, unsigned order, double result[50]);
MLP_BITS_API int mlp_search_reflection(const double correlation[9],
    unsigned order, double reflection[8], double *error);
MLP_BITS_API int mlp_search_fir(const double correlation[9],
    unsigned order, mlp_wire_filter *result);
/* Original 10008e40 candidate restriction, not generic stability alone.
 * Returns 1 accepted, 0 rejected, -1 invalid (order must be 1..4). */
MLP_BITS_API int mlp_search_iir_stable(const double coefficient[4], unsigned order);
/* 10008cd0: correlation has lag+1 entries, lag <=49. Caller supplies
 * regularization (original search adds 10 to R0 before evaluation).
 * B order 0..4, A order min(max_fir_order,8-Border).
 * Returns reflections and J=transformed prediction error+2*impulse energy.
 * Does not perform B stability screening or coefficient quantization. */
MLP_BITS_API int mlp_search_iir_evaluate(const double *correlation,
    unsigned lag, const double *coefficient, unsigned order,
    unsigned max_fir_order, double reflection[8], double *score);
/* MSVC rand recurrence, explicit independent state (not global CRT state). */
MLP_BITS_API unsigned mlp_search_rand(uint32_t *state);
/* 10008f60: requested order 0 means randomized, capped by maximum 1..4.
 * Explicit retry budget; failures leave state/output unchanged. */
MLP_BITS_API int mlp_search_iir_random(uint32_t *state, unsigned requested,
    unsigned maximum, unsigned attempts, mlp_wire_filter *result);
#define MLP_SEARCH_POOL_MAX 18
typedef struct mlp_search_entry {
    mlp_wire_filter filter;
    uint32_t timestamp;
    unsigned fixed, no_perturb;
} mlp_search_entry;
typedef struct mlp_search_pool {
    mlp_search_entry entry[MLP_SEARCH_POOL_MAX];
    unsigned count;
    uint32_t clock, rng;
} mlp_search_pool;
/* Original default six fixed seeds plus twelve randomized entries.
 * RNG seed supplied explicitly; original shared CRT state/call order is not
 * reproduced by independent per-channel pools. No wall-clock seed is assumed. */
MLP_BITS_API int mlp_search_pool_init(mlp_search_pool *pool, uint32_t seed);
/* 100093d0 retained-index age rule, including slot-zero fallback. */
MLP_BITS_API unsigned mlp_search_pool_replacement(const mlp_search_pool *pool,
    unsigned order);
/* Caller supplies validated pool entries.
 * 10009210 selection, including its retained-index age quirk.
 * Correlation includes caller's +10 regularization. Atomic failure. */
MLP_BITS_API int mlp_search_pool_select(mlp_search_pool *pool,
    const double *correlation, unsigned lag, unsigned maximum_b,
    unsigned maximum_a, unsigned perturbations, mlp_wire_filter *b,
    double reflection[8], double *relative_error);
/* 10009670: +10 regularization, persistent selection, joint order/precision
 * and optional A/B transform. Default original probability is 0.001.
 * Correlation is not modified; pool and outputs commit atomically. */
MLP_BITS_API int mlp_search_joint(mlp_search_pool *pool,
    const double *correlation, unsigned lag, unsigned maximum_b,
    unsigned maximum_a, unsigned perturbations, double probability,
    mlp_wire_filter *a, mlp_wire_filter *b);
/* 100099f0: first 8 samples are FIR history, next 32 fit feedback state.
 * Output states newest first. Explicit retry budget is a safety extension.
 * Invalid/nonfinite/out-of-range failures leave outputs unchanged. */
MLP_BITS_API int mlp_search_feedback_state(const int32_t pcm[40],
    const mlp_wire_filter *a, const mlp_wire_filter *b,
    unsigned attempts, int32_t state[4], unsigned *quantization);
typedef struct mlp_search_plan {
    mlp_wire_filter a,b;
    mlp_wire_state state;
} mlp_search_plan;
/* 1000a2e0 selection orchestration with explicit limits/pool. Mode flag 2
 * forces FIR and caps its order at 4; otherwise maximum_b=0 uses FIR <=8.
 * Joint mode uses lag 8..49 and first-40 feedback fitting. Caller must
 * reapply the selected filters to each AU, including after local overflow.
 * Blocks must be even, 8..160. Joint mode requires total >=40.
 * Floating arithmetic is the mlpencoder model, not an x87 identity claim. */
MLP_BITS_API int mlp_search_interval(mlp_search_pool *pool,
    const int32_t *pcm, const size_t *lengths, size_t blocks,
    unsigned maximum_a, unsigned maximum_b, unsigned lag, unsigned mode_flags,
    mlp_search_plan *plan);
#endif
