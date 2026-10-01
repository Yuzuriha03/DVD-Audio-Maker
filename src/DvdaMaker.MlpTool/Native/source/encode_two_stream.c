/* Two-substream application: stereo downmix + full channel output.
 * wire writers; independent fixed-16-AU, unpredicted policy.
 * Optional original downmix design/scale/application with explicit matrices. */
#define _POSIX_C_SOURCE 200809L
#include "mlp_pcm.h"
#include "mlp_substream.h"
#include "mlp_rate.h"
#include "mlp_matrix.h"
#include "mlp_scale.h"
#include "mlp_search.h"
#include "mlp_predict.h"
#include <ctype.h>
#include <math.h>
#include <fcntl.h>
#include <float.h>
#include <io.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

static int put_word(FILE *file,unsigned value)
{
    return fputc((value>>8)&255,file)!=EOF && fputc(value&255,file)!=EOF;
}
static unsigned lossless(unsigned check,const int32_t *pcm,size_t count,unsigned stride,unsigned channels)
{
    size_t n; unsigned ch;
    for(n=0;n<count;++n) for(ch=0;ch<channels;++ch) {
        uint32_t v=((uint32_t)pcm[n*stride+ch]&0xffffffu)<<ch;
        check^=(v^(v>>8)^(v>>16)^(v>>24))&255;
    }
    return check;
}
typedef struct mix_interval {
    mlp_downmix_design design;
    mlp_downmix_state state;
    mlp_downmix_plan plan;
    mlp_scale_plan scale;
    mlp_downmix_block block[16];
    int32_t source[16][960];
    unsigned length[16],checks[16][2],queued,slot,maximum_bits;
    uint64_t clipped_samples;
    int matrix_search,joint_search;
    mlp_matrix_analysis analysis;
    mlp_matrix_primitive matrix[6];
    unsigned matrix_count,bypass_bits,appended,truncated;
    mlp_search_pool pool[6];
    uint32_t search_rng;
    mlp_search_plan prediction[6];
    unsigned search_ready[6],searched,skipped,feedback,fallback;
    int32_t work[16*960];
    uint8_t bypass[16*160];
} mix_interval;
static int prepare_mix(mlp_pcm *in,mix_interval *mix)
{
    uint32_t summary[16*6]={0};
    unsigned b,ch,channels=in->format.channels;
    mix->queued=mix->slot=0; mix->maximum_bits=1;
    for(b=0;b<16 && in->remaining;++b) {
        int32_t pcm[960]; size_t n,count,capacity=in->format.au_samples;
        if(in->remaining>capacity && in->remaining<capacity+8) capacity=(size_t)in->remaining-8;
        if(mlp_pcm_read(in,pcm,capacity,&count)) return 0;
        mix->length[b]=(unsigned)count;
        for(n=0;n<count;++n) for(ch=0;ch<channels;++ch) {
            unsigned mapped=mix->design.permutation[ch]; int32_t v;
            if(mapped>=channels) return 0;
            v=pcm[n*channels+mapped]; mix->source[b][n*channels+ch]=v;
            summary[b*6+ch]|=(uint32_t)(v<0 ? -v : v);
        }
        mix->checks[b][1]=lossless(0,mix->source[b],count,channels,channels);
        ++mix->queued;
    }
    if(mlp_scale_analyze(summary,mix->queued,channels,2,mix->design.required_headroom,&mix->scale) ||
        mlp_matrix_downmix_plan(mix->design.forward,mix->design.inverse,2,channels,
            mix->scale.scale_count,mix->scale.shift,mix->scale.qss[0],mix->design.post_shift,&mix->plan)) return 0;
    for(ch=0;ch<channels;++ch) if(mix->scale.shift[ch]>7) return 0;
    for(ch=0;ch<2;++ch) if(mix->plan.output_shift[ch]>7) return 0;
    for(b=0;b<mix->queued;++b) {
        size_t n; mlp_downmix_check check={0,0};
        if(mlp_matrix_process_downmix(&mix->state,&mix->plan,channels,mix->scale.shift,
            mix->scale.qss[b],mix->source[b],mix->length[b],b ? 0 : 2,mix->block+b) ||
            (mix->block[b].flags&0x200)) return 0;
        for(n=0;n<mix->length[b];++n) for(ch=0;ch<2;++ch) {
            double value=ldexp((double)mix->block[b].stereo[n*2+ch],(int)mix->plan.output_shift[ch]);
            /* Original VFY 10009240 permits primary two-stream saturation
             * within signed magnitude 27 bits, then clips the rendered PCM. */
            if(value < -134217727 || value>134217727) {
                fprintf(stderr,"Downmix exceeds the original two-stream saturation limit.\n"); return 0;
            }
            if(value < -8388608 || value>8388607) ++mix->clipped_samples;
        }
        if(mlp_matrix_downmix_pcm(&check,mix->block[b].stereo,mix->length[b],2,mix->plan.output_shift,NULL)<0) return 0;
        /* Restart checks cover the pre-clipping values (original 100077f0). */
        mix->checks[b][0]=(check.checksum^(check.checksum>>8)^(check.checksum>>16)^(check.checksum>>24))&255;
        if(mix->maximum_bits<check.maximum_bits) mix->maximum_bits=check.maximum_bits;
    }
    {
        size_t total=0,offset,n; unsigned prefix=mix->plan.forward_count;
        int rc;
        mix->matrix_count=prefix; mix->bypass_bits=mix->plan.bypass_bits;
        memcpy(mix->matrix,mix->plan.forward,prefix*sizeof(*mix->matrix));
        if(mix->matrix_search && mlp_matrix_analysis_reset(&mix->analysis)) return 0;
        for(b=0;b<mix->queued;++b) {
            memcpy(mix->work+total*channels,mix->block[b].transformed,mix->length[b]*channels*sizeof(*mix->work));
            for(n=0;n<mix->length[b];++n) mix->bypass[total+n]=mix->block[b].bypass[n]>>(8-mix->bypass_bits);
            if(mix->matrix_search && mlp_matrix_analysis_add(&mix->analysis,mix->block[b].transformed,mix->length[b],channels)) return 0;
            total+=mix->length[b];
        }
        if(mix->matrix_search) {
            double ratio[6]={0};
            for(ch=0;ch<channels;++ch) ratio[ch]=mix->analysis.covariance[ch*7]/mix->analysis.energy[ch];
            if(mlp_matrix_select_append(mix->analysis.covariance,ratio,channels,2,total,
                mix->scale.qss[0],mix->plan.remaining_scale,1,mix->matrix,&mix->matrix_count,&mix->bypass_bits)) return 0;
            rc=mlp_matrix_apply_suffix_blocks(mix->matrix,prefix,&mix->matrix_count,channels,
                mix->scale.qss,mix->length,mix->queued,mix->work,mix->bypass,&mix->bypass_bits);
            if(rc<0) return 0;
            mix->truncated+=rc!=0; mix->appended+=mix->matrix_count-prefix;
        }
        for(b=0,offset=0;b<mix->queued;++b) {
            memcpy(mix->block[b].transformed,mix->work+offset*channels,mix->length[b]*channels*sizeof(*mix->work));
            for(n=0;n<mix->length[b];++n) mix->block[b].bypass[n]=(uint8_t)(mix->bypass[offset+n]<<(8-mix->bypass_bits));
            offset+=mix->length[b];
        }
        if(mix->joint_search) {
            int32_t mono[16*160]; size_t lengths[16];
            for(b=0;b<mix->queued;++b) lengths[b]=mix->length[b];
            for(ch=0;ch<channels;++ch) {
                for(n=0;n<total;++n) mono[n]=mix->work[n*channels+ch];
                mix->pool[ch].rng=mix->search_rng;
                rc=mlp_search_interval(&mix->pool[ch],mono,lengths,mix->queued,8,total>=40 ? 4 : 0,32,0,&mix->prediction[ch]);
                mix->search_rng=mix->pool[ch].rng;
                if(rc) rc=mlp_search_interval(NULL,mono,lengths,mix->queued,8,0,8,0,&mix->prediction[ch]);
                mix->search_ready[ch]=rc==0;
                mix->searched+=rc==0; mix->skipped+=rc!=0;
            }
        }
    }
    return 1;
}
static int encode(mlp_pcm *in,FILE *out,mix_interval *mix)
{
    mlp_parameters base[2]={{0}},previous[2]={{0}};
    mlp_predict_state history[6]={0};
    mlp_rate_state rate; unsigned channels=in->format.channels,au=0,timing=0,checks[2]={0};
    uint64_t frames=0,bytes=0;
    unsigned s,ch,i;
    for(s=0;s<2;++s) {
        base[s].minimum_channel=s ? 2 : 0;
        base[s].maximum_channel=s ? channels-1 : 1;
        for(ch=0;ch<channels;++ch) base[s].coding[ch].total_width=24;
        previous[s]=base[s];
    }
    mlp_rate_init(&rate);
    while(frames<in->frames) {
        int32_t pcm[960],residual[960]; size_t count,capacity=in->format.au_samples;
        uint32_t payload[2][2048],flags=0; unsigned sizes[2],directories[2],length,parity;
        unsigned restart=(au%16)==0,first;
        uint16_t arrival,major[14];
        if(in->remaining>capacity && in->remaining<capacity+8) capacity=(size_t)in->remaining-8;
        if(mix) {
            if(mix->slot==mix->queued && (!restart || !prepare_mix(in,mix))) return 0;
            count=mix->length[mix->slot];
            memcpy(pcm,mix->block[mix->slot].transformed,count*channels*sizeof(*pcm));
        } else if(mlp_pcm_read(in,pcm,capacity,&count)) return 0;
        first=restart && count>=16 ? 8 : 0;
        if(restart) memset(history,0,sizeof(history));
        for(i=0;i<count*channels;++i) {
            unsigned qss=mix ? mix->scale.qss[mix->slot][i%channels] : 24-in->format.bits;
            residual[i]=pcm[i]/(int32_t)(1u<<qss);
        }
        for(s=0;s<2;++s) {
            mlp_parameters main=base[s],initial=base[s];
            mlp_restart header={0}; mlp_substream stream={0}; mlp_bits writer;
            main.flags=restart || count!=previous[s].blocksize ? 2 : 0;
            main.blocksize=(unsigned)count; initial.blocksize=8;
            if(mix) {
                main.matrix_count=initial.matrix_count=s ? mix->matrix_count : mix->plan.inverse_count;
                memcpy(main.matrix,s ? mix->matrix : mix->plan.inverse,main.matrix_count*sizeof(*main.matrix));
                memcpy(initial.matrix,main.matrix,main.matrix_count*sizeof(*main.matrix));
                main.matrix_changed=initial.matrix_changed=restart && main.matrix_count;
                for(ch=0;ch<=main.maximum_channel;++ch)
                    main.output_shift[ch]=initial.output_shift[ch]=(int)(s ? mix->scale.shift[ch] : mix->plan.output_shift[ch]);
            }
            if(restart) {
                previous[s]=base[s]; header.timing=timing;
                header.minimum_channel=main.minimum_channel; header.maximum_channel=main.maximum_channel;
                header.maximum_lsbs=24; header.maximum_bits=mix && !s ? mix->maximum_bits : 25;
                header.lossless_check=checks[s]; checks[s]=0;
                for(ch=0;ch<channels;++ch) header.assignment[ch]=ch;
                if(mix) {
                    header.dither_shift=s ? mix->plan.forward_noise_shift : mix->plan.inverse_noise_shift;
                    header.seed=s ? mix->block[mix->slot].forward_seed : mix->block[mix->slot].inverse_seed;
                    header.maximum_shift=s ? mix->scale.maximum_shift : (unsigned)mix->plan.maximum_shift;
                    if(s) for(ch=0;ch<channels;++ch) header.assignment[ch]=mix->design.permutation[ch];
                }
            }
            checks[s]=mix ? checks[s]^mix->checks[mix->slot][s] : lossless(checks[s],pcm,count,channels,s ? channels : 2);
            for(ch=0;ch<=main.maximum_channel;++ch)
                main.qss[ch]=initial.qss[ch]=mix ? mix->scale.qss[mix->slot][ch] : 24-in->format.bits;
            for(ch=main.minimum_channel;ch<=main.maximum_channel;++ch) {
                int32_t input[160],output[160]; unsigned qss=main.qss[ch];
                mlp_predict_filter a={0},b={0}; int rc=MLP_PREDICT_OK;
                int searched=mix && mix->joint_search && mix->search_ready[ch] && (!restart || first) && !((count-first)&1);
                for(i=0;i<count;++i) input[i]=pcm[i*channels+ch];
                for(i=0;i<first;++i) {
                    output[i]=input[i]/(int32_t)(1u<<qss); history[ch].input[i]=(float)input[7-i];
                }
                if(first && mlp_cost_select(output,first,qss,&base[s].coding[ch],1,31,&initial.coding[ch])) return 0;
                if(searched) {
                    main.a[ch]=mix->prediction[ch].a; main.b[ch]=mix->prediction[ch].b;
                    a.order=main.a[ch].order; b.order=main.b[ch].order;
                    memcpy(a.coefficient,main.a[ch].coefficient,sizeof(a.coefficient));
                    memcpy(b.coefficient,main.b[ch].coefficient,sizeof(b.coefficient));
                    if(restart && b.order) {
                        main.state[ch]=mix->prediction[ch].state;
                        for(i=0;i<b.order;++i) history[ch].residual[i]=(float)main.state[ch].value[i];
                    }
                }
                if((count-first)&1) {
                    for(i=first;i<count;++i) {
                        unsigned k; output[i]=input[i]/(int32_t)(1u<<qss);
                        for(k=7;k;--k) history[ch].input[k]=history[ch].input[k-1];
                        history[ch].input[0]=(float)input[i];
                    }
                } else if(count>first) rc=mlp_predict_block(&a,b.order ? &b : NULL,&history[ch],qss,input+first,count-first,output+first,0);
                if(rc==MLP_PREDICT_INVALID) return 0;
                if(mix) { mix->fallback+=rc==MLP_PREDICT_FALLBACK; mix->feedback+=b.order!=0; }
                if(!a.order && !restart && previous[s].a[ch].order) { a.order=1; memset(a.coefficient,0,sizeof(a.coefficient)); }
                main.a[ch].order=a.order; if(!searched) main.a[ch].precision=8;
                memcpy(main.a[ch].coefficient,a.coefficient,sizeof(a.coefficient));
                main.a[ch].changed=(restart && a.order) || main.a[ch].order!=previous[s].a[ch].order ||
                    memcmp(main.a[ch].coefficient,previous[s].a[ch].coefficient,a.order*sizeof(a.coefficient[0]));
                main.b[ch].order=b.order;
                memcpy(main.b[ch].coefficient,b.coefficient,sizeof(b.coefficient));
                main.b[ch].changed=main.b[ch].order!=previous[s].b[ch].order ||
                    memcmp(main.b[ch].coefficient,previous[s].b[ch].coefficient,sizeof(b.coefficient));
                if(!b.order) main.state[ch].changed=0;
                if(mlp_cost_select(output+first,count-first,qss,first ? &initial.coding[ch] : &previous[s].coding[ch],0,31,&main.coding[ch])) return 0;
                for(i=0;i<count;++i) residual[i*channels+ch]=output[i];
            }
            stream.restart=restart ? &header : NULL; stream.initial=&initial;
            stream.main=&main; stream.previous=&previous[s]; stream.residual=residual;
            stream.count=count; stream.stride=channels; stream.primary=s==0;
            if(mix && s) { stream.bypass=mix->block[mix->slot].bypass; stream.bypass_bits=mix->bypass_bits; }
            stream.end_markers=frames+count==in->frames; stream.single_restart=restart && !first;
            mlp_bits_init(&writer,payload[s],2048);
            if(mlp_substream_put(&writer,&stream)) { fprintf(stderr,"Substream %u serialization failed at AU %u\n",s,au); return 0; }
            sizes[s]=(unsigned)writer.count; previous[s]=main;
        }
        length=(restart ? 18 : 4)+sizes[0]+sizes[1];
        {
            int32_t next=rate.arrival;
            int32_t earliest=(int32_t)rate.decode-(int32_t)(in->format.sample_rate*75u/1000u);
            if(next>(int32_t)rate.decode+0x4000) next-=0x10000;
            if(next<earliest) rate.arrival=(uint16_t)earliest;
        }
        if(mlp_rate_update(&rate,(unsigned)count,length,in->format.rate_field,&flags,&arrival) || (flags&0x2000)) {
            fprintf(stderr,"Two-substream AU %u exceeds FIFO/size limit (%x)\n",au,flags); return 0;
        }
        directories[0]=(restart ? 0x2000 : 0x6000)|sizes[0];
        directories[1]=(restart ? 0x2000 : 0x6000)|(sizes[0]+sizes[1]);
        parity=mlp_au_parity(length,arrival,directories,2);
        if(!put_word(out,parity<<12|length) || !put_word(out,arrival)) return 0;
        if(restart) {
            mlp_format_major(&in->format,major);
            /* Original Init profile 0xd and 1000d850's two-stream 0x2200. */
            major[8]=0x220d; major[13]=mlp_major_checksum(major);
            for(i=0;i<14;++i) if(!put_word(out,major[i])) return 0;
        }
        for(s=0;s<2;++s) if(!put_word(out,directories[s])) return 0;
        for(s=0;s<2;++s) for(i=0;i<sizes[s];++i) if(!put_word(out,payload[s][i])) return 0;
        frames+=count; bytes+=2*length; timing=(timing+(unsigned)count)&65535; ++au;
        if(mix) ++mix->slot;
    }
    if(mix && mix->clipped_samples) fprintf(stderr,"Downmix saturated: %llu stereo samples clipped; full-channel PCM remains lossless.\n",(unsigned long long)mix->clipped_samples);
    if(mix && (mix->matrix_search || mix->joint_search)) fprintf(stderr,"Optimization appended=%u truncated=%u searched=%u skipped=%u feedback=%u fallback=%u\n",mix->appended,mix->truncated,mix->searched,mix->skipped,mix->feedback,mix->fallback);
    fprintf(stderr,"Encoded %llu frames, %u channels, two substreams, %u AUs, %llu bytes\n",(unsigned long long)frames,channels,au,(unsigned long long)bytes);
    return !ferror(out);
}
static int two_stream_main(int argc,char **argv)
{
    mlp_pcm input; FILE *out; int fd,ok; mix_interval *mix=NULL; double coefficients[2][6];
    const char *matrix_path=NULL; int arg,matrix_search=0,joint_search=0;
    if(argc<3) goto usage;
    for(arg=3;arg<argc;++arg) {
        if(!strcmp(argv[arg],"--downmix") && !matrix_path && arg+1<argc) matrix_path=argv[++arg];
        else if(!strcmp(argv[arg],"--matrix-search") && !matrix_search) matrix_search=1;
        else if(!strcmp(argv[arg],"--joint-search") && !joint_search) joint_search=1;
        else goto usage;
    }
    if((matrix_search || joint_search) && !matrix_path) goto usage;
    if(matrix_path) {
        FILE *matrix=fopen(matrix_path,"r"); unsigned i; int tail;
        if(!matrix) { perror(matrix_path); return 2; }
        for(i=0;i<12;++i) if(fscanf(matrix,"%lf",&coefficients[i/6][i%6])!=1) break;
        do { tail=fgetc(matrix); } while(tail!=EOF && isspace((unsigned char)tail));
        fclose(matrix);
        if(i!=12 || tail!=EOF || !(mix=calloc(1,sizeof(*mix)))) return 2;
        if(mlp_matrix_design_downmix(coefficients,14,&mix->design)) { free(mix); return 2; }
        /* Original 100086b0: forward 10024d0c=2, inverse 10024d08=1. */
        mix->state.forward_seed=2; mix->state.inverse_seed=1;
        mix->matrix_search=matrix_search; mix->joint_search=joint_search;
        mix->search_rng=1;
        if(joint_search) for(i=0;i<6;++i) {
            if(mlp_search_pool_init(mix->pool+i,mix->search_rng)) { free(mix); return 2; }
            mix->search_rng=mix->pool[i].rng;
        }
    }
    if(mlp_pcm_open(&input,argv[1],NULL)) { fprintf(stderr,"%s\n",input.error); free(mix); return 1; }
    if(input.format.channels<3 || input.frames<8) {
        fprintf(stderr,"This diagnostic requires 3..6 channels and at least eight frames.\n"); mlp_pcm_close(&input); free(mix); return 1;
    }
    if(mix) {
        unsigned i,j;
        if(mlp_matrix_analysis_init(&mix->analysis,input.format.channels)) { mlp_pcm_close(&input); free(mix); return 2; }
        for(i=0;i<2;++i) for(j=input.format.channels;j<6;++j) if(coefficients[i][j]) {
            fprintf(stderr,"Downmix coefficient refers to an absent input channel.\n"); mlp_pcm_close(&input); free(mix); return 2;
        }
    }
    fd=_open(argv[2],O_WRONLY|O_CREAT|O_EXCL|O_BINARY,0600);
    if(fd<0) { perror(argv[2]); mlp_pcm_close(&input); free(mix); return 1; }
    out=_fdopen(fd,"wb");
    if(!out) { _close(fd); remove(argv[2]); mlp_pcm_close(&input); free(mix); return 1; }
    ok=encode(&input,out,mix); free(mix);
    if(fclose(out)) ok=0;
    if(!ok && input.error[0]) fprintf(stderr,"%s\n",input.error);
    mlp_pcm_close(&input);
    if(!ok) { remove(argv[2]); fprintf(stderr,"Encoding failed; partial output removed.\n"); return 1; }
    return 0;
usage:
    fprintf(stderr,"Usage: mlp_two_stream_encode.exe input.wav|aiff output.mlp [--downmix MATRIX.txt [--matrix-search] [--joint-search]]\n"); return 2;
}
int mlp_two_stream_main(int argc,char **argv)
{
    unsigned saved = _controlfp(0,0);
    int result;
    _controlfp(_PC_53,_MCW_PC);
    result = two_stream_main(argc,argv);
    _controlfp(saved,_MCW_PC);
    return result;
}
#ifndef MLP_TWO_STREAM_EMBEDDED
int main(int argc,char **argv) { return mlp_two_stream_main(argc,argv); }
#endif
