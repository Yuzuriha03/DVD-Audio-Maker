/* Application assembly around components; not the VC6 job ABI. */
#define _POSIX_C_SOURCE 200809L
#include "mlp_pcm.h"
#include "mlp_substream.h"
#include "mlp_predict.h"
#include "mlp_rate.h"
#include "mlp_output_queue.h"
#include "mlp_stamp.h"
#include "mlp_encoder.h"
#include "mlp_matrix.h"
#include "mlp_scale.h"
#include "mlp_interval.h"
#include "mlp_search.h"
#include <errno.h>
#include <fcntl.h>
#include <float.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>
#ifdef MLP_TWO_STREAM_EMBEDDED
int mlp_two_stream_main(int argc,char **argv);
#endif
#ifdef _WIN32
#include <io.h>
#include <sys/stat.h>
#define open_fd _open
#define fd_file _fdopen
#define close_fd _close
#else
#include <unistd.h>
#define open_fd open
#define fd_file fdopen
#define close_fd close
#define O_BINARY 0
#endif
#ifndef MLP_ENCODER_LIBRARY
static int put_word(FILE *file, unsigned word)
{
    return fputc((word>>8)&255,file) != EOF && fputc(word&255,file) != EOF;
}
#endif
typedef struct planned_au {
    mlp_parameters initial,main,previous;
    int32_t residual[960],unpredicted[960];uint8_t bypass[160];
    unsigned count,bypass_bits,single_restart,end_markers,stamp;
} planned_au;
typedef struct interval_output {
    planned_au au[128];unsigned aus;
    unsigned maximum_lsbs;
    mlp_restart restart;
    const char *failure;
} interval_output;
static int serialize_planned(mlp_bits *writer,const interval_output *pending,
    unsigned au,const mlp_format *format)
{
    const planned_au *p=pending->au+au;mlp_substream stream={0};
    stream.restart=au==0?&pending->restart:NULL;stream.initial=&p->initial;
    stream.main=&p->main;stream.previous=&p->previous;stream.residual=p->residual;
    stream.stride=format->channels;stream.count=p->count;stream.primary=1;
    stream.end_markers=p->end_markers;stream.single_restart=p->single_restart;
    stream.bypass=p->bypass;stream.bypass_bits=p->bypass_bits;
    return mlp_substream_put(writer,&stream);
}
/* Check the entire restart interval before committing bytes or timing state.
 * The normal plan is retained byte-for-byte whenever it fits. */
static int interval_fits(interval_output *pending,const mlp_format *format)
{
    unsigned au;
    pending->restart.maximum_lsbs=pending->maximum_lsbs;
    for(au=0;au<pending->aus;++au) {
        uint32_t payload[2048];mlp_bits writer;
        mlp_bits_init(&writer,payload,2048);
        if(serialize_planned(&writer,pending,au,format)) return -1;
        if(writer.count+(au==0?17u:3u)>0x300u) return 0;
    }
    return 1;
}
/* An unpredicted restart interval avoids large FIR/IIR parameter overhead.
 * Saved values are the exact reversible-matrix PCM divided by its existing
 * QSS. Keep matrix/bypass, shifts, AU boundaries, checks and metadata intact. */
