/* Diagnostic requested heap storage for this linked compiler instance.
   Do not include tcc.h: its allocator macros belong inside the compiler. */
#include <stddef.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include "libtcc.h"

typedef union {
    max_align_t alignment;
    size_t requested_size;
} AllocationHeader;

static _Atomic size_t live_bytes;
static _Atomic size_t live_blocks;

static void allocation_failure(void)
{
    /* Match the default libtcc allocator's allocation-failure contract. */
    fprintf(stderr, "tcc: memory full\n");
    exit(1);
}

static void *accounting_reallocator(void *pointer, unsigned long size)
{
    AllocationHeader *old = pointer ? (AllocationHeader *)pointer - 1 : NULL;
    int is_new = pointer == NULL;
    size_t previous = old ? old->requested_size : 0;
    AllocationHeader *next;
    if (!size) {
        if (old) {
            atomic_fetch_sub_explicit(&live_bytes, previous, memory_order_relaxed);
            atomic_fetch_sub_explicit(&live_blocks, 1, memory_order_relaxed);
            free(old);
        }
        return NULL;
    }
    if (size > SIZE_MAX - sizeof(AllocationHeader))
        allocation_failure();
    next = realloc(old, sizeof(AllocationHeader) + size);
    if (!next)
        allocation_failure();
    next->requested_size = size;
    if (size >= previous)
        atomic_fetch_add_explicit(&live_bytes, size - previous, memory_order_relaxed);
    else
        atomic_fetch_sub_explicit(&live_bytes, previous - size, memory_order_relaxed);
    if (is_new)
        atomic_fetch_add_explicit(&live_blocks, 1, memory_order_relaxed);
    return next + 1;
}

#if defined(__GNUC__)
__attribute__((visibility("hidden")))
#endif
void birnam_tcc_install_allocation_metrics(void)
{
    tcc_set_realloc(accounting_reallocator);
}

#if defined(__GNUC__)
__attribute__((visibility("hidden")))
#endif
void birnam_tcc_heap_allocation_totals(size_t *blocks, size_t *bytes)
{
    *blocks = atomic_load_explicit(&live_blocks, memory_order_relaxed);
    *bytes = atomic_load_explicit(&live_bytes, memory_order_relaxed);
}
