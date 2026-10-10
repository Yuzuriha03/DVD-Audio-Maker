/* Acceptance oracle uses the unchanged native adapter, not Rust coefficients. */
#include <math.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include "mlp_encoder.h"
#include "mlp_format.h"
static unsigned host_current_fp(void) { return 0; }
static unsigned codec_fp(void) { return 0; }
static void host_fp(unsigned value) { (void)value; }
static int encode_stream_common(const mlp_encoder_config *c, mlp_encoder_read r, void *i,
    mlp_encoder_write w, void *o, mlp_encoder_result *result, unsigned assignment,
    unsigned bits, unsigned rate) {
    (void)c;(void)r;(void)i;(void)w;(void)o;(void)result;(void)assignment;(void)bits;(void)rate;
    return -1;
}
#include "mlp_group_input.inc"
typedef struct source { const int32_t *pcm; size_t at, frames, channels; } source;
static int pull(void *opaque, int32_t *pcm, size_t capacity, size_t *frames) {
    source *s=opaque;
    size_t n=s->frames-s->at;
    if(n>capacity) n=capacity;
    if(n>13) n=13;
    memcpy(pcm,s->pcm+s->at*s->channels,n*s->channels*sizeof(*pcm));
    s->at+=n;*frames=n;return 0;
}
__declspec(dllexport) int group_reference(unsigned rate,unsigned bits,unsigned assignment,
    unsigned rate2,unsigned bits2,size_t frames,const int32_t *a,const int32_t *b,int32_t *output) {
    mlp_group_input s={0}; source x,y; unsigned ch; size_t at=0;
    if(mlp_format_init_assignment(&s.format,rate,bits,assignment) ||
       mlp_format_set_group2_bits(&s.format,bits2) || mlp_format_set_group2_rate(&s.format,rate2)) return -1;
    s.frames1=frames;s.ratio=rate/rate2;s.frames2=frames/s.ratio;
    x=(source){a,0,frames,s.format.group1_channels};y=(source){b,0,s.frames2,s.format.group2_channels};
    s.read1=pull;s.read2=pull;s.input1=&x;s.input2=&y;
    for(ch=0;ch<s.format.group2_channels;++ch) s.rng[ch]=(((s.format.group1_channels+ch)*214013u+2531011u)>>16)&32767;
    while(at<frames) {
        size_t got=0,capacity=frames-at;if(capacity>80) capacity=80;
        if(group_read(&s,output+at*s.format.channels,capacity,&got) || !got) return -1;
        at+=got;
    }
    return 0;
}
