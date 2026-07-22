#include <stddef.h>

#if !defined(__aarch64__)
#error "Birnam stack-root probe requires arm64"
#endif

typedef void *(*birnam_alloc_fn)(void *, size_t);
typedef size_t (*birnam_collect_fn)(void);

__attribute__((noinline))
void birnam_hold_new_object_and_collect(void *vtable,
                                        birnam_alloc_fn allocate,
                                        birnam_collect_fn collect) {
  register void *kept __asm("x19") = allocate(vtable, 0);
  __asm__ volatile("" : "+r"(kept) : : "memory");
  (void)collect();
  __asm__ volatile("" : "+r"(kept) : : "memory");
}
