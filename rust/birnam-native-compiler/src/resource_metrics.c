/* Mapping introspection built with the same header/configuration as libtcc.
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

/* The native owner creates fresh states with default library paths. Deferring
   their automatic runtime setup is private to this owner; ordinary libtcc
   clients keep their existing eager setup and path-order semantics. */
#if defined(__GNUC__)
__attribute__((visibility("hidden")))
#endif
int birnam_tcc_set_memory_output(TCCState *state)
{
#if defined TCC_IS_NATIVE && defined TCC_TARGET_MACHO
    state->host_runtime = 1;
#endif
    return tcc_set_output_type(state, TCC_OUTPUT_MEMORY);
}

#if defined(__GNUC__)
__attribute__((visibility("hidden")))
#endif
int birnam_tcc_uses_host_runtime(const TCCState *state)
{
    return state->host_runtime;
}
