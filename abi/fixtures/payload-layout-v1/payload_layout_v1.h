#ifndef BIRNAM_PAYLOAD_LAYOUT_V1_H
#define BIRNAM_PAYLOAD_LAYOUT_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct { const uint8_t *kinds; size_t count; uint8_t rest,is_set; } PayloadLayoutViewV1;
typedef struct { PayloadLayoutViewV1 view; } PayloadLayoutOwnedV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*admit)(void *);
    void (*replace)(void *,void *,PayloadLayoutOwnedV1);
    PayloadLayoutViewV1 (*view)(void *,void *);
} PayloadLayoutFloorV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*set)(void *,void *,const uint8_t *,size_t,uint8_t);
    uint8_t (*slot_kind)(void *,void *,size_t);
} PayloadLayoutV1;
typedef struct {
    uint32_t version; size_t byte_size;
    PayloadLayoutV1 *(*create)(const PayloadLayoutFloorV1 *);
    void (*destroy)(PayloadLayoutV1 *);
    PayloadLayoutOwnedV1 (*copy)(const uint8_t *,size_t,uint8_t);
    void (*release)(PayloadLayoutOwnedV1);
    uint8_t (*slot_kind)(PayloadLayoutViewV1,size_t);
    int (*has_layout)(PayloadLayoutViewV1);
    int (*all_oop)(PayloadLayoutViewV1,size_t);
} PayloadLayoutFactoryV1;
typedef const PayloadLayoutFactoryV1 *(*PayloadLayoutFactoryProviderV1)(void);
typedef int (*PayloadLayoutDependencyV1)(const PayloadLayoutFactoryV1 *);
typedef PayloadLayoutV1 *(*PayloadLayoutProviderV1)(void);
_Static_assert(sizeof(PayloadLayoutViewV1)==24 && offsetof(PayloadLayoutViewV1,rest)==16,"layout view");
_Static_assert(sizeof(PayloadLayoutOwnedV1)==24,"layout owner");
_Static_assert(sizeof(PayloadLayoutFloorV1)==48 && offsetof(PayloadLayoutFloorV1,view)==40,"layout floor");
_Static_assert(sizeof(PayloadLayoutV1)==40 && offsetof(PayloadLayoutV1,slot_kind)==32,"layout API");
_Static_assert(sizeof(PayloadLayoutFactoryV1)==72 && offsetof(PayloadLayoutFactoryV1,all_oop)==64,"layout factory");
static inline void payload_layout_set_v1(const PayloadLayoutV1 *selected,void *vt,const uint8_t *kinds,size_t count,uint8_t rest) {
    PayloadLayoutV1 api=*selected; api.set(api.context,vt,kinds,count,rest);
}
static inline uint8_t payload_layout_slot_v1(const PayloadLayoutV1 *selected,void *vt,size_t word) {
    PayloadLayoutV1 api=*selected; return api.slot_kind(api.context,vt,word);
}
#endif
