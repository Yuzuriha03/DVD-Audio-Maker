#ifndef DVDA_AUTHOR_LIBRARY_H
#define DVDA_AUTHOR_LIBRARY_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define DVDA_AUTHOR_ABI_VERSION 1u
#define DVDA_AUTHOR_OK 0
#define DVDA_AUTHOR_INVALID_ARGUMENT 1
#define DVDA_AUTHOR_UNAVAILABLE 2
#define DVDA_AUTHOR_BUSY 3

typedef void (*dvda_author_diagnostic)(void *context, int status, const char *message);

typedef struct dvda_author_request {
    size_t struct_size;
    uint32_t abi_version;
    int argc;
    const char *const *argv;
    dvda_author_diagnostic diagnostic;
    void *context;
} dvda_author_request;

/* Synchronous, borrowed arguments. Callback strings live only during callback.
 * Concurrent/reentrant calls return BUSY; callbacks must return normally.
 * UNAVAILABLE means no legacy code ran and no output was created.
 */
int dvda_author_run(const dvda_author_request *request);
uint32_t dvda_author_abi_version(void);
int dvda_author_is_available(void);

#ifdef __cplusplus
}
#endif
#endif
