/* Streaming DVD-Audio PES/MLP comparison. This code only reads data.
 * It compares every MLP byte, including headers and end markers, in track
 * order across all AOB segments. Padding packets are excluded by PES length. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef int (*ReadChunk)(void *,unsigned char *,unsigned);
typedef struct VerifyResult { int code,track; uint64_t offset,sectors,bytes; } VerifyResult;
typedef struct Compare {
    const char *const *paths;unsigned count,index;FILE *source;uint64_t left,offset;
    VerifyResult *result;
} Compare;
static FILE *open_utf8(const char *path) {
    int n=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,path,-1,NULL,0);
    if(!n)return NULL;
    wchar_t *w=malloc((size_t)n*sizeof(*w));if(!w)return NULL;
    MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,path,-1,w,n);
    FILE *f=_wfopen(w,L"rb");free(w);return f;
}
static int consume(Compare *c,const unsigned char *data,unsigned length) {
    unsigned char expected[2048];
    while(length) {
        c->result->track=(int)c->index;c->result->offset=c->offset;
        if(!c->source) {
            if(c->index==c->count)return -6;
            c->source=open_utf8(c->paths[c->index]);
            if(!c->source || _fseeki64(c->source,0,SEEK_END))return -4;
            int64_t size=_ftelli64(c->source);
            if(size<=0 || _fseeki64(c->source,0,SEEK_SET))return -4;
            c->left=(uint64_t)size;c->offset=0;
        }
        unsigned n=c->left<length ? (unsigned)c->left : length;
        if(fread(expected,1,n,c->source)!=n)return -4;
        for(unsigned i=0;i<n;i++)if(data[i]!=expected[i]) {
            c->result->offset=c->offset+i;return -5;
        }
        c->offset+=n;c->left-=n;data+=n;length-=n;c->result->bytes+=n;
        if(!c->left){fclose(c->source);c->source=NULL;c->index++;c->offset=0;}
    }
    return 0;
}
static unsigned u16(const unsigned char *p) {return ((unsigned)p[0]<<8)|p[1];}
static int sector(Compare *c,const unsigned char *s) {
    if(memcmp(s,"\0\0\1\272",4) || (s[4]&0xc0)!=0x40)return -1;
    unsigned at=14+(s[13]&7);
    while(at<2048) {
        if(at+4<=2048 && !memcmp(s+at,"\0\0\1\271",4)) {
            at+=4;
            while(at<2048)if(s[at++]!=0xff)return -2;
            break;
        }
        if(at+6>2048 || memcmp(s+at,"\0\0\1",3))return -2;
        unsigned end=at+6+u16(s+at+4),id=s[at+3];
        if(end>2048 || (end==at+6 && id!=0xbe))return -2;
        if(id==0xbd) {
            if(at+9>end || (s[at+6]&0xc0)!=0x80)return -2;
            unsigned private_at=at+9+s[at+8];
            if(private_at+4>end)return -2;
            if(s[private_at]!=0xa1)return -3;
            unsigned audio=private_at+4+s[private_at+3];
            if(s[private_at+3]<6 || audio>end)return -2;
            int status=consume(c,s+audio,end-audio);if(status)return status;
        } else if(id!=0xbb && id!=0xbe)return -3;
        at=end;
    }
    c->result->sectors++;return 0;
}
__declspec(dllexport) int dvda_verify_mlp_payload(const char *const *paths,unsigned count,
    ReadChunk read,void *opaque,VerifyResult *result) {
    if(!result)return -9;
    memset(result,0,sizeof(*result));
    if(!paths || !count || !read)return result->code=-9;
    Compare c={paths,count,0,NULL,0,0,result};
    unsigned char *buffer=malloc(128*1024);int status=buffer ? 0 : -4;
    while(!status) {
        int n=read(opaque,buffer,128*1024);
        if(n<0 || n>128*1024 || n%2048){status=-9;break;}
        if(!n)break;
        for(int at=0;at<n;at+=2048)if((status=sector(&c,buffer+at)))break;
    }
    if(!status && (c.source || c.index!=count)) {
        status=-7;result->track=(int)c.index;result->offset=c.offset;
    }
    if(c.source)fclose(c.source);
    free(buffer);result->code=status;return status;
}

/* LPCM byte packing follows the GPL DVD-Audio author source in
 * tools/dvda-author-mlp8/src/audio.c (interleave_*_sample_extended).
 * Compare source PCM in two-frame units. Only the mandatory final zero
 * frame of an odd-length title is generated. No data is rewritten. */
