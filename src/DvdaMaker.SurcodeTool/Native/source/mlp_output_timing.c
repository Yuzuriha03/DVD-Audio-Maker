#include "mlp_output_timing.h"
#include <string.h>
static int32_t signed32(uint32_t v)
{ return v<=INT32_MAX ? (int32_t)v : (int32_t)((int64_t)v-INT64_C(4294967296)); }
static int32_t add32(int32_t a,int32_t b)
{ return signed32((uint32_t)a+(uint32_t)b); }
static mlp_output_pair combine(mlp_output_pair a,mlp_output_pair b)
{
    mlp_output_pair r; int32_t candidate=add32(a.total,b.minimum);
    r.minimum=a.minimum<candidate ? a.minimum : candidate;
    r.total=add32(a.total,b.total); return r;
}
void mlp_output_timing_init(mlp_output_timing *s)
{
    unsigned i;
    if(!s) return;
    memset(s,0,sizeof(*s)); s->cursor=MLP_OUTPUT_LOOKAHEAD-1;
    s->aggregate.minimum=INT32_MAX; s->previous_delay=INT32_MIN;
    for(i=0;i<MLP_OUTPUT_LOOKAHEAD;++i) s->tree[i].minimum=INT32_MAX;
}
static unsigned duration(const mlp_output_descriptor *d)
{
    unsigned rate=d->rate&0x7fff;
    unsigned result=(d->words*256+rate-1)/rate;
    return result<d->samples/4 ? d->samples/4 : result;
}
static int32_t buffer_delay(mlp_output_timing *s,const mlp_output_descriptor *d,int32_t delay)
{
    unsigned write=s->write,read=s->read,i; int full,expired;
    s->times[write]=d->decode+d->samples+1;
    s->sizes[write][0]=d->words; s->sizes[write][1]=d->group0_words;
    s->sizes[write][2]=d->group1_words;
    for(i=0;i<3;++i) s->totals[i]+=s->sizes[write][i];
    write=(write+1)%100;
    if(write==read) { s->write=s->read=0; memset(s->totals,0,sizeof(s->totals)); }
    do {
        full=s->totals[0]>=45000 || (d->group1_words &&
            (s->totals[1]>=15000 || s->totals[2]>=30000));
        expired=write!=read && ((d->decode-s->times[read]+(uint32_t)delay)&0xffff)<=0x7fff;
        if(!expired) {
            if(!full) break;
            delay=(int32_t)((s->times[read]-d->decode)&0xffff)-65536;
        }
        for(i=0;i<3;++i) s->totals[i]-=s->sizes[read][i];
        read=(read+1)%100;
    } while(expired || full);
    s->write=write; s->read=read; return delay;
}
int mlp_output_timing_step(mlp_output_timing *s,const mlp_output_descriptor *next,mlp_output_descriptor *d)
{
    mlp_output_pair pair,range; int32_t lower,upper,previous;
    uint32_t arrival,delta; int failed=0;
    if(!s || !next || !d || s->cursor < -1 || s->cursor>=MLP_OUTPUT_LOOKAHEAD ||
       (next->words && (!(next->rate&0x7fff) || !next->samples || next->samples>160 || next->words>4095)) ||
       (d->words && (!(d->rate&0x7fff) || !d->samples || d->samples>160 || d->words>4095))) return -1;
    pair.total=next->words ? (int32_t)next->samples-(int32_t)duration(next) : 80;
    pair.minimum=pair.total<0 ? pair.total : 0;
    if(s->cursor<0) {
        unsigned i; s->aggregate=pair;
        for(i=1;i<MLP_OUTPUT_LOOKAHEAD;++i) s->tree[i]=combine(s->tree[i],s->tree[i-1]);
        s->cursor=MLP_OUTPUT_LOOKAHEAD-1;
        range=combine(s->tree[s->cursor],s->aggregate);
    } else {
        s->tree[s->cursor]=pair; --s->cursor;
        s->aggregate=combine(s->aggregate,pair);
        range=s->cursor<0 ? s->aggregate : combine(s->tree[s->cursor],s->aggregate);
    }
    if(!d->words) { s->previous_delay=INT32_MIN; return 0; }
    upper=add32(range.minimum,-(int32_t)d->samples);
    lower=add32(range.total,-(int32_t)d->samples*90);
    if(lower<-(int32_t)d->samples*90) lower=-(int32_t)d->samples*90;
    previous=buffer_delay(s,d,s->previous_delay);
    if(upper<previous) { failed=1; lower=upper; }
    else {
        if(lower<previous) lower=previous;
        if(lower>upper) lower=upper;
    }
    s->previous_delay=add32(lower,(int32_t)duration(d)-(int32_t)d->samples);
    if(s->previous_delay<-(int32_t)d->samples*90) s->previous_delay=-(int32_t)d->samples*90;
    arrival=(d->decode+(uint32_t)lower)&0xffff;
    if(!(d->rate&0x8000) && (int32_t)d->rate>0) arrival=(arrival-15/d->rate-1)&0xffff;
    delta=d->arrival^arrival; delta^=signed32(delta)>>8;
    d->parity_length^=(((delta&15)<<4)^(delta&240))<<8;
    d->arrival=arrival; return failed;
}
