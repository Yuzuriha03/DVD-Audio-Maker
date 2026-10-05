#ifndef MLP_OUTPUT_QUEUE_H
#define MLP_OUTPUT_QUEUE_H
#include "mlp_output_timing.h"
typedef int (*mlp_output_emit)(void *opaque,const uint32_t *words,size_t count);
typedef struct mlp_output_node {
    mlp_output_descriptor descriptor; uint32_t *words; size_t count;
} mlp_output_node;
typedef struct mlp_output_queue {
    mlp_output_timing timing;
    mlp_output_node nodes[MLP_OUTPUT_LOOKAHEAD+1];
    unsigned read,write,count,primed,rate,substreams,decode;
    int failed,finished; mlp_output_emit emit; void *opaque;
} mlp_output_queue;
MLP_BITS_API void mlp_output_queue_init(mlp_output_queue *queue,mlp_output_emit emit,void *opaque);
MLP_BITS_API int mlp_output_queue_push(mlp_output_queue *queue,const uint32_t *words,size_t count,unsigned samples);
MLP_BITS_API int mlp_output_queue_finish(mlp_output_queue *queue);
MLP_BITS_API void mlp_output_queue_dispose(mlp_output_queue *queue);
#endif
