/* Test-only introspection built with the same header/configuration as libtcc.
   No copied Rust layout or new language/native-module contract is involved. */
#include "tcc.h"

#if defined(__GNUC__)
__attribute__((visibility("hidden")))
#endif
size_t birnam_tcc_relocated_allocation_bytes(const TCCState *state)
{
#ifdef TCC_IS_NATIVE
    return state && state->run_ptr ? state->run_size : 0;
#else
    return 0;
#endif
}
