#ifndef MLP_BITS_H
#define MLP_BITS_H
#include <stddef.h>
#include <stdint.h>
#if defined(_WIN32) && defined(MLP_BITS_BUILD)
#define MLP_BITS_API __declspec(dllexport)
#elif defined(_WIN32) && defined(MLP_BITS_USE_DLL)
#define MLP_BITS_API __declspec(dllimport)
#else
#define MLP_BITS_API
#endif
/* New C API, NOT the original MSVC6 encoder ABI. */
typedef struct mlp_bits {
    uint32_t *words;
    size_t capacity;
    size_t count;
    uint16_t pending;
    unsigned pending_bits;
    int finished;
} mlp_bits;
enum { MLP_BITS_OK = 0, MLP_BITS_INVALID = -1, MLP_BITS_FULL = -2 };
MLP_BITS_API int mlp_bits_init(mlp_bits *ctx, uint32_t *words, size_t capacity);
MLP_BITS_API int mlp_bits_put(mlp_bits *ctx, uint32_t value, unsigned width);
/* Terminal flush. Repeated calls succeed without appending data. */
MLP_BITS_API int mlp_bits_finish(mlp_bits *ctx);
MLP_BITS_API uint32_t mlp_bits_checksum(const uint32_t *words, size_t count);
/* Includes terminal flush, optional D234 D234 markers, and checksum. */
MLP_BITS_API int mlp_bits_seal(mlp_bits *ctx, int end_markers);
MLP_BITS_API int mlp_bits_to_bytes(const mlp_bits *ctx, uint8_t *out,
                                  size_t capacity, size_t *written);
#endif