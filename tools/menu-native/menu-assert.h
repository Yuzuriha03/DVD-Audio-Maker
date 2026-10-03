/* A failed legacy invariant must return through the DLL session boundary. */
#include <assert.h>
#include "session.h"
#undef assert
#define assert(condition) ((condition) ? (void)0 : menu_abort())
