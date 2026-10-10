#include "author_library.h"
#include <assert.h>
#include <string.h>

static unsigned callbacks;
static void diagnostic(void *context, int status, const char *message)
{
    const dvda_author_request *request = context;
    assert(status == DVDA_AUTHOR_UNAVAILABLE);
    assert(message && strlen(message));
    assert(dvda_author_run(request) == DVDA_AUTHOR_BUSY);
    ++callbacks;
}

int main(void)
{
    const char *args[] = {"dvda-author", "--help"};
    dvda_author_request request = {sizeof(request), DVDA_AUTHOR_ABI_VERSION,
                                  2, args, diagnostic, NULL};
    request.context = &request;
    assert(dvda_author_abi_version() == DVDA_AUTHOR_ABI_VERSION);
    assert(!dvda_author_is_available());
    assert(dvda_author_run(NULL) == DVDA_AUTHOR_INVALID_ARGUMENT);
    request.abi_version = 0;
    assert(dvda_author_run(&request) == DVDA_AUTHOR_INVALID_ARGUMENT);
    request.abi_version = DVDA_AUTHOR_ABI_VERSION;
    request.argc = 0;
    assert(dvda_author_run(&request) == DVDA_AUTHOR_INVALID_ARGUMENT);
    request.argc = 2;
    for (unsigned i = 0; i < 1000; ++i)
        assert(dvda_author_run(&request) == DVDA_AUTHOR_UNAVAILABLE);
    assert(callbacks == 1000);
    request.diagnostic = NULL;
    assert(dvda_author_run(&request) == DVDA_AUTHOR_UNAVAILABLE);
    return 0;
}
