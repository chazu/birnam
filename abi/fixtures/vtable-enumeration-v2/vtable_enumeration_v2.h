#ifndef BIRNAM_VTABLE_ENUMERATION_V2_H
#define BIRNAM_VTABLE_ENUMERATION_V2_H
#include <stddef.h>
#include <stdint.h>
typedef void *Oop;
typedef struct { const Oop *entries; size_t count; } VtableViewV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    VtableViewV1 (*view)(void *);
    VtableViewV1 (*selectors)(void *, Oop);
} VtableFloorV2;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    size_t (*count)(void *);
    Oop (*at)(void *, size_t);
    Oop (*selector_at)(void *, Oop, size_t);
} VtableEnumerationV2;
typedef struct {
    uint32_t version; size_t byte_size;
    VtableEnumerationV2 *(*create)(const VtableFloorV2 *);
    void (*destroy)(VtableEnumerationV2 *);
} VtableEnumerationFactoryV2;
typedef const VtableEnumerationFactoryV2 *(*VtableEnumerationFactoryProviderV2)(void);
typedef int (*VtableEnumerationDependencyV2)(const VtableEnumerationFactoryV2 *);
typedef VtableEnumerationV2 *(*VtableEnumerationProviderV2)(void);
_Static_assert(sizeof(VtableViewV1)==16,"view extent");
_Static_assert(sizeof(VtableFloorV2)==40,"floor extent");
_Static_assert(sizeof(VtableEnumerationV2)==48,"enumeration extent");
_Static_assert(offsetof(VtableEnumerationV2,count)==24,"count offset");
_Static_assert(offsetof(VtableEnumerationV2,at)==32,"at offset");
_Static_assert(sizeof(VtableEnumerationFactoryV2)==32,"factory extent");
static inline size_t vtable_count_v2(const VtableEnumerationV2 *api) {
    VtableEnumerationV2 selected=*api;return selected.count(selected.context);
}
static inline Oop vtable_at_v2(const VtableEnumerationV2 *api,size_t index) {
    VtableEnumerationV2 selected=*api;return selected.at(selected.context,index);
}
static inline Oop vtable_selector_at_v2(const VtableEnumerationV2 *api,Oop vtable,size_t index) {
    VtableEnumerationV2 selected=*api;return selected.selector_at(selected.context,vtable,index);
}
_Static_assert(offsetof(VtableEnumerationV2,selector_at)==40,"v2 selector offset");
#endif
