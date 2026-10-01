#ifndef MLP_STAMP_H
#define MLP_STAMP_H
#include "mlp_bits.h"
typedef struct mlp_stamp_record {
    uint64_t start; uint32_t size; const uint8_t *packet;
    /* Zero means a complete packet; otherwise only this prefix is known.
     * A partial packet may occur only at the end of the metadata context. */
    uint32_t valid_bits;
} mlp_stamp_record;
typedef struct mlp_stamp_state {
    const mlp_stamp_record *records; size_t count,index;
    uint64_t position,limit; uint32_t byte;
    unsigned bit,prefix,ones,start; int failed;
} mlp_stamp_state;
typedef struct mlp_stamp_context { mlp_stamp_record *records; size_t count; mlp_stamp_state state; } mlp_stamp_context;
/* Pure 10003f20 framing. Caller-owned TLV packets and update AU positions
 * are explicit metadata inputs, independent of audio coding decisions. */
MLP_BITS_API int mlp_stamp_init(mlp_stamp_state *state,const mlp_stamp_record *records,size_t count,uint64_t aus);
MLP_BITS_API int mlp_stamp_next(mlp_stamp_state *state,unsigned *bit);
MLP_BITS_API int mlp_stamp_complete(const mlp_stamp_state *state);
MLP_BITS_API int mlp_stamp_load_file(mlp_stamp_context *context,const char *path);
MLP_BITS_API void mlp_stamp_dispose(mlp_stamp_context *context);
#endif
