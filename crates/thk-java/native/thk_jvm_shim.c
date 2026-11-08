#include <malloc.h>

#ifndef M_BIONIC_SET_HEAP_TAGGING_LEVEL
#define M_BIONIC_SET_HEAP_TAGGING_LEVEL (-204)
#endif

#ifndef M_HEAP_TAGGING_LEVEL_NONE
#define M_HEAP_TAGGING_LEVEL_NONE 0
#endif

__attribute__((constructor(101)))
static void thk_init_bionic_jvm_compat(void) {
    mallopt(M_BIONIC_SET_HEAP_TAGGING_LEVEL, M_HEAP_TAGGING_LEVEL_NONE);
}
