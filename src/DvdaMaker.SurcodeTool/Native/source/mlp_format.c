#include "mlp_format.h"
#include <string.h>
static uint32_t transform(uint32_t r, uint32_t last)
{
    uint32_t t = r >> 8, v = (r & 255) << 3;
    v = (v ^ (t & 0x07ff)) << 2;
    v = (v ^ (t & 0x1fff)) << 1;
    v = (v ^ (t & 0x3fff)) << 2;
    return v ^ ((t ^ last) & 65535);
}
uint16_t mlp_major_checksum(const uint16_t words[13])
{
    uint32_t r = words[0];
    unsigned i;
    for (i = 1; i < 13; ++i) { r = transform(r,0); r = transform(r,words[i]); }
    return (uint16_t)r;
}
unsigned mlp_au_parity(unsigned length, unsigned arrival,
    const unsigned *directories, unsigned count)
{
    unsigned v = length ^ arrival, p = 15, i;
    for (i = 0; i < count; ++i) v ^= directories[i];
    while (v) { p ^= v & 15; v >>= 4; }
    return p;
}
int mlp_format_init(mlp_format *f, unsigned rate, unsigned bits, unsigned channels)
{
    /* Standard WAVE layouts: mono, stereo, 3.0, quad, 5.0, 5.1.
     * Original SSF ModeBin 7/9 are 3.0/5.0; 4/6 include LFE instead. */
    static const unsigned assignments[6] = {0,1,7,3,9,12};
    if (!channels || channels > 6) return -1;
    return mlp_format_init_assignment(f,rate,bits,assignments[channels-1]);
}
int mlp_format_init_assignment(mlp_format *f,unsigned rate,unsigned bits,unsigned assignment)
{
    /* DVD-Audio assignment table, independently checked against original
     * descriptor table 1001b6d0 (six labels, group masks, channel meaning).
     * Speaker bits use WAVE: L/R/C/LFE/Ls/Rs/S = 1/2/4/8/16/32/256. */
    static const unsigned speakers[21][6] = {
        {4},{1,2},{1,2,256},{1,2,16,32},{1,2,8},
        {1,2,8,256},{1,2,8,16,32},{1,2,4},{1,2,4,256},
        {1,2,4,16,32},{1,2,4,8},{1,2,4,8,256},{1,2,4,8,16,32},
        {1,2,4,256},{1,2,4,16,32},{1,2,4,8},{1,2,4,8,256},
        {1,2,4,8,16,32},{1,2,16,32,8},{1,2,16,32,4},{1,2,16,32,4,8}
    };
    static const unsigned counts[21]={1,2,3,4,3,4,5,3,4,5,4,5,6,4,5,4,5,6,5,5,6};
    static const unsigned first[21]={1,2,2,2,2,2,2,2,2,2,2,2,2,3,3,3,3,3,4,4,4};
    mlp_format v;
    unsigned base, shift = 0,channels,i,j;
    if (!f || assignment > 20 ||
        (bits != 16 && bits != 20 && bits != 24)) return -1;
    channels=counts[assignment];
    base = rate;
    while (base > 48000 && !(base & 1)) { base /= 2; ++shift; }
    if ((base != 44100 && base != 48000) || shift > 2 ||
        (shift == 2 && channels > 2)) return -1;
    memset(&v,0,sizeof(v));
    v.sample_rate = rate; v.bits = bits; v.channels = channels;
    v.assignment=assignment;v.group1_channels=first[assignment];
    v.group2_channels=channels-v.group1_channels;
    v.group2_bits=v.group2_channels?bits:0;
    for(i=0;i<channels;++i) {
        v.channel_mask|=speakers[assignment][i];
        for(j=0;j<channels;++j)
            v.input_order[i]+=speakers[assignment][j]<speakers[assignment][i];
    }
    v.rate_code = (base == 44100 ? 8 : 0) | shift;
    v.group2_sample_rate=v.group2_channels?rate:0;
    v.group2_rate_code=v.group2_channels?v.rate_code:15;
    v.au_samples = 40u << shift;
    v.rate_field = 0x8000u | ((base == 44100 ? 3482u : 3200u) >> shift);
    *f = v; return 0;
}
int mlp_format_set_group2_bits(mlp_format *f,unsigned bits)
{
    if(!f || !f->group2_channels || bits>f->bits ||
       (bits!=16 && bits!=20 && bits!=24)) return -1;
    f->group2_bits=bits;return 0;
}
int mlp_format_set_group2_rate(mlp_format *f,unsigned rate)
{
    if(!f || !f->group2_channels ||
       (rate!=f->sample_rate && !((f->sample_rate==88200 || f->sample_rate==96000) && rate==f->sample_rate/2))) return -1;
    f->group2_sample_rate=rate;f->group2_rate_code=f->rate_code-(rate!=f->sample_rate);return 0;
}
void mlp_format_major(const mlp_format *f, uint16_t words[14])
{
    /* Original Init 1000442b: last major word comes from layout table
     * 1001b6f0, stride 36; default job metadata fields contribute zero. */
    static const uint16_t layout_meaning[21] = {
        31,27,31,25,3,31,1,26,31,24,2,31,0,31,24,2,31,0,1,24,0
    };
    unsigned depth = (f->bits-16)/4, group2 = f->group2_channels != 0;
    const uint16_t defaults[14] = {0xf872,0x6fbb,0,0,0xb752,0x4000,0,0,
                                  0x1105,0x5601,0,0x8080,0x001f,0};
    memcpy(words,defaults,sizeof(defaults));
    words[2] = (uint16_t)((depth<<12) | ((group2 ? (f->group2_bits-16)/4 : 15)<<8) |
                         (f->rate_code<<4) | (group2 ? f->group2_rate_code : 15));
    words[3] = (uint16_t)f->assignment;
    words[7] = (uint16_t)f->rate_field;
    if (f->assignment < 21) words[12] = layout_meaning[f->assignment];
    /* Original VFY 100046b0: substream-info bit 0 promises <=2 channels.
     * Profile 4 allows a six-channel primary; profile 7 is high-rate stereo. */
    words[8] = (uint16_t)(f->au_samples == 160 ? 0x1107 :
                         f->channels > 2 ? 0x1104 : 0x1105);
    words[9] = (uint16_t)((((f->rate_code&8 ? 9u : 10u)+4*(f->rate_code&3))<<11) |
                         (f->bits<<6) | ((1u<<f->channels)-1));
    words[13] = mlp_major_checksum(words);
}