static const unsigned char pcm_order[2][6][36]={
 {{1,0,3,2},{1,0,3,2,5,4,7,6},
  {5,4,11,10,1,0,3,2,7,6,9,8},
  {5,4,7,6,13,12,15,14,1,0,3,2,9,8,11,10},
  {7,6,9,8,17,16,19,18,1,0,3,2,5,4,11,10,13,12,15,14},
  {7,6,9,8,11,10,19,18,21,20,23,22,1,0,3,2,5,4,13,12,15,14,17,16}},
 {{2,1,5,4,0,3},{2,1,5,4,8,7,11,10,0,3,6,9},
  {8,7,17,16,6,15,2,1,5,4,11,10,14,13,0,3,9,12},
  {8,7,11,10,20,19,23,22,6,9,18,21,2,1,5,4,14,13,17,16,0,3,12,15},
  {11,10,14,13,26,25,29,28,9,12,24,27,2,1,5,4,8,7,17,16,20,19,23,22,0,3,6,15,18,21},
  {8,7,11,10,26,25,29,28,6,9,24,27,2,1,5,4,14,13,17,16,20,19,23,22,32,31,35,34,0,3,12,15,18,21,30,33}}
};
static unsigned le16(const unsigned char *p){return p[0]|((unsigned)p[1]<<8);}
static uint32_t le32(const unsigned char *p){return le16(p)|((uint32_t)le16(p+2)<<16);}
typedef struct PcmCompare {
    Compare input;const unsigned char *ends;unsigned channels,bits,rate,cga;
    unsigned char packed[36];unsigned at,length;
} PcmCompare;
static int pcm_open(PcmCompare *p) {
    Compare *c=&p->input;unsigned char h[40];
    c->source=open_utf8(c->paths[c->index]);
    if(!c->source || fread(h,1,12,c->source)!=12 || memcmp(h,"RIFF",4) || memcmp(h+8,"WAVE",4))return -4;
    uint64_t end=(uint64_t)le32(h+4)+8,data=0,size=0;unsigned bits=0,channels=0,rate=0,mask=0,align=0;
    if(_fseeki64(c->source,0,SEEK_END) || (uint64_t)_ftelli64(c->source)!=end || _fseeki64(c->source,12,SEEK_SET))return -4;
    while((uint64_t)_ftelli64(c->source)+8<=end) {
        if(fread(h,1,8,c->source)!=8)return -4;
        uint32_t n=le32(h+4);uint64_t pos=(uint64_t)_ftelli64(c->source),next=pos+n+(n&1);
        if(next>end)return -4;
        if(!memcmp(h,"fmt ",4)) {
            if(bits || n<16 || fread(h,1,n<40?n:40,c->source)!=(n<40?n:40))return -4;
            unsigned tag=le16(h);channels=le16(h+2);rate=le32(h+4);align=le16(h+12);bits=le16(h+14);
            if(tag==0xfffe) {
                static const unsigned char guid[]={1,0,0,0,0,0,16,0,128,0,0,170,0,56,155,113};
                if(n<40 || le16(h+16)<22 || le16(h+18)!=bits || memcmp(h+24,guid,16))return -4;
                mask=le32(h+20);
            } else if(tag!=1)return -4;
        } else if(!memcmp(h,"data",4)) {if(data)return -4;data=pos;size=n;}
        if(_fseeki64(c->source,(int64_t)next,SEEK_SET))return -4;
    }
    if(channels<1 || channels>6 || (bits!=16 && bits!=24) || align!=channels*(bits/8) ||
       !data || !size || size%align || (uint64_t)rate*channels*bits>9600000)return -4;
    if(rate!=44100 && rate!=48000 && rate!=88200 && rate!=96000 && rate!=176400 && rate!=192000)return -4;
    if(rate>96000 && channels>2)return -4;
    const unsigned masks[]={4,3,0x103,0x33,0xb,0x10b,0x3b,7,0x107,0x37,0xf,0x10f,0x3f};
    const unsigned defaults[]={4,3,7,0x33,0x37,0x3f};
    if(!mask)mask=defaults[channels-1];
    unsigned cga=0;for(;cga<sizeof(masks)/sizeof(masks[0]);cga++)if(masks[cga]==mask)break;
    if(cga==sizeof(masks)/sizeof(masks[0]))return -4;
    if(p->channels && (channels!=p->channels || bits!=p->bits || rate!=p->rate || cga!=p->cga))return -8;
    p->channels=channels;p->bits=bits;p->rate=rate;p->cga=cga;c->left=size;c->offset=0;
    return _fseeki64(c->source,(int64_t)data,SEEK_SET)?-4:0;
}
static int pcm_pair(PcmCompare *p) {
    Compare *c=&p->input;unsigned char raw[36]={0};unsigned used=0,unit=p->channels*(p->bits/8)*2;
    while(used<unit) {
        if(!c->source) {
            if(c->index==c->count)return -6;
            int status=pcm_open(p);if(status)return status;
        }
        unsigned n=c->left<unit-used?(unsigned)c->left:unit-used;
        if(fread(raw+used,1,n,c->source)!=n)return -4;
        c->result->track=(int)c->index;c->result->offset=c->offset;
        c->left-=n;c->offset+=n;used+=n;
        if(!c->left) {
            unsigned ended=c->index;fclose(c->source);c->source=NULL;c->index++;c->offset=0;
            if(p->ends[ended])break;
        }
    }
    if(used!=unit && used!=unit/2)return -8;
    for(unsigned i=0;i<unit;i++)p->packed[i]=raw[pcm_order[p->bits==24][p->channels-1][i]];
    p->at=0;p->length=unit;return 0;
}
static int pcm_sector(PcmCompare *p,const unsigned char *s) {
    if(memcmp(s,"\0\0\1\272",4) || (s[4]&0xc0)!=0x40)return -1;
    unsigned at=14+(s[13]&7);
    while(at<2048) {
        if(at+4<=2048 && !memcmp(s+at,"\0\0\1\271",4)) {
            at+=4;while(at<2048)if(s[at++]!=0xff)return -2;break;
        }
        if(at+6>2048 || memcmp(s+at,"\0\0\1",3))return -2;
        unsigned end=at+6+u16(s+at+4),id=s[at+3];
        if(end>2048 || (end==at+6 && id!=0xbe))return -2;
        if(id==0xbd) {
            if(at+9>end || (s[at+6]&0xc0)!=0x80)return -2;
            unsigned q=at+9+s[at+8];
            if(q+12>end || s[q]!=0xa0 || s[q+3]<8)return -3;
            unsigned audio=q+4+s[q+3];if(audio>end)return -2;
            unsigned rate=p->rate==48000?0:p->rate==96000?1:p->rate==192000?2:p->rate==44100?8:p->rate==88200?9:10;
            unsigned bit=p->bits==24?2:0;
            if(s[q+7]!=(bit<<4|(p->channels<=2?15:bit)) ||
               s[q+8]!=(rate<<4|(p->channels<=2?15:rate)) || s[q+10]!=p->cga)return -8;
            for(;audio<end;audio++) {
                if(p->at==p->length){int status=pcm_pair(p);if(status)return status;}
                if(s[audio]!=p->packed[p->at++])return -5;
                p->input.result->bytes++;
            }
        } else if(id!=0xbb && id!=0xbe)return -3;
        at=end;
    }
    p->input.result->sectors++;return 0;
}
__declspec(dllexport) int dvda_verify_lpcm_payload(const char *const *paths,const unsigned char *title_ends,unsigned count,
    ReadChunk read,void *opaque,VerifyResult *result) {
    if(!result)return -9;
    memset(result,0,sizeof(*result));
    if(!paths || !title_ends || !count || !read || !title_ends[count-1])return result->code=-9;
    PcmCompare p={0};p.input.paths=paths;p.input.count=count;p.input.result=result;p.ends=title_ends;
    int status=pcm_open(&p);unsigned char *buffer=malloc(128*1024);if(!buffer)status=-4;
    while(!status) {
        int n=read(opaque,buffer,128*1024);
        if(n<0 || n>128*1024 || n%2048){status=-9;break;}
        if(!n)break;
        for(int at=0;at<n;at+=2048)if((status=pcm_sector(&p,buffer+at)))break;
    }
    if(!status && (p.input.source || p.input.index!=count || p.at!=p.length))status=-7;
    if(p.input.source)fclose(p.input.source);
    free(buffer);result->code=status;return status;
}
