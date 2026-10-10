#include <stddef.h>
#include <stdio.h>
#include <MagickCore/MagickCore.h>
int main(void) {
 printf("pub const EXCEPTION_SEVERITY:usize=%zu;\npub const EXCEPTION_REASON:usize=%zu;\npub const EXCEPTION_DESCRIPTION:usize=%zu;\n",offsetof(ExceptionInfo,severity),offsetof(ExceptionInfo,reason),offsetof(ExceptionInfo,description));
 return 0;
}
