#ifndef BIRNAM_NATIVE_MARK_V1_H
#define BIRNAM_NATIVE_MARK_V1_H
#include <stddef.h>
#include <stdint.h>
typedef void (*NativeMarkFnV1)(void *);
typedef struct NativeMarkSinkV1 {
    void *context;
    int (*emit)(void *, uintptr_t);
} NativeMarkSinkV1;
typedef struct NativeMarkV1 {
    uint32_t version;
    size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    void (*begin)(void *, const NativeMarkSinkV1 *);
    void (*end)(void *);
    void (*mark)(void *, void *);
} NativeMarkV1;
typedef const NativeMarkV1 *(*NativeMarkProviderV1)(void);
typedef int (*NativeMarkDependencyV1)(const NativeMarkV1 *);
_Static_assert(sizeof(NativeMarkV1)==56, "Native Mark v1 size");
_Static_assert(offsetof(NativeMarkV1,mark)==48, "Native Mark v1 mark");
#endif
