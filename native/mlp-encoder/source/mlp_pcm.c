#define _POSIX_C_SOURCE 200809L
#include "mlp_pcm.h"
#include <math.h>
#include <string.h>
#ifdef _WIN32
#define seek64 _fseeki64
#define tell64 _ftelli64
#else
#define seek64 fseeko
#define tell64 ftello
#endif
static uint32_t integer(const unsigned char *p, unsigned n, int be)
{
    uint32_t v = 0; unsigned i;
    for (i = 0; i < n; ++i) v |= (uint32_t)p[be ? n-1-i : i] << (8*i);
    return v;
}
static int fail(mlp_pcm *p, const char *message)
{
    snprintf(p->error,sizeof(p->error),"%s",message);
    if (p->file) { fclose(p->file); p->file = NULL; }
    return -1;
}
int mlp_pcm_open(mlp_pcm *p, const char *path, const mlp_format *raw)
{
    unsigned char h[64];
    uint64_t size, end, pos, data_size = 0, declared_frames = 0;
    unsigned channels = 0, rate = 0, bits = 0, mask = 0, alignment = 0;
    int wave = 0, aifc, have_fmt = 0, have_data = 0;
    if (!p || !path) return -1;
    memset(p,0,sizeof(*p));
    p->file = fopen(path,"rb");
    if (!p->file) return fail(p,"Cannot open PCM input");
    if (seek64(p->file,0,SEEK_END) || tell64(p->file) < 0) return fail(p,"Input must be seekable");
    size = (uint64_t)tell64(p->file);
    if (seek64(p->file,0,SEEK_SET)) return fail(p,"Cannot seek input");
    if (raw) {
        if (mlp_format_init_assignment(&p->format,raw->sample_rate,raw->bits,raw->assignment) ||
            p->format.channels!=raw->channels)
            return fail(p,"Unsupported raw format");
        if(raw->group2_bits && mlp_format_set_group2_bits(&p->format,raw->group2_bits))
            return fail(p,"Unsupported second-group precision");
        if(raw->group2_sample_rate && mlp_format_set_group2_rate(&p->format,raw->group2_sample_rate))
            return fail(p,"Unsupported second-group sample rate");
        p->storage_bytes = (raw->bits+7)/8; p->valid_bits = raw->bits;
        data_size = size;
        goto ready;
    }
    if (size < 12 || fread(h,1,12,p->file) != 12) return fail(p,"Truncated container");
    wave = !memcmp(h,"RIFF",4) && !memcmp(h+8,"WAVE",4);
    aifc = !memcmp(h+8,"AIFC",4);
    if (!wave && (memcmp(h,"FORM",4) || (!aifc && memcmp(h+8,"AIFF",4))))
        return fail(p,"Expected integer WAVE, AIFF or AIFC; use --raw for raw PCM");
    end = (uint64_t)integer(h+4,4,!wave)+8;
    if (end > size || end < 12) return fail(p,"Invalid container size");
    for (pos = 12; pos+8 <= end;) {
        uint64_t chunk, next;
        if (seek64(p->file,(int64_t)pos,SEEK_SET) || fread(h,1,8,p->file) != 8)
            return fail(p,"Truncated chunk header");
        chunk = integer(h+4,4,!wave); next = pos+8+chunk+(chunk&1);
        if (pos+8+chunk > end || next > end) return fail(p,"Chunk exceeds container");
        if (wave && !memcmp(h,"fmt ",4)) {
            unsigned tag, byte_rate;
            if (have_fmt || chunk < 16 || fread(h,1,(size_t)(chunk < 40 ? chunk : 40),p->file) != (chunk < 40 ? chunk : 40))
                return fail(p,"Invalid WAVE format chunk");
            tag = integer(h,2,0); channels = integer(h+2,2,0);
            rate = integer(h+4,4,0); byte_rate = integer(h+8,4,0);
            alignment = integer(h+12,2,0); bits = integer(h+14,2,0);
            p->valid_bits = bits;
            if (tag == 0xfffe) {
                static const unsigned char pcm_guid[16] = {1,0,0,0,0,0,16,0,128,0,0,170,0,56,155,113};
                if (chunk < 40 || integer(h+16,2,0) < 22 || memcmp(h+24,pcm_guid,16))
                    return fail(p,"Unsupported WAVE extensible encoding");
                p->valid_bits = integer(h+18,2,0); mask = integer(h+20,4,0);
            } else if (tag != 1) return fail(p,"Only integer PCM is supported");
            if ((bits != 16 && bits != 24 && bits != 32) || !p->valid_bits || p->valid_bits > bits ||
                channels > 6 || alignment != channels*(bits/8) || byte_rate != (uint64_t)rate*alignment)
                return fail(p,"Inconsistent WAVE format");
            p->storage_bytes = bits/8; have_fmt = 1;
        } else if (!wave && !memcmp(h,"COMM",4)) {
            double sr; unsigned exponent; uint64_t mantissa;
            if (have_fmt || chunk < (aifc ? 22u : 18u) || fread(h,1,aifc ? 22 : 18,p->file) != (aifc ? 22u : 18u))
                return fail(p,"Invalid AIFF common chunk");
            channels = integer(h,2,1); declared_frames = integer(h+2,4,1);
            bits = integer(h+6,2,1); p->valid_bits = bits;
            exponent = integer(h+8,2,1);
            mantissa = ((uint64_t)integer(h+10,4,1)<<32)|integer(h+14,4,1);
            if (exponent & 0x8000 || exponent == 0x7fff) return fail(p,"Invalid AIFF sample rate");
            sr = ldexp((double)mantissa,(int)exponent-16383-63);
            if (!isfinite(sr) || sr < 1 || sr > 192000 || floor(sr) != sr) return fail(p,"Unsupported AIFF sample rate");
            rate = (unsigned)sr; p->storage_bytes = (bits+7)/8; p->big_endian = 1;
            if (aifc) {
                if (!memcmp(h+18,"sowt",4)) p->big_endian = 0;
                else if (memcmp(h+18,"NONE",4)) return fail(p,"Compressed AIFC is unsupported");
            }
            have_fmt = 1;
        } else if ((wave && !memcmp(h,"data",4)) || (!wave && !memcmp(h,"SSND",4))) {
            unsigned offset = 0;
            if (have_data) return fail(p,"Multiple audio data chunks are unsupported");
            p->data_offset = pos+8; data_size = chunk;
            if (!wave) {
                if (chunk < 8 || fread(h,1,8,p->file) != 8) return fail(p,"Invalid AIFF sound chunk");
                offset = integer(h,4,1);
                if ((uint64_t)offset+8 > chunk) return fail(p,"Invalid AIFF data offset");
                p->data_offset += 8+offset; data_size -= 8+offset;
            }
            have_data = 1;
        }
        pos = next;
    }
    if (pos != end) return fail(p,"Truncated trailing chunk header");
    if (!have_fmt || !have_data || mlp_format_init(&p->format,rate,p->valid_bits,channels))
        return fail(p,"Missing chunks or unsupported rate/depth/channel configuration");
    if (mask && mask != p->format.channel_mask) {
        unsigned assignment;mlp_format layout;int found=0;
        for(assignment=0;assignment<21;++assignment)
            if(!mlp_format_init_assignment(&layout,rate,p->valid_bits,assignment) &&
               layout.channels==channels && layout.channel_mask==mask) {
                p->format=layout;found=1;break;
            }
        if(!found) return fail(p,"WAVE layout is not a DVD-Audio channel assignment");
    }
ready:
    if (!data_size || !p->storage_bytes || data_size % (p->storage_bytes*p->format.channels))
        return fail(p,"Empty or incomplete PCM frame");
    p->frames = data_size/(p->storage_bytes*p->format.channels);
    if (!raw && !wave && declared_frames != p->frames) return fail(p,"AIFF sample count disagrees with sound data");
    p->remaining = p->frames;
    if (seek64(p->file,(int64_t)p->data_offset,SEEK_SET)) return fail(p,"Cannot seek audio data");
    return 0;
}
int mlp_pcm_pad_final(mlp_pcm *p)
{
    uint64_t tail;
    if (!p || (!p->file && !p->read_callback) || p->remaining != p->frames || !p->format.au_samples) return -1;
    tail = (p->format.au_samples-p->frames%p->format.au_samples)%p->format.au_samples;
    if (UINT64_MAX-p->frames < tail) return -1;
    p->zero_tail += tail; p->frames += tail; p->remaining += tail;
    return 0;
}
static int group_order(const mlp_format *format,int32_t *samples,size_t frames)
{
    size_t n;unsigned ch;int32_t frame[6];
    for(n=0;n<frames;++n) {
        memcpy(frame,samples+n*format->channels,format->channels*sizeof(*frame));
        for(ch=0;ch<format->channels;++ch) {
            unsigned bits=ch<format->group1_channels?format->bits:format->group2_bits;
            uint32_t mask=(UINT32_C(1)<<(24-bits))-1;
            samples[n*format->channels+ch]=frame[format->input_order[ch]];
            if((uint32_t)samples[n*format->channels+ch]&mask) return -1;
        }
    }
    return 0;
}
int mlp_pcm_read(mlp_pcm *p, int32_t *samples, size_t capacity, size_t *frames)
{
    unsigned char bytes[160*6*4];
    size_t count, actual, i, n;
    unsigned padding;
    if (!p || (!p->file && !p->read_callback) || !samples || !frames || !capacity || capacity > 160) return -1;
    count = p->remaining < capacity ? (size_t)p->remaining : capacity;
    actual = p->remaining-p->zero_tail < count ? (size_t)(p->remaining-p->zero_tail) : count;
    if (p->read_callback) {
        size_t done=0;uint32_t mask=(UINT32_C(1)<<(24-p->format.bits))-1;
        while (done<actual) {
            size_t got=0;
            if (p->read_callback(p->read_opaque,samples+done*p->format.channels,actual-done,&got))
                return fail(p,"Host input callback failed");
            if (!got || got>actual-done) return fail(p,"Host input ended early or returned an invalid count");
            done+=got;
        }
        for (i=0;i<actual*p->format.channels;++i)
            if (samples[i]<-8388608 || samples[i]>8388607 || ((uint32_t)samples[i]&mask))
                return fail(p,"Host PCM exceeds the declared left-aligned precision");
        if (actual<count) memset(samples+actual*p->format.channels,0,(count-actual)*p->format.channels*sizeof(*samples));
        if(group_order(&p->format,samples,count)) return fail(p,"PCM exceeds its channel-group precision");
        p->zero_tail-=count-actual;p->remaining-=count;*frames=count;return 0;
    }
    n = actual*p->format.channels; padding = p->storage_bytes*8-p->valid_bits;
    if (fread(bytes,p->storage_bytes,n,p->file) != n) return fail(p,"Truncated PCM data");
    for (i = 0; i < n; ++i) {
        uint32_t v = integer(bytes+i*p->storage_bytes,p->storage_bytes,p->big_endian);
        if (padding && (v & ((1u<<padding)-1))) return fail(p,"Nonzero bits below declared PCM precision");
        if (p->storage_bytes == 4) v >>= 8;
        else v <<= 24-8*p->storage_bytes;
        samples[i] = v&0x800000 ? (int32_t)((int64_t)v-0x1000000) : (int32_t)v;
    }
    if (actual < count) memset(samples+n,0,(count-actual)*p->format.channels*sizeof(*samples));
    if(group_order(&p->format,samples,count)) return fail(p,"PCM exceeds its channel-group precision");
    p->zero_tail -= count-actual;
    p->remaining -= count; *frames = count; return 0;
}
void mlp_pcm_close(mlp_pcm *p)
{
    if (p && p->file) { fclose(p->file); p->file = NULL; }
}