static int unpredicted_interval(interval_output *pending,const mlp_format *format)
{
    mlp_parameters previous=pending->au[0].previous;unsigned au,ch,i;
    pending->maximum_lsbs=0;
    for(au=0;au<pending->aus;++au) {
        planned_au *p=pending->au+au;unsigned first=au==0 && !p->single_restart?8u:0u;
        p->previous=previous;
        memset(p->main.a,0,sizeof(p->main.a));memset(p->main.b,0,sizeof(p->main.b));
        memset(p->main.state,0,sizeof(p->main.state));
        memset(p->initial.a,0,sizeof(p->initial.a));memset(p->initial.b,0,sizeof(p->initial.b));
        memset(p->initial.state,0,sizeof(p->initial.state));
        memcpy(p->residual,p->unpredicted,p->count*format->channels*sizeof(*p->residual));
        for(ch=0;ch<format->channels;++ch) {
            int32_t mono[160];unsigned qss=p->main.qss[ch];
            int limit=32-((ch==0 || ch==format->channels-1)?(int)p->bypass_bits:0);
            for(i=0;i<p->count;++i) mono[i]=p->unpredicted[i*format->channels+ch];
            p->main.a[ch].precision=p->initial.a[ch].precision=8;
            if(first && mlp_cost_select(mono,first,qss,&previous.coding[ch],1,limit,&p->initial.coding[ch])) return 0;
            if(mlp_cost_select(mono+first,p->count-first,qss,first?&p->initial.coding[ch]:&previous.coding[ch],
                0,limit,&p->main.coding[ch])) return 0;
            if((unsigned)p->main.coding[ch].total_width>pending->maximum_lsbs)
                pending->maximum_lsbs=(unsigned)p->main.coding[ch].total_width;
            if(first && (unsigned)p->initial.coding[ch].total_width>pending->maximum_lsbs)
                pending->maximum_lsbs=(unsigned)p->initial.coding[ch].total_width;
        }
        previous=p->main;
    }
    return 1;
}
#ifndef MLP_ENCODER_LIBRARY
static int emit_words(void *opaque,const uint32_t *words,size_t count)
{
    size_t i;FILE *out=opaque;
    for(i=0;i<count;++i) if(!put_word(out,words[i])) return 0;
    return 1;
}
#endif
static int flush_interval(mlp_output_queue *queue,interval_output *pending,
    const mlp_format *format,mlp_rate_state *rate,uint64_t *words_total)
{
    unsigned au;int fits;
    if (!pending->aus) return 1;
    fits=interval_fits(pending,format);
    if(fits==0) {
        if(!unpredicted_interval(pending,format)) { pending->failure="Cannot build lossless oversized-AU fallback";return 0; }
        fits=interval_fits(pending,format);
    }
    if(fits!=1) { pending->failure=fits==0?"Access unit too large after lossless fallback (1536-byte limit)":
        "Cannot serialize lossless access unit";return 0; }
    /* Match 10008510: all interval costs/widths are known before
     * 1000d9a0 serializes any substream. No header/checksum backpatch. */
    for(au=0;au<pending->aus;++au) {
        planned_au *p=pending->au+au;mlp_bits writer;
        uint32_t payload[2048],words[2065],flags=0;uint16_t major[14],arrival;
        unsigned is_restart=au==0,length,directory,parity,i;size_t position=0;
        mlp_bits_init(&writer,payload,2048);
        if(serialize_planned(&writer,pending,au,format)) return 0;
        length=(is_restart?17:3)+(unsigned)writer.count;
        {
            int32_t next=rate->arrival;
            int32_t earliest=(int32_t)rate->decode-(int32_t)(format->sample_rate*75u/1000u);
            if(next>(int32_t)rate->decode+0x4000) next-=0x10000;
            if(next<earliest) rate->arrival=(uint16_t)earliest;
        }
        if(mlp_rate_update(rate,p->count,length,format->rate_field,&flags,&arrival) || (flags&0x2000)) {
            pending->failure=(flags&0x2000)?"Access unit too large (1536-byte limit)":
                "MLP FIFO rate limit exceeded; input cannot be encoded at the required delivery rate";return 0;
        }
        directory=(is_restart?0x2000:0x6000)|(unsigned)writer.count|(p->stamp<<12);
        parity=mlp_au_parity(length,arrival,&directory,1);
        words[position++]=parity<<12|length;words[position++]=arrival;
        if(is_restart) {
            mlp_format_major(format,major);
            for(i=0;i<14;++i) words[position++]=major[i];
        }
        words[position++]=directory;
        memcpy(words+position,payload,writer.count*sizeof(*payload));position+=writer.count;
        if(position!=length || !mlp_output_queue_push(queue,words,position,p->count)) return 0;
        *words_total+=length;
    }
    pending->maximum_lsbs=0;pending->aus=0;
    return 1;
}
static unsigned lossless(unsigned check, const int32_t *pcm, size_t count, unsigned channels)
{
    size_t n; unsigned ch;
    for (n = 0; n < count; ++n) for (ch = 0; ch < channels; ++ch) {
        uint32_t v = ((uint32_t)pcm[n*channels+ch]&0xffffffu)<<ch;
        check ^= (v ^ (v>>8) ^ (v>>16) ^ (v>>24))&255;
    }
    return check;
}
typedef struct matrix_interval {
    int32_t pcm[128*160*6];
    uint8_t bypass[128*160];
    unsigned lengths[128],checks[128],queued,slot,offset,bits,count,selected_matrix;
    mlp_matrix_primitive matrix[6];
    mlp_matrix_analysis analysis;
    mlp_scale_plan scale;
    int original_scale,enable_matrix,joint_search;
    unsigned search_ready[6];
    mlp_search_plan prediction[6];
    mlp_search_pool pool[6];
    uint32_t search_rng;
} matrix_interval;
static int prepare_matrix(mlp_pcm *in, matrix_interval *m, unsigned interval,
    int allow_bypass, unsigned *truncated)
{
    unsigned ch,i,qss[6] = {0},scale[6] = {0},required[6] = {0};
    uint32_t summary[32*6] = {0};
    double ratio[6] = {0}; size_t total = 0; int rc;
    m->queued = m->slot = m->offset = 0;
    if (mlp_matrix_analysis_reset(&m->analysis)) return 0;
    for (i = 0; i < interval && in->remaining; ++i) {
        size_t count,capacity = in->format.au_samples;
        int32_t *p = m->pcm+total*in->format.channels;
        if (in->remaining > capacity && in->remaining < capacity+8) capacity = (size_t)in->remaining-8;
        if (mlp_pcm_read(in,p,capacity,&count)) return 0;
        m->lengths[i] = (unsigned)count;
        m->checks[i] = lossless(0,p,count,in->format.channels);
        if (m->original_scale) {
            size_t n;
            for (n = 0; n < count; ++n) for (ch = 0; ch < in->format.channels; ++ch) {
                int32_t v = p[n*in->format.channels+ch];
                summary[i*6+ch] |= (uint32_t)(v < 0 ? -v : v);
            }
        }
        total += count; ++m->queued;
    }
    if (m->original_scale) {
        size_t n;
        if (mlp_scale_analyze(summary,m->queued,in->format.channels,0,required,&m->scale)) return 0;
        for (ch = 0; ch < in->format.channels; ++ch) {
            if (m->scale.shift[ch] > 7) return 0;
            for (n = 0; n < total; ++n) m->pcm[n*in->format.channels+ch] /= (int32_t)(1u<<m->scale.shift[ch]);
        }
    }
    {
        size_t offset = 0;
        for (i = 0; i < m->queued; ++i) {
            if (mlp_matrix_analysis_add(&m->analysis,m->pcm+offset*in->format.channels,
                m->lengths[i],in->format.channels)) return 0;
            offset += m->lengths[i];
        }
    }
    for (ch = 0; ch < in->format.channels; ++ch) {
        qss[ch] = m->original_scale ? m->scale.qss[0][ch] : 24-in->format.bits;
        scale[ch] = m->original_scale ? m->scale.scale_count[ch] : allow_bypass != 0;
        ratio[ch] = m->analysis.covariance[ch*7]/m->analysis.energy[ch];
    }
    if (m->enable_matrix) {
    if (mlp_matrix_select(m->analysis.covariance,ratio,in->format.channels,0,total,
        qss,scale,(allow_bypass || m->original_scale) && in->format.au_samples != 160,
        m->matrix,&m->count,&m->bits)) return 0;
    /* 10007cd0 records this flag before 10007a40 can truncate the
     * primitive list. An empty list can still require an explicit update. */
    m->selected_matrix = m->count != 0;
    if (m->original_scale) rc = mlp_matrix_apply_blocks(m->matrix,&m->count,in->format.channels,
        m->scale.qss,m->lengths,m->queued,m->pcm,m->bypass,&m->bits);
    else rc = mlp_matrix_apply_interval(m->matrix,&m->count,in->format.channels,qss,m->pcm,total,m->bypass,&m->bits);
    if (rc < 0) return 0;
    *truncated += rc != 0;
    for (i = 0; i < total; ++i) m->bypass[i] <<= 8-m->bits;
    } else m->count = m->bits = m->selected_matrix = 0;
    if (m->joint_search) {
        int32_t *mono = malloc(total*sizeof(*mono));
        size_t lengths[128],n;
        if (!mono) return 0;
        for (i = 0; i < m->queued; ++i) lengths[i] = m->lengths[i];
        for (ch = 0; ch < in->format.channels; ++ch) {
            unsigned limited = in->format.au_samples == 160;
            for (n = 0; n < total; ++n) mono[n] = m->pcm[n*in->format.channels+ch];
            /* High-rate primary profile permits FIR <=4 and no B filter. */
            m->pool[ch].rng = m->search_rng;
            rc = mlp_search_interval(&m->pool[ch],mono,lengths,m->queued,
                limited ? 4 : 8,!limited && total >= 40 ? 4 : 0,32,
                limited ? 2 : 0,&m->prediction[ch]);
            m->search_rng = m->pool[ch].rng;
            if (rc) rc = mlp_search_interval(NULL,mono,lengths,m->queued,
                limited ? 4 : 8,0,8,limited ? 2 : 0,&m->prediction[ch]);
            m->search_ready[ch] = rc == 0;
        }
        free(mono);
    }
    return 1;
}
static int encode(mlp_pcm *in, FILE *out, unsigned restart_interval, unsigned cycle, int predict, matrix_interval *matrix, int allow_bypass, interval_output *pending,mlp_output_queue *queue,mlp_stamp_state *stamp)
{
    mlp_parameters base = {0}, previous = {0};
    mlp_predict_state history[6] = {0};
    unsigned prediction_reset[6] = {0};
    mlp_rate_state rate;
    mlp_boundary_state boundary = {0};
    unsigned channels = in->format.channels;
    unsigned au = 0, timing = 0, interval_check = 0, restarted = 0;
    unsigned predicted_channels = 0, fallback_channels = 0;
    unsigned matrix_intervals = 0, matrix_primitives = 0, matrix_truncated = 0, matrix_bypasses = 0;
    unsigned scale_intervals = 0,shifted_channels = 0,qss_changes = 0;
    unsigned search_intervals = 0,search_skipped = 0,feedback_channels = 0;
    uint32_t noise_seed = 2;
    uint64_t frames = 0, words_total = 0;
    size_t count;
    unsigned ch;
    base.maximum_channel = channels-1;
    for (ch = 0; ch < channels; ++ch) base.coding[ch].total_width = 24;
    previous = base; mlp_rate_init(&rate);
    if (matrix && mlp_matrix_analysis_init(&matrix->analysis,channels)) return 0;
    if (matrix && matrix->joint_search) {
        /* Original 10009100 initializes all six pools on one CRT stream.
         * Seed 1 models a fresh normal encode thread, without upsampling. */
        matrix->search_rng = 1;
        for (ch = 0; ch < 6; ++ch) {
            if (mlp_search_pool_init(&matrix->pool[ch],matrix->search_rng)) return 0;
            matrix->search_rng = matrix->pool[ch].rng;
        }
    }
    while (frames < in->frames) {
        int32_t pcm[960], residual[960],unpredicted[960];
        mlp_parameters p = base, initial = base;
        mlp_restart restart = {0};
        mlp_substream stream = {0};
        unsigned is_restart,span = restart_interval,first,i;
        size_t capacity = in->format.au_samples;
        if (cycle) {
            int decision = mlp_interval_boundary(&boundary,cycle,restart_interval,in->format.au_samples);
            if (decision < 0) return 0;
            is_restart = (unsigned)decision;
            if (matrix && is_restart) {
                mlp_boundary_state ahead = boundary;
                span = 1;
                while (mlp_interval_boundary(&ahead,cycle,restart_interval,in->format.au_samples) == 0)
                    if (++span > 128) return 0;
                if (matrix->original_scale && span > MLP_SCALE_BLOCKS) return 0;
            }
        } else is_restart = au%restart_interval == 0;
        if (is_restart && !flush_interval(queue,pending,&in->format,&rate,&words_total)) return 0;
        /* Keep the final AU at least 8 samples without padding: shorten
         * its predecessor when the remaining tail would be 1..7 samples. */
        if (in->remaining > capacity && in->remaining < capacity+8)
            capacity = (size_t)in->remaining-8;
        if (matrix) {
            if (matrix->slot == matrix->queued) {
                if (!is_restart || !prepare_matrix(in,matrix,span,allow_bypass,&matrix_truncated)) return 0;
                matrix_intervals += matrix->count != 0;
                matrix_primitives += matrix->count; matrix_bypasses += matrix->bits;
                if (matrix->joint_search) for (ch = 0; ch < channels; ++ch) {
                    search_intervals += matrix->search_ready[ch]; search_skipped += !matrix->search_ready[ch];
                }
                if (matrix->original_scale) {
                    ++scale_intervals;
                    for (ch = 0; ch < channels; ++ch) shifted_channels += matrix->scale.shift[ch] != 0;
                }
            }
            count = matrix->lengths[matrix->slot];
            memcpy(pcm,matrix->pcm+matrix->offset*channels,count*channels*sizeof(*pcm));
            p.matrix_count = initial.matrix_count = matrix->count;
            memcpy(p.matrix,matrix->matrix,matrix->count*sizeof(*p.matrix));
            memcpy(initial.matrix,matrix->matrix,matrix->count*sizeof(*p.matrix));
            p.matrix_changed = initial.matrix_changed = is_restart && matrix->selected_matrix;
            stream.bypass = matrix->bypass+matrix->offset; stream.bypass_bits = matrix->bits;
            if (matrix->original_scale) for (ch = 0; ch < channels; ++ch)
                p.output_shift[ch] = initial.output_shift[ch] = (int)matrix->scale.shift[ch];
        } else if (mlp_pcm_read(in,pcm,capacity,&count)) return 0;
        first = is_restart && count >= 16 ? 8 : 0;
        /* Original descriptor bit 2 marks the restart AU. Parameter
         * serialization also emits a size on its successor. Explicitly
         * signal a changing short tail in this application's tail mode. */
        p.flags = is_restart || count != previous.blocksize ? 2 : 0;
        p.blocksize = (unsigned)count;
        initial.flags = 0; initial.blocksize = 8;
        if (is_restart) {
            previous = base; memset(history,0,sizeof(history)); ++restarted;
            for (ch=0;ch<channels;++ch) prediction_reset[ch]=1;
            restart.maximum_channel = channels-1;
            restart.seed = noise_seed;
            restart.maximum_lsbs = 24;
            /* Width is based on absolute magnitude, including -0x800000.
             * Without an interval plan, 25 is the conservative 24-bit bound. */
            restart.maximum_bits = 25;
            if (matrix && matrix->original_scale) {
                restart.maximum_shift = matrix->scale.maximum_shift;
                restart.maximum_bits = matrix->scale.maximum_bits;
                /* 10007cd0 calls 10007c30 even with no downmix candidates.
                 * Its dormant forward-noise shift is max(first QSS-8,0),
                 * and is serialized even when no matrix uses noise. */
                restart.dither_shift = matrix->scale.qss[0][0]>8 ? matrix->scale.qss[0][0]-8 : 0;
            }
            restart.timing = timing; restart.lossless_check = interval_check; interval_check = 0;
        }
        interval_check = matrix ? interval_check^matrix->checks[matrix->slot] : lossless(interval_check,pcm,count,channels);
        for (ch = 0; ch < channels; ++ch) {
            int32_t input[160], output[160];
            unsigned qss = matrix && matrix->original_scale ? matrix->scale.qss[matrix->slot][ch] : 24-in->format.bits;
            /* 10008510 reserves bypass bits only at the two endpoints of
             * this substream's channel range; interior channels get 32. */
            int code_limit=32-((ch==0 || ch==channels-1)?(int)stream.bypass_bits:0);
            mlp_predict_filter a = {0};
            mlp_predict_filter b = {0};
            int rc = MLP_PREDICT_OK;
            int searched = matrix && matrix->joint_search && matrix->search_ready[ch] &&
                (!is_restart || first) && !((count-first)&1);
            for (i = 0; i < count; ++i) {
                input[i] = pcm[i*channels+ch];
                unpredicted[i*channels+ch]=input[i]/(int32_t)(1u<<qss);
            }
            p.qss[ch] = initial.qss[ch] = qss;
            if (!is_restart && qss != previous.qss[ch]) ++qss_changes;
            for (i = 0; i < first; ++i) {
                output[i] = input[i]/(int32_t)(1u<<qss);
                history[ch].input[i] = (float)input[7-i];
            }
            if (first && mlp_cost_select(output,first,qss,&base.coding[ch],1,code_limit,&initial.coding[ch])) return 0;
            if (predict && (!is_restart || first) && !((count-first)&1)) { a.order = 1; a.coefficient[0] = 1; }
            if (searched) {
                p.a[ch] = matrix->prediction[ch].a; p.b[ch] = matrix->prediction[ch].b;
                a.order = p.a[ch].order; b.order = p.b[ch].order;
                memcpy(a.coefficient,p.a[ch].coefficient,sizeof(a.coefficient));
                memcpy(b.coefficient,p.b[ch].coefficient,sizeof(b.coefficient));
                if (is_restart && b.order) {
                    p.state[ch] = matrix->prediction[ch].state;
                    for (i = 0; i < b.order; ++i) history[ch].residual[i] = (float)p.state[ch].value[i];
                }
            }
            if ((count-first)&1) {
                /* The predictor consumes pairs; an odd final block
                 * uses unpredicted coding without padding the source. */
                for (i = first; i < count; ++i) {
                    unsigned k;
                    output[i] = input[i]/(int32_t)(1u<<qss);
                    for (k = 7; k; --k) history[ch].input[k] = history[ch].input[k-1];
                    history[ch].input[0] = (float)input[i];
                }
            } else if (count > first) rc = mlp_predict_block(&a,b.order ? &b : NULL,&history[ch],qss,input+first,count-first,output+first,prediction_reset[ch]);
            if (rc == MLP_PREDICT_INVALID) return 0;
            predicted_channels += a.order != 0; fallback_channels += rc == MLP_PREDICT_FALLBACK;
            /* VFY rejects an explicit transition to order zero after a
             * nonzero filter. A restart already resets the FIR to zero;
             * unchanged silence needs no filter update at all. */
            if (!a.order && !is_restart && previous.a[ch].order) {
                a.order = 1;
                memset(a.coefficient,0,sizeof(a.coefficient));
            }
            p.a[ch].order = a.order; if (!searched) p.a[ch].precision = 8;
            memcpy(p.a[ch].coefficient,a.coefficient,sizeof(a.coefficient));
            p.a[ch].changed = (is_restart && a.order) || p.a[ch].order != previous.a[ch].order ||
                memcmp(p.a[ch].coefficient,previous.a[ch].coefficient,a.order*sizeof(a.coefficient[0]));
            p.b[ch].order = b.order;
            memcpy(p.b[ch].coefficient,b.coefficient,sizeof(b.coefficient));
            p.b[ch].changed = p.b[ch].order != previous.b[ch].order ||
                memcmp(p.b[ch].coefficient,previous.b[ch].coefficient,sizeof(b.coefficient));
            /* 1000aad0 starts local_14=1 and carries the predictor return
             * across the interval. A first/repeated overflow leaves the
             * already reset wire filter alone (1000a4a0/1000a730). */
            if (rc==MLP_PREDICT_FALLBACK && prediction_reset[ch]) {
                p.a[ch]=previous.a[ch];p.b[ch]=previous.b[ch];
                p.a[ch].changed=p.b[ch].changed=0;
            }
            prediction_reset[ch]=rc==MLP_PREDICT_FALLBACK;
            if (!b.order) p.state[ch].changed = 0;
            feedback_channels += b.order != 0;
            if (mlp_cost_select(output+first,count-first,qss,first ? &initial.coding[ch] : &previous.coding[ch],0,code_limit,&p.coding[ch])) return 0;
            if ((unsigned)p.coding[ch].total_width > pending->maximum_lsbs)
                pending->maximum_lsbs=(unsigned)p.coding[ch].total_width;
            if (first && (unsigned)initial.coding[ch].total_width > pending->maximum_lsbs)
                pending->maximum_lsbs=(unsigned)initial.coding[ch].total_width;
            for (i = 0; i < count; ++i) residual[i*channels+ch] = output[i];
        }
        {
            planned_au *planned;
            if(pending->aus>=128) return 0;
            planned=pending->au+pending->aus++;
            planned->initial=initial;planned->main=p;planned->previous=previous;
            planned->count=(unsigned)count;planned->single_restart=is_restart && !first;
            planned->end_markers=frames+count==in->frames;planned->bypass_bits=stream.bypass_bits;
            memcpy(planned->residual,residual,count*channels*sizeof(*residual));
            memcpy(planned->unpredicted,unpredicted,count*channels*sizeof(*unpredicted));
            if(stream.bypass_bits) memcpy(planned->bypass,stream.bypass,count);
            planned->stamp=0;if(stamp && !mlp_stamp_next(stamp,&planned->stamp)) return 0;
            if(is_restart) pending->restart=restart;
        }
        {
            int32_t noise_a[160],noise_b[160];
            /* 10007a40 advances the forward noise stream for every AU,
             * even when no matrix references the noise channels. */
            if (mlp_matrix_noise(&noise_seed,count,0,noise_a,noise_b)) return 0;
        }
        previous = p; ++au; timing = (timing+(unsigned)count)&65535;
        frames += count;
        if (matrix) { ++matrix->slot; matrix->offset += (unsigned)count; }
    }
    if ((stamp && !mlp_stamp_complete(stamp)) || !flush_interval(queue,pending,&in->format,&rate,&words_total) || !mlp_output_queue_finish(queue)) return 0;
    if(out) {
    fprintf(stderr,"Encoded %llu frames, %u channels, %u Hz, %u bits; %u AUs, %u restarts, %llu bytes; predicted=%u fallback=%u\n",
        (unsigned long long)frames,channels,in->format.sample_rate,in->format.bits,au,restarted,
        (unsigned long long)(words_total*2),predicted_channels,fallback_channels);
    if (matrix) fprintf(stderr,"Matrix intervals=%u primitives=%u bypasses=%u truncated=%u\n",
        matrix_intervals,matrix_primitives,matrix_bypasses,matrix_truncated);
    if (matrix && matrix->original_scale) fprintf(stderr,"Scaling intervals=%u shifted_channels=%u qss_changes=%u\n",
        scale_intervals,shifted_channels,qss_changes);
    if (cycle) fprintf(stderr,"boundary cycle=%u preferred_span=%u restarts=%u\n",cycle,restart_interval,restarted);
    if (matrix && matrix->joint_search) fprintf(stderr,"Search channel_intervals=%u skipped=%u feedback_AU_channels=%u\n",
        search_intervals,search_skipped,feedback_channels);
    }
    return frames == in->frames && (!out || !ferror(out));
}
typedef struct host_stream {
    mlp_encoder_read read;mlp_encoder_write write;void *input,*output;
    mlp_encoder_result *result;unsigned caller_fp;
} host_stream;
/* Windows x64 _controlfp does not provide x87 precision control.
 * Keep the original 53-bit x87 evaluation and round-to-nearest explicitly,
 * and restore both x87 and SSE state around managed host callbacks. */
