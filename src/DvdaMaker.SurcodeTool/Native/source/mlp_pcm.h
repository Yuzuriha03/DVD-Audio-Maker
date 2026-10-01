#ifndef MLP_PCM_H
#define MLP_PCM_H
#include "mlp_format.h"
#include <stdio.h>
typedef struct mlp_pcm {
    FILE *file;
    mlp_format format;
    uint64_t frames, remaining, data_offset, zero_tail;
    unsigned storage_bytes, valid_bits, big_endian;
    int (*read_callback)(void *,int32_t *,size_t,size_t *);
    void *read_opaque;
    char error[192];
} mlp_pcm;
/* Auto-detect integer PCM WAVE/AIFF/AIFC, or explicit raw format.
 * Samples are returned in signed, left-aligned 24-bit units. */
int mlp_pcm_open(mlp_pcm *pcm, const char *path, const mlp_format *raw);
/* Before the first read, extend the final AU with zero samples.
 * Aligned input is unchanged; frames/remaining include the explicit tail. */
int mlp_pcm_pad_final(mlp_pcm *pcm);
int mlp_pcm_read(mlp_pcm *pcm, int32_t *samples, size_t capacity, size_t *frames);
void mlp_pcm_close(mlp_pcm *pcm);
#endif
