#ifndef MLP_FORMAT_H
#define MLP_FORMAT_H
#include "mlp_bits.h"
typedef struct mlp_format {
    unsigned sample_rate, bits, channels, assignment, au_samples, rate_field;
    unsigned rate_code, channel_mask;
    unsigned group1_channels, group2_channels;
    unsigned group2_bits;
    unsigned group2_sample_rate,group2_rate_code;
    unsigned input_order[6]; /* coded channel -> ascending WAVE-mask position */
} mlp_format;
/* DVD-Audio format admission: 16/20/24 bits; 44.1/48/88.2/96 kHz with
 * 1..6 channels; 176.4/192 kHz with 1..2 channels. Uniform sample rate/depth,
 * standard mono through 5.1 layouts. This is not SurCode GUI validation.
 */
MLP_BITS_API int mlp_format_init(mlp_format *format, unsigned rate,
    unsigned bits, unsigned channels);
/* Explicit DVD-Audio assignment 0..20. PCM input remains WAVE-mask order;
 * assignments 18..20 are reordered internally into group order. */
MLP_BITS_API int mlp_format_init_assignment(mlp_format *format, unsigned rate,
    unsigned bits, unsigned assignment);
MLP_BITS_API int mlp_format_set_group2_bits(mlp_format *format,unsigned bits);
MLP_BITS_API int mlp_format_set_group2_rate(mlp_format *format,unsigned rate);
MLP_BITS_API void mlp_format_major(const mlp_format *format, uint16_t words[14]);
MLP_BITS_API uint16_t mlp_major_checksum(const uint16_t words[13]);
MLP_BITS_API unsigned mlp_au_parity(unsigned length, unsigned arrival,
    const unsigned *directories, unsigned count);
#endif
