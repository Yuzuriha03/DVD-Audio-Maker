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
        if(end>2048 || end<=at+6)return -2;
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
