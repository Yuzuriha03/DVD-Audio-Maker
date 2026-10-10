#include "include/author_library.h"
#include <stdatomic.h>

static atomic_flag active = ATOMIC_FLAG_INIT;

uint32_t dvda_author_abi_version(void)
{
    return DVDA_AUTHOR_ABI_VERSION;
}

int dvda_author_is_available(void)
{
    return 0;
}

int dvda_author_run(const dvda_author_request *request)
{
    int i;
    if (!request || request->struct_size != sizeof(*request)
        || request->abi_version != DVDA_AUTHOR_ABI_VERSION
        || request->argc < 1 || !request->argv)
        return DVDA_AUTHOR_INVALID_ARGUMENT;
    for (i = 0; i < request->argc; ++i)
        if (!request->argv[i]) return DVDA_AUTHOR_INVALID_ARGUMENT;

    if (atomic_flag_test_and_set_explicit(&active, memory_order_acquire))
        return DVDA_AUTHOR_BUSY;

    /* Legacy exit paths cannot unwind resources, and parser statics retain
     * request-owned state. Do not enter main or intercept exit with longjmp.
     */
    if (request->diagnostic)
        request->diagnostic(request->context, DVDA_AUTHOR_UNAVAILABLE,
            "Embedded author unavailable: legacy fatal unwinding and request ownership are not audited safe");

    atomic_flag_clear_explicit(&active, memory_order_release);
    return DVDA_AUTHOR_UNAVAILABLE;
}
