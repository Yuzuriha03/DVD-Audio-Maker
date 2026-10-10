#define MLP_ENCODER_LIBRARY
#include "encode_file.c"

typedef struct memory_pcm { const int32_t *pcm;size_t position,frames;unsigned channels; } memory_pcm;
static int read_memory(void *opaque,int32_t *out,size_t capacity,size_t *frames) {
    memory_pcm *m=opaque;size_t count=m->frames-m->position;
    if(count>capacity) count=capacity;
    memcpy(out,m->pcm+m->position*m->channels,count*m->channels*sizeof(*out));
    m->position+=count;*frames=count;return 0;
}
MLP_BITS_API int prepare_reference(const int32_t *pcm,size_t frames,unsigned channels,unsigned bits,
    unsigned au_samples,unsigned interval,unsigned options,int32_t *out,uint8_t *bypass,
    unsigned *lengths,unsigned *checks,unsigned *state,mlp_matrix_primitive *matrix,
    mlp_matrix_analysis *analysis,mlp_scale_plan *scale,mlp_search_plan *prediction,
    mlp_search_pool *pools) {
    matrix_interval *m=calloc(1,sizeof(*m));mlp_pcm in={0};memory_pcm memory={pcm,0,frames,channels};
    unsigned truncated=0,ch;int result;
    if(!m) return -1;
    in.format.channels=channels;in.format.bits=bits;in.format.au_samples=au_samples;
    in.format.group1_channels=channels;in.format.group2_bits=bits;
    for(ch=0;ch<channels;++ch) in.format.input_order[ch]=ch;
    in.remaining=frames;in.read_callback=read_memory;in.read_opaque=&memory;
    m->original_scale=options&1;m->enable_matrix=(options>>1)&1;m->joint_search=(options>>2)&1;
    m->search_rng=1;mlp_matrix_analysis_init(&m->analysis,channels);
    for(ch=0;ch<6;++ch) {mlp_search_pool_init(m->pool+ch,(options&16)?m->search_rng:1);if(options&16)m->search_rng=m->pool[ch].rng;}
    for(ch=0;ch<= (options>>8);++ch) {
        result=prepare_matrix(&in,m,interval,(options>>3)&1,&truncated);
        if(!result) break;
    }
    memcpy(out,m->pcm,m->analysis.samples*channels*sizeof(*out));
    memcpy(bypass,m->bypass,m->analysis.samples);
    memcpy(lengths,m->lengths,m->queued*sizeof(*lengths));memcpy(checks,m->checks,m->queued*sizeof(*checks));
    state[0]=m->queued;state[1]=m->slot;state[2]=m->offset;state[3]=m->bits;state[4]=m->count;
    state[5]=m->selected_matrix;state[6]=m->search_rng;state[7]=truncated;state[8]=(unsigned)memory.position;
    for(ch=0;ch<6;++ch) state[9+ch]=m->search_ready[ch];
    memcpy(matrix,m->matrix,sizeof(m->matrix));*analysis=m->analysis;*scale=m->scale;
    memcpy(prediction,m->prediction,sizeof(m->prediction));memcpy(pools,m->pool,sizeof(m->pool));
    free(m);return result;
}
MLP_BITS_API int prepare_decorrelate(const double *covariance,const double *ratio,unsigned channels,unsigned *order,double *transform,double *reduced) {
    return mlp_matrix_decorrelate(covariance,ratio,channels,0,10,order,transform,reduced);
}
typedef struct flush_capture { uint32_t *words;size_t count,capacity;int fail; } flush_capture;
static int capture_flush(void *opaque,const uint32_t *words,size_t count) {
    flush_capture *c=opaque;
    if(c->fail || count>c->capacity-c->count) return 0;
    memcpy(c->words+c->count,words,count*sizeof(*words));c->count+=count;return 1;
}
MLP_BITS_API int flush_reference(const mlp_parameters *parameters,const mlp_restart *restart,
    const int32_t *samples,unsigned channels,unsigned count,unsigned aus,unsigned width,
    unsigned sample_rate,unsigned rate_field,const uint32_t *major,unsigned stamp,unsigned fail,
    uint32_t *words,size_t capacity,uint64_t *state) {
    interval_output *pending=calloc(1,sizeof(*pending));mlp_output_queue *queue=calloc(1,sizeof(*queue));
    mlp_format format={0};mlp_rate_state rate;flush_capture capture={words,0,capacity,(int)fail};
    unsigned au,i;uint64_t total=0;int result,finished;
    if(!pending || !queue) {free(pending);free(queue);return -1;}
    format.channels=channels;format.sample_rate=sample_rate;format.rate_field=rate_field;
    /* Use the real format generator rather than reproducing its checksum. */
    if(mlp_format_init(&format,sample_rate,24,channels)) {free(pending);free(queue);return -1;}
    (void)major;format.rate_field=rate_field;
    pending->restart=*restart;pending->aus=aus;pending->maximum_lsbs=width;
    for(au=0;au<aus;++au) {
        planned_au *p=pending->au+au;
        p->initial=p->main=p->previous=*parameters;
        p->count=count;p->single_restart=count<16;p->end_markers=au+1==aus;p->stamp=stamp;
        for(i=0;i<count*channels;++i) p->residual[i]=p->unpredicted[i]=samples[au*count*channels+i];
    }
    mlp_rate_init(&rate);mlp_output_queue_init(queue,capture_flush,&capture);
    result=flush_interval(queue,pending,&format,&rate,&total);
    finished=mlp_output_queue_finish(queue);
    state[0]=total;state[1]=rate.arrival;state[2]=rate.decode;state[3]=pending->aus;
    state[4]=pending->maximum_lsbs;state[5]=pending->restart.maximum_lsbs;state[6]=capture.count;
    state[7]=finished;state[8]=pending->failure?1:0;
    mlp_output_queue_dispose(queue);free(queue);free(pending);return result;
}
MLP_BITS_API int flush_major(unsigned rate,unsigned channels,unsigned rate_field,uint32_t *major) {
    mlp_format format;uint16_t words[14];unsigned i;
    if(mlp_format_init(&format,rate,24,channels)) return -1;
    format.rate_field=rate_field;mlp_format_major(&format,words);
    for(i=0;i<14;++i) major[i]=words[i];return 0;
}
MLP_BITS_API int encode_reference(const int32_t *samples,size_t frames,unsigned channels,unsigned bits,
    unsigned interval,unsigned cycle,unsigned options,uint32_t *words,size_t capacity,size_t *written) {
    mlp_pcm in={0};memory_pcm memory={samples,0,frames,channels};
    interval_output *pending=calloc(1,sizeof(*pending));matrix_interval *matrix=calloc(1,sizeof(*matrix));
    mlp_output_queue *queue=calloc(1,sizeof(*queue));flush_capture capture={words,0,capacity,0};int result;
    const uint8_t packet[5]={0,1,0x40,0,0xff};mlp_stamp_record record={0,5,packet,0};mlp_stamp_state stamp;
    uint64_t aus=(frames+39)/40;
    if(!pending || !matrix || !queue || mlp_format_init(&in.format,48000,bits,channels)) {free(pending);free(matrix);free(queue);return -1;}
    in.frames=in.remaining=frames;in.read_callback=read_memory;in.read_opaque=&memory;
    {unsigned ch;for(ch=0;ch<channels;++ch) in.format.input_order[ch]=ch;}
    matrix->original_scale=(options>>1)&1;matrix->enable_matrix=(options>>2)&1;matrix->joint_search=(options>>3)&1;
    mlp_output_queue_init(queue,capture_flush,&capture);
    if(!mlp_stamp_init(&stamp,&record,1,aus)) {free(pending);free(matrix);free(queue);return -1;}
    result=encode(&in,NULL,interval,cycle,options&1,(options&16)?matrix:NULL,(options>>5)&1,pending,queue,&stamp);
    *written=capture.count;mlp_output_queue_dispose(queue);free(queue);free(matrix);free(pending);return result;
}
MLP_BITS_API int encode_major(unsigned bits,unsigned channels,uint32_t *major) {
    mlp_format format;uint16_t words[14];unsigned i;
    if(mlp_format_init(&format,48000,bits,channels)) return -1;
    mlp_format_major(&format,words);for(i=0;i<14;++i) major[i]=words[i];return 0;
}
