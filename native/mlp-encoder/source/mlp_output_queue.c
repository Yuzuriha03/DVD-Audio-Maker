#include "mlp_output_queue.h"
#include <stdlib.h>
#include <string.h>
void mlp_output_queue_init(mlp_output_queue *q,mlp_output_emit emit,void *opaque)
{
    if(!q) return;
    memset(q,0,sizeof(*q)); mlp_output_timing_init(&q->timing);
    q->emit=emit; q->opaque=opaque;
}
void mlp_output_queue_dispose(mlp_output_queue *q)
{
    unsigned i; if(!q) return;
    for(i=0;i<=MLP_OUTPUT_LOOKAHEAD;++i) { free(q->nodes[i].words); q->nodes[i].words=NULL; }
    q->count=0; q->finished=1;
}
static int release(mlp_output_queue *q,const mlp_output_descriptor *next)
{
    mlp_output_node *node=q->nodes+q->read;
    if(mlp_output_timing_step(&q->timing,next,&node->descriptor)!=0) return 0;
    node->words[0]=node->descriptor.parity_length; node->words[1]=node->descriptor.arrival;
    if(!q->emit(q->opaque,node->words,node->count)) return 0;
    free(node->words);node->words=NULL;
    q->read=(q->read+1)%(MLP_OUTPUT_LOOKAHEAD+1); --q->count; return 1;
}
int mlp_output_queue_push(mlp_output_queue *q,const uint32_t *words,size_t count,unsigned samples)
{
    mlp_output_node *node; mlp_output_descriptor d={0},empty={0};
    unsigned rate,substreams,major,directory,first,last;
    if(!q || !words || !q->emit || q->failed || q->finished || !samples || samples>160 ||
       count<4 || count>4095 || (words[0]&4095)!=count || q->count>MLP_OUTPUT_LOOKAHEAD) return 0;
    major=count>=4 && words[2]==0xf872 && words[3]==0x6fbb;
    rate=q->rate;substreams=q->substreams;directory=major ? 16 : 2;
    if(major) {
        if(count<18) return 0;
        rate=words[9];substreams=words[10]>>12;
    }
    if(!(rate&0x7fff) || substreams<1 || substreams>2 || count<=directory+substreams) return 0;
    first=words[directory]&4095;last=words[directory+substreams-1]&4095;
    if(!first || first>last || last+directory+substreams!=count) return 0;
    d.rate=rate;d.samples=samples;d.decode=q->decode;d.words=(uint32_t)count;
    d.group0_words=first;d.group1_words=last-first;
    d.arrival=words[1];d.parity_length=words[0];
    node=q->nodes+q->write; node->words=malloc(count*sizeof(*words));
    if(!node->words) { q->failed=1; return 0; }
    memcpy(node->words,words,count*sizeof(*words));node->count=count;node->descriptor=d;
    q->rate=rate;q->substreams=substreams;q->decode=(q->decode+samples)&65535;
    q->write=(q->write+1)%(MLP_OUTPUT_LOOKAHEAD+1);++q->count;
    if(q->primed<MLP_OUTPUT_LOOKAHEAD) {
        if(mlp_output_timing_step(&q->timing,&d,&empty)!=0) { q->failed=1;return 0; }
        ++q->primed;
    } else if(!release(q,&d)) { q->failed=1;return 0; }
    return 1;
}
int mlp_output_queue_finish(mlp_output_queue *q)
{
    mlp_output_descriptor empty={0};
    if(!q || !q->emit || q->failed || q->finished) return 0;
    while(q->primed<MLP_OUTPUT_LOOKAHEAD) {
        if(mlp_output_timing_step(&q->timing,&empty,&empty)!=0) { q->failed=1;return 0; }
        ++q->primed;
    }
    while(q->count) if(!release(q,&empty)) { q->failed=1;return 0; }
    q->finished=1;return 1;
}