static unsigned host_current_fp(void)
{
#if defined(_WIN32) && defined(__x86_64__)
    uint16_t cw;uint32_t mxcsr;
    __asm__ __volatile__("fnstcw %0" : "=m"(cw));
    __asm__ __volatile__("stmxcsr %0" : "=m"(mxcsr));
    return ((unsigned)cw<<16)|(mxcsr&65535u);
#elif defined(_WIN32)
    return _controlfp(0,0);
#else
    return 0;
#endif
}
static void host_fp(unsigned control)
{
#if defined(_WIN32) && defined(__x86_64__)
    uint16_t cw=(uint16_t)(control>>16);uint32_t mxcsr=control&65535u;
    __asm__ __volatile__("fldcw %0" : : "m"(cw));
    __asm__ __volatile__("ldmxcsr %0" : : "m"(mxcsr));
#elif defined(_WIN32)
    _controlfp(control,_MCW_PC|_MCW_RC);
#else
    (void)control;
#endif
}
static unsigned codec_fp(void)
{
#if defined(_WIN32) && defined(__x86_64__)
    return 0x027f1f80u;
#elif defined(_WIN32)
    return _PC_53|_RC_NEAR;
#else
    return 0;
#endif
}
static int host_read(void *opaque,int32_t *pcm,size_t capacity,size_t *frames)
{
    host_stream *s=opaque;int rc;
    host_fp(s->caller_fp);rc=s->read(s->input,pcm,capacity,frames);host_fp(codec_fp());
    if(rc || !*frames || *frames>capacity) { s->result->status=MLP_ENCODER_INPUT;return -1; }
    s->result->input_frames+=*frames;return 0;
}
static int host_write(void *opaque,const uint32_t *words,size_t count)
{
    host_stream *s=opaque;uint8_t bytes[8190];size_t i;int rc;
    if(count>4095) return 0;
    for(i=0;i<count;++i) {bytes[2*i]=(uint8_t)(words[i]>>8);bytes[2*i+1]=(uint8_t)words[i];}
    host_fp(s->caller_fp);rc=s->write(s->output,bytes,count*2);host_fp(codec_fp());
    if(rc) { s->result->status=MLP_ENCODER_OUTPUT;return 0; }
    s->result->output_bytes+=count*2;++s->result->access_units;return 1;
}
uint32_t MLP_ENCODER_CALL mlp_encoder_abi_version(void) { return MLP_ENCODER_ABI_VERSION; }
static int encode_stream_common(const mlp_encoder_config *c,
    mlp_encoder_read read,void *input,mlp_encoder_write write,void *output,mlp_encoder_result *result,
    unsigned assignment,unsigned group2_bits,unsigned group2_rate)
{
    mlp_pcm pcm={0};mlp_stamp_state stamp={0};unsigned interval;
    matrix_interval *matrix=NULL;interval_output *pending=NULL;mlp_output_queue *queue=NULL;
    host_stream host={read,write,input,output,result,0};int ok=0;const char *failure=NULL;
    if(!result) return MLP_ENCODER_INVALID;
    memset(result,0,sizeof(*result));result->status=MLP_ENCODER_INVALID;
    if(!c || c->struct_size!=sizeof(*c) || c->abi_version!=MLP_ENCODER_ABI_VERSION ||
       !read || !write || !c->frames || c->frames>UINT64_MAX-160 ||
       (assignment==UINT32_MAX ? mlp_format_init(&pcm.format,c->sample_rate,c->bits,c->channels) :
        mlp_format_init_assignment(&pcm.format,c->sample_rate,c->bits,assignment))) goto invalid;
    if(pcm.format.channels!=c->channels) goto invalid;
    if(group2_bits && mlp_format_set_group2_bits(&pcm.format,group2_bits)) goto invalid;
    if(group2_rate && mlp_format_set_group2_rate(&pcm.format,group2_rate)) goto invalid;
    /* Format admission follows DVD-Audio, not SurCode's GUI policy.
     * mlp_format_init permits mono/stereo at 176.4/192 kHz. */
    interval=c->restart_interval?c->restart_interval:8;
    if(interval>MLP_SCALE_BLOCKS) goto invalid;
    pcm.frames=pcm.remaining=c->frames;pcm.read_callback=host_read;pcm.read_opaque=&host;
    if(mlp_pcm_pad_final(&pcm) || pcm.frames/pcm.format.au_samples>UINT32_MAX ||
       !mlp_stamp_init(&stamp,c->metadata,c->metadata_count,pcm.frames/pcm.format.au_samples)) goto invalid;
    matrix=calloc(1,sizeof(*matrix));pending=calloc(1,sizeof(*pending));queue=calloc(1,sizeof(*queue));
    if(!matrix || !pending || !queue) {result->status=MLP_ENCODER_MEMORY;goto done;}
    matrix->original_scale=matrix->enable_matrix=matrix->joint_search=1;
    mlp_output_queue_init(queue,host_write,&host);
#ifdef _WIN32
    host.caller_fp=host_current_fp();
#endif
    host_fp(codec_fp());result->status=MLP_ENCODER_OK;
    ok=encode(&pcm,NULL,interval,pcm.format.sample_rate/pcm.format.au_samples,1,matrix,0,pending,queue,&stamp);
    host_fp(host.caller_fp);
    if(ok) result->encoded_frames=pcm.frames;
    else if(result->status==MLP_ENCODER_OK) result->status=pcm.error[0]?MLP_ENCODER_INPUT:MLP_ENCODER_FAILED;
done:
    if(pending) failure=pending->failure;
    mlp_output_queue_dispose(queue);free(queue);free(pending);free(matrix);
    if(result->status) snprintf(result->error,sizeof(result->error),"%s",pcm.error[0]?pcm.error:
        failure?failure:
        result->status==MLP_ENCODER_MEMORY?"Encoder allocation failed":
        result->status==MLP_ENCODER_OUTPUT?"Host output callback failed; discard partial output":
        "Encoding failed; discard partial output");
    return result->status;
invalid:
    snprintf(result->error,sizeof(result->error),"Invalid encoder configuration, frame count or explicit metadata");
    return result->status;
}
int MLP_ENCODER_CALL mlp_encode_stream(const mlp_encoder_config *c,
    mlp_encoder_read read,void *input,mlp_encoder_write write,void *output,mlp_encoder_result *result)
{
    return encode_stream_common(c,read,input,write,output,result,UINT32_MAX,0,0);
}
int MLP_ENCODER_CALL mlp_encode_stream_layout(const mlp_encoder_config *c,unsigned assignment,
    mlp_encoder_read read,void *input,mlp_encoder_write write,void *output,mlp_encoder_result *result)
{
    /* Reject the private default-layout sentinel at the public boundary. */
    if(assignment>20) assignment=21;
    return encode_stream_common(c,read,input,write,output,result,assignment,0,0);
}
int MLP_ENCODER_CALL mlp_encode_stream_depths(const mlp_encoder_config *c,unsigned assignment,
    unsigned group2_bits,mlp_encoder_read read,void *input,mlp_encoder_write write,void *output,
    mlp_encoder_result *result)
{
    if(assignment>20) assignment=21;
    if(!group2_bits) group2_bits=1;
    return encode_stream_common(c,read,input,write,output,result,assignment,group2_bits,0);
}
#include "mlp_group_input.inc"
#ifndef MLP_ENCODER_LIBRARY
static int number(const char *s, unsigned *out)
{
    char *end; unsigned long value;
    if (!s || !*s || *s == '-') return 0;
    errno = 0; value = strtoul(s,&end,10);
    if (errno || *end || value > 1000000) return 0;
    *out = (unsigned)value; return 1;
}
static int encode_main(int argc, char **argv)
{
    mlp_pcm input; mlp_format raw; FILE *out;
    matrix_interval *matrix = NULL;
    interval_output *pending = NULL;
    mlp_output_queue *queue = NULL;
    mlp_stamp_context stamp={0};const char *stamp_path=NULL;
    unsigned interval = 16,cycle = 0,assignment=UINT32_MAX,group2_bits=0; int i, use_raw = 0, predict = 1, fd, ok, use_matrix = 0, allow_bypass = 0, original_scale = 0, joint_search = 0, pad_final = 0, original_mode = 0;
    if (argc < 3) goto usage;
#ifdef MLP_TWO_STREAM_EMBEDDED
    for(i=3;i<argc;++i) if(!strcmp(argv[i],"--downmix")) return mlp_two_stream_main(argc,argv);
#endif
    for (i = 3; i < argc; ++i) {
        if (!strcmp(argv[i],"--raw") && i+3 < argc) {
            unsigned rate, bits, channels;
            if (use_raw || !number(argv[i+1],&rate) || !number(argv[i+2],&bits) ||
                !number(argv[i+3],&channels) || mlp_format_init(&raw,rate,bits,channels)) goto usage;
            use_raw = 1; i += 3;
        } else if (!strcmp(argv[i],"--assignment") && i+1<argc) {
            if(assignment!=UINT32_MAX || !number(argv[++i],&assignment) || assignment>20) goto usage;
        } else if (!strcmp(argv[i],"--group2-bits") && i+1<argc) {
            if(group2_bits || !number(argv[++i],&group2_bits) || !group2_bits) goto usage;
        } else if (!strcmp(argv[i],"--restart") && i+1 < argc) {
            if (!number(argv[++i],&interval) || !interval || interval > 128) goto usage;
        } else if (!strcmp(argv[i],"--cycle") && i+1 < argc) {
            if (!number(argv[++i],&cycle) || !cycle || cycle >= MLP_INTERVAL_SLOTS) goto usage;
        } else if (!strcmp(argv[i],"--original")) {
            interval=8;use_matrix=original_scale=joint_search=pad_final=original_mode=1;predict=1;
        } else if (!strcmp(argv[i],"--pad-final")) pad_final=1;
        else if (!strcmp(argv[i],"--no-predict")) predict = 0;
        else if (!strcmp(argv[i],"--matrix")) use_matrix = 1;
        else if (!strcmp(argv[i],"--matrix-bypass")) { use_matrix = 1; allow_bypass = 1; }
        else if (!strcmp(argv[i],"--original-scale")) { use_matrix = 1; original_scale = 1; }
        else if (!strcmp(argv[i],"--joint-search")) joint_search = 1;
        else if (!strcmp(argv[i],"--stamp-context") && !stamp_path && i+1<argc) stamp_path=argv[++i];
        else goto usage;
    }
    if (original_scale && interval > MLP_SCALE_BLOCKS) goto usage;
    if (joint_search && !predict) goto usage;
    if (mlp_pcm_open(&input,argv[1],use_raw ? &raw : NULL)) { fprintf(stderr,"%s\n",input.error); return 1; }
    if(assignment!=UINT32_MAX) {
        mlp_format layout;
        if(mlp_format_init_assignment(&layout,input.format.sample_rate,input.format.bits,assignment) ||
           layout.channels!=input.format.channels || (!use_raw && layout.channel_mask!=input.format.channel_mask)) {
            fprintf(stderr,"DVD-Audio assignment does not match the input format\n");
            mlp_pcm_close(&input);return 1;
        }
        input.format=layout;
    }
    if(group2_bits && mlp_format_set_group2_bits(&input.format,group2_bits)) {
        fprintf(stderr,"Invalid DVD-Audio second-group precision\n");mlp_pcm_close(&input);return 1;
    }
    /* Original job Init assigns descriptor+1c = 1102 for the 44.1-kHz
     * family, 1200 for the 48-kHz family, unless flag 0x8000 disables it.
     * 10007470 distributes preferred spans across that complete cycle. */
    if(original_mode && !cycle) cycle=input.format.sample_rate/input.format.au_samples;
    if (pad_final && mlp_pcm_pad_final(&input)) { mlp_pcm_close(&input); return 1; }
    if (input.frames < 8) {
        fprintf(stderr,"MLP blocks need at least 8 samples; input is too short (no padding applied)\n");
        mlp_pcm_close(&input); return 1;
    }
    if(stamp_path && !mlp_stamp_load_file(&stamp,stamp_path)) {
        fprintf(stderr,"Invalid explicit metadata context\n");mlp_pcm_close(&input);return 1;
    }
    fd = open_fd(argv[2],O_WRONLY|O_CREAT|O_EXCL|O_BINARY,0600);
    if (fd < 0) { perror(argv[2]); mlp_stamp_dispose(&stamp);mlp_pcm_close(&input); return 1; }
    out = fd_file(fd,"wb");
    if (!out) { close_fd(fd); remove(argv[2]); mlp_stamp_dispose(&stamp);mlp_pcm_close(&input); return 1; }
    if (use_matrix || joint_search) matrix = calloc(1,sizeof(*matrix));
    if (matrix) { matrix->original_scale = original_scale; matrix->enable_matrix = use_matrix; matrix->joint_search = joint_search; }
    pending = calloc(1,sizeof(*pending));
    queue = calloc(1,sizeof(*queue));
    if(queue) mlp_output_queue_init(queue,emit_words,out);
    ok = pending && queue && (!(use_matrix || joint_search) || matrix) && encode(&input,out,interval,cycle,predict,matrix,allow_bypass,pending,queue,stamp_path ? &stamp.state : NULL);
    mlp_stamp_dispose(&stamp);
    mlp_output_queue_dispose(queue);free(queue);
    free(pending);
    free(matrix);
    if (fclose(out)) ok = 0;
    if (!ok && input.error[0]) fprintf(stderr,"%s\n",input.error);
    mlp_pcm_close(&input);
    if (!ok) { remove(argv[2]); fprintf(stderr,"Encoding failed; partial output removed\n"); return 1; }
    return 0;
usage:
#ifdef MLP_TWO_STREAM_EMBEDDED
    fprintf(stderr,"Two-substream downmix: mlp_encode input.wav|aiff output.mlp --downmix MATRIX.txt [--matrix-search] [--joint-search]\n");
#endif
    fprintf(stderr,"Usage: mlp_encode input.wav|aiff output.mlp [--raw RATE BITS CHANNELS] [--restart 1..128] [--cycle 1..1264] [--no-predict|--joint-search] [--matrix|--matrix-bypass|--original-scale]\n"
                   "Raw PCM: signed little-endian, 16 bits in 2 bytes or 20/24 bits left-aligned in 3 bytes.\n"
                   "Standard layouts: mono, stereo, 3.0, quad, 5.0, 5.1. Existing output is never overwritten.\n"
                   "Matrix options use selection with an independent fixed-interval policy.\n"
                   "--original-scale also uses shift/QSS planning; restart interval is limited to 32.\n"
                   "--cycle enables original balanced restart decisions, using --restart as the preferred span.\n");
    fprintf(stderr,"--stamp-context FILE reproduces explicit metadata TLVs/update timing; it contains no audio data.\n");
    fprintf(stderr,"--pad-final zero-pads the last AU. --original selects preferred restart=8, original cycle/scale/matrix, joint search and final padding.\n");
    return 2;
}
int main(int argc, char **argv)
{
    int result;
#ifdef _WIN32
    /* SurCode startup 00427948 sets _PC_53, not MinGW's x87 PC64.
     * Scope the precision change to this encoding invocation. */
    unsigned saved = _controlfp(0,0);
    _controlfp(_PC_53,_MCW_PC);
#endif
    result = encode_main(argc,argv);
#ifdef _WIN32
    _controlfp(saved,_MCW_PC);
#endif
    return result;
}
#endif
