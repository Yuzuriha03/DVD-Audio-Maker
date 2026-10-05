/* Native-rate group adapter for the synchronous encoder. Kept in this
 * translation unit to share the codec/callback FP scopes and encoder state. */
#include "mlp_upsample_coefficients.inc"
typedef struct mlp_group_input {
    mlp_encoder_read read1,read2;void *input1,*input2;
    mlp_format format;uint64_t frames1,frames2,position,loaded;
    unsigned ratio,caller_fp;int32_t ring[512*4];uint32_t rng[4];
} mlp_group_input;
static int group_call(mlp_group_input *s,mlp_encoder_read read,void *input,
    int32_t *pcm,size_t capacity,size_t *frames)
{
    int rc;host_fp(s->caller_fp);rc=read(input,pcm,capacity,frames);host_fp(s->caller_fp);return rc;
}
static int group_load(mlp_group_input *s,uint64_t need)
{
    int32_t buffer[160*4];unsigned channels=s->format.group2_channels;
    if(need>s->frames2) need=s->frames2;
    while(s->loaded<need) {
        size_t capacity=(size_t)(need-s->loaded),got=0,n;unsigned ch;
        uint32_t mask=(UINT32_C(1)<<(24-s->format.group2_bits))-1;
        if(capacity>160) capacity=160;
        if(group_call(s,s->read2,s->input2,buffer,capacity,&got) || !got || got>capacity) return -1;
        for(n=0;n<got;++n) for(ch=0;ch<channels;++ch) {
            int32_t value=buffer[n*channels+ch];
            if(value<-8388608 || value>8388607 || ((uint32_t)value&mask)) return -1;
            s->ring[((s->loaded+n)%512)*channels+ch]=value;
        }
        s->loaded+=got;
    }
    return 0;
}
static int32_t group_sample(const mlp_group_input *s,int64_t frame,unsigned channel)
{
    if(frame<0 || (uint64_t)frame>=s->frames2) return 0;
    return s->ring[((uint64_t)frame%512)*s->format.group2_channels+channel];
}
static int32_t group_midpoint(mlp_group_input *s,int64_t frame,unsigned channel)
{
    double value=0,dither;int64_t quantized;unsigned i,bits=s->format.group2_bits;
    uint32_t u=(uint32_t)((int32_t)s->rng[channel]>>7);
    for(i=0;i<103;++i)
        value+=(double)(group_sample(s,frame-i,channel)+group_sample(s,frame+i+1,channel))*mlp_upsample_coefficients[i];
    s->rng[channel]=((((s->rng[channel]&127)<<11)^u)<<5)^u;
    dither=ldexp((double)((int8_t)(u>>8)+(int8_t)u),16-(int)bits);
    quantized=(int64_t)floor(ldexp(value+dither,(int)bits-24)+0.5)*(INT64_C(1)<<(24-bits));
    if(quantized<-8388608) quantized=-8388608;
    if(quantized>8388608-(INT64_C(1)<<(24-bits))) quantized=8388608-(INT64_C(1)<<(24-bits));
    return (int32_t)quantized;
}
static int MLP_ENCODER_CALL group_read(void *opaque,int32_t *pcm,size_t capacity,size_t *frames)
{
    mlp_group_input *s=opaque;int32_t primary[160*4];size_t count,done=0,n;
    unsigned first=s->format.group1_channels,second=s->format.group2_channels,ch,caller;
    uint64_t last;
    if(capacity>160 || !capacity) return -1;
#ifdef _WIN32
    caller=host_current_fp();
#else
    caller=0;
#endif
    s->caller_fp=caller;
    count=s->frames1-s->position<capacity?(size_t)(s->frames1-s->position):capacity;
    if(!count) {*frames=0;return 0;}
    while(done<count) {
        size_t got=0;
        if(group_call(s,s->read1,s->input1,primary+done*first,count-done,&got) || !got || got>count-done) return -1;
        done+=got;
    }
    last=(s->position+count-1)/s->ratio;
    if(group_load(s,last+1+(s->ratio==2?103:0))) return -1;
    host_fp(codec_fp());
    for(n=0;n<count;++n) {
        uint64_t at=s->position+n;int64_t low=(int64_t)(at/s->ratio);
        for(ch=0;ch<first;++ch)
            pcm[n*s->format.channels+s->format.input_order[ch]]=primary[n*first+ch];
        for(ch=0;ch<second;++ch)
            pcm[n*s->format.channels+s->format.input_order[first+ch]]=
                s->ratio==2 && (at&1) ? group_midpoint(s,low,ch) : group_sample(s,low,ch);
    }
    host_fp(caller);s->position+=count;*frames=count;return 0;
}
int MLP_ENCODER_CALL mlp_encode_stream_groups(const mlp_encoder_groups_config *c,
    mlp_encoder_read read1,void *input1,mlp_encoder_read read2,void *input2,
    mlp_encoder_write write,void *output,mlp_encoder_result *result)
{
    mlp_format format;mlp_group_input *source;unsigned ratio,ch;int status;
    if(!result) return MLP_ENCODER_INVALID;
    memset(result,0,sizeof(*result));result->status=MLP_ENCODER_INVALID;
    if(!c || c->struct_size!=sizeof(*c) || !read1 || !read2 || !write ||
       c->primary.struct_size!=sizeof(c->primary) || c->primary.abi_version!=MLP_ENCODER_ABI_VERSION ||
       !c->primary.frames || c->primary.frames>UINT32_MAX*UINT64_C(160) ||
       mlp_format_init_assignment(&format,c->primary.sample_rate,c->primary.bits,c->assignment) ||
       format.channels!=c->primary.channels || mlp_format_set_group2_bits(&format,c->group2_bits) ||
       mlp_format_set_group2_rate(&format,c->group2_sample_rate)) goto invalid_group;
    ratio=format.sample_rate/format.group2_sample_rate;
    if(c->primary.frames%ratio || c->group2_frames!=c->primary.frames/ratio) goto invalid_group;
    source=calloc(1,sizeof(*source));
    if(!source) {result->status=MLP_ENCODER_MEMORY;snprintf(result->error,sizeof(result->error),"Group-input allocation failed");return result->status;}
    source->read1=read1;source->read2=read2;source->input1=input1;source->input2=input2;
    source->format=format;source->frames1=c->primary.frames;source->frames2=c->group2_frames;source->ratio=ratio;
    for(ch=0;ch<format.group2_channels;++ch)
        source->rng[ch]=(((format.group1_channels+ch)*214013u+2531011u)>>16)&32767;
    status=encode_stream_common(&c->primary,group_read,source,write,output,result,
                                c->assignment,c->group2_bits,c->group2_sample_rate);
    free(source);return status;
invalid_group:
    snprintf(result->error,sizeof(result->error),"Invalid DVD-Audio groups, precision, native rates or unequal durations");
    return result->status;
}
