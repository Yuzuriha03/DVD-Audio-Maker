#ifndef MLP_OUTPUT_TIMING_H
#define MLP_OUTPUT_TIMING_H
#include "mlp_bits.h"
#define MLP_OUTPUT_LOOKAHEAD 1200
typedef struct mlp_output_pair { int32_t minimum,total; } mlp_output_pair;
typedef struct mlp_output_descriptor {
    uint32_t rate,samples,decode,words,group0_words,group1_words;
    uint32_t arrival,parity_length;
} mlp_output_descriptor;
typedef struct mlp_output_timing {
    mlp_output_pair tree[MLP_OUTPUT_LOOKAHEAD],aggregate;
    int cursor; int32_t previous_delay;
    unsigned write,read;
    uint32_t times[100],sizes[100][3],totals[3];
} mlp_output_timing;
/* The caller
 * supplies the next lookahead descriptor and the AU being released.
 * Empty descriptors have words=0. No timestamps come from reference files. */
MLP_BITS_API void mlp_output_timing_init(mlp_output_timing *state);
MLP_BITS_API int mlp_output_timing_step(mlp_output_timing *state,
    const mlp_output_descriptor *lookahead,mlp_output_descriptor *current);
#endif
