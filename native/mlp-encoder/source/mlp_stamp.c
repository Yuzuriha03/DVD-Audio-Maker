#include "mlp_stamp.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int mlp_stamp_init(mlp_stamp_state *s,const mlp_stamp_record *records,size_t count,uint64_t aus)
{
    size_t i;
    if(!s || !records || !count || !aus || records[0].start) return 0;
    for(i=0;i<count;++i) {
        const mlp_stamp_record *r=records+i;
        uint64_t period=10;uint32_t byte,declared;unsigned bit,ones=0;
        if(!r->packet || r->size<4 || r->size>65539 || r->start>=aus ||
           (i && r->start<=records[i-1].start) ||
           r->packet[2]!=0x40 || r->packet[3]!=0) return 0;
        declared=(((unsigned)r->packet[0]<<8)|r->packet[1])+4;
        if(r->valid_bits) {
            if(i+1!=count || declared<r->size || r->valid_bits<32 ||
               r->valid_bits<=8*(r->size-1) || r->valid_bits>8*r->size) return 0;
        } else if(declared!=r->size) return 0;
        /* Updates are accepted only between complete framed packets.
         * Reject impossible timelines before an output file is opened. */
        for(byte=0;byte<r->size;++byte) for(bit=8;bit;--bit) {
            if(ones==8) { ++period;ones=0; }
            ++period;ones=(r->packet[byte]>>(bit-1))&1 ? ones+1 : 0;
        }
        if(ones==8) ++period;
        if(i+1<count && (records[i+1].start-r->start)%period) return 0;
    }
    memset(s,0,sizeof(*s));s->records=records;s->count=count;s->limit=aus;
    s->bit=7;s->prefix=9;s->start=1;return 1;
}
int mlp_stamp_next(mlp_stamp_state *s,unsigned *result)
{
    const mlp_stamp_record *r;unsigned bit;
    if(!s || !result || s->failed || !s->records || s->position>=s->limit) return 0;
    if(s->index+1<s->count && s->records[s->index+1].start<=s->position) {
        if(!s->start || s->records[s->index+1].start!=s->position) { s->failed=1;return 0; }
        ++s->index;
    }
    s->start=0;r=s->records+s->index;
    if(s->prefix) { bit=1;--s->prefix; }
    else if(s->ones==8) { bit=0;s->ones=0; }
    else if(r->valid_bits && s->byte*8+7-s->bit>=r->valid_bits) { s->failed=1;return 0; }
    else if(s->byte==r->size) {
        bit=0;s->byte=0;s->bit=7;s->prefix=9;s->ones=0;s->start=1;
    } else {
        bit=(r->packet[s->byte]>>s->bit)&1;
        s->ones=bit ? s->ones+1 : 0;
        if(!s->bit) { s->bit=7;++s->byte; } else --s->bit;
    }
    ++s->position;*result=bit;return 1;
}
int mlp_stamp_complete(const mlp_stamp_state *s)
{ return s && !s->failed && s->position==s->limit && s->index+1==s->count; }
static int little(FILE *f,unsigned bytes,uint64_t *v)
{
    unsigned i;uint64_t result=0;
    for(i=0;i<bytes;++i) { int c=fgetc(f);if(c==EOF) return 0;result|=(uint64_t)(unsigned)c<<(8*i); }
    *v=result;return 1;
}
void mlp_stamp_dispose(mlp_stamp_context *c)
{
    size_t i;if(!c) return;
    for(i=0;i<c->count;++i) free((void *)c->records[i].packet);
    free(c->records);memset(c,0,sizeof(*c));
}
int mlp_stamp_load_file(mlp_stamp_context *c,const char *path)
{
    FILE *f;uint8_t magic[8];uint64_t count,aus,size,valid,total=0;size_t i;int version;
    mlp_stamp_context work={0};int ok=0;
    if(!c || !path || !(f=fopen(path,"rb"))) return 0;
    if(fread(magic,1,8,f)!=8) goto done;
    version=!memcmp(magic,"MSCTX001",8)?1:!memcmp(magic,"MSCTX002",8)?2:0;
    if(!version ||
       !little(f,8,&aus) || !little(f,4,&count) || !count || count>4096) goto done;
    work.count=(size_t)count;work.records=calloc(work.count,sizeof(*work.records));
    if(!work.records) { work.count=0;goto done; }
    for(i=0;i<work.count;++i) {
        uint8_t *data;
        if(!little(f,8,&work.records[i].start) || !little(f,4,&size) || size<4 || size>65539) goto done;
        if(version==2) { if(!little(f,4,&valid)) goto done;work.records[i].valid_bits=(uint32_t)valid; }
        total+=size;if(total>16777216) goto done;
        data=malloc((size_t)size);if(!data) goto done;
        work.records[i].packet=data;work.records[i].size=(uint32_t)size;
        if(fread(data,1,(size_t)size,f)!=(size_t)size) goto done;
    }
    if(fgetc(f)!=EOF || !mlp_stamp_init(&work.state,work.records,work.count,aus)) goto done;
    *c=work;memset(&work,0,sizeof(work));ok=1;
done:
    fclose(f);mlp_stamp_dispose(&work);return ok;
}
