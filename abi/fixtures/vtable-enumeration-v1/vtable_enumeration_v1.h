#ifndef BIRNAM_VTABLE_ENUMERATION_V1_H
#define BIRNAM_VTABLE_ENUMERATION_V1_H
#include <stddef.h>
#include <stdint.h>
typedef void *Oop;
typedef struct { const Oop *entries; size_t count; } VtableViewV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    VtableViewV1 (*view)(void *);
} VtableFloorV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    size_t (*count)(void *);
    Oop (*at)(void *, size_t);
} VtableEnumerationV1;
typedef struct {
    uint32_t version; size_t byte_size;
    VtableEnumerationV1 *(*create)(const VtableFloorV1 *);
    void (*destroy)(VtableEnumerationV1 *);
} VtableEnumerationFactoryV1;
typedef const VtableEnumerationFactoryV1 *(*VtableEnumerationFactoryProviderV1)(void);
typedef int (*VtableEnumerationDependencyV1)(const VtableEnumerationFactoryV1 *);
typedef VtableEnumerationV1 *(*VtableEnumerationProviderV1)(void);
_Static_assert(sizeof(VtableViewV1)==16,"view extent");
_Static_assert(sizeof(VtableFloorV1)==32,"floor extent");
_Static_assert(sizeof(VtableEnumerationV1)==40,"enumeration extent");
_Static_assert(offsetof(VtableEnumerationV1,count)==24,"count offset");
_Static_assert(offsetof(VtableEnumerationV1,at)==32,"at offset");
_Static_assert(sizeof(VtableEnumerationFactoryV1)==32,"factory extent");
static inline size_t vtable_count_v1(const VtableEnumerationV1 *api) {
    VtableEnumerationV1 selected=*api;return selected.count(selected.context);
}
static inline Oop vtable_at_v1(const VtableEnumerationV1 *api,size_t index) {
    VtableEnumerationV1 selected=*api;return selected.at(selected.context,index);
}
#endif
