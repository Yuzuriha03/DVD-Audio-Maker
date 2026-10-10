#include "mlp_output_queue.h"
#include "mlp_stamp.h"
#include <stdlib.h>
MLP_BITS_API int stamp_trace(const mlp_stamp_record *records,size_t count,uint64_t aus,unsigned *bits,unsigned *complete) {
    mlp_stamp_state s; uint64_t i;
    if(!mlp_stamp_init(&s,records,count,aus)) return -1;
    for(i=0;i<=aus;++i) {
        unsigned bit=99;
        int ok=mlp_stamp_next(&s,&bit);
        bits[i]=ok?bit:99;
        complete[i]=mlp_stamp_complete(&s);
    }
    return 0;
}
MLP_BITS_API mlp_output_queue *queue_create(mlp_output_emit emit,void *opaque) {
    mlp_output_queue *q=malloc(sizeof(*q));
    if(q) mlp_output_queue_init(q,emit,opaque);
    return q;
}
MLP_BITS_API void queue_destroy(mlp_output_queue *q) {
    if(q) { mlp_output_queue_dispose(q); free(q); }
}
MLP_BITS_API void queue_state(const mlp_output_queue *q,unsigned *s) {
    s[0]=q->read;s[1]=q->write;s[2]=q->count;s[3]=q->primed;
    s[4]=q->rate;s[5]=q->substreams;s[6]=q->decode;
    s[7]=q->failed;s[8]=q->finished;
}
