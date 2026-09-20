#ifndef FIXTURE_PAYLOAD_LAYOUT_SUPPORT_H
#define FIXTURE_PAYLOAD_LAYOUT_SUPPORT_H
#include "../../abi/fixtures/payload-layout-v1/payload_layout_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_payload_layout_floor;
static inline void fixture_prepare_payload_layout(void *floor,const char *floor_path) {
    PayloadLayoutDependencyV1 attach=(PayloadLayoutDependencyV1)dlsym(floor,"birnam_payload_layout_dependency_v1"); assert(attach);
    const char *slash=strrchr(floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_payload_layout.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    PayloadLayoutFactoryProviderV1 provide=(PayloadLayoutFactoryProviderV1)dlsym(image,"birnam_payload_layout_factory_v1"); assert(provide);
    const PayloadLayoutFactoryV1 *factory=provide(); assert(factory && attach(factory)==1);
    fixture_payload_layout_floor=floor;
}
static inline PayloadLayoutV1 *fixture_payload_layout(void) {
    assert(fixture_payload_layout_floor);
    PayloadLayoutProviderV1 provide=(PayloadLayoutProviderV1)dlsym(fixture_payload_layout_floor,"birnam_payload_layout_provider_v1"); assert(provide);
    PayloadLayoutV1 *api=provide(); assert(api && api->version==1 && api->byte_size==sizeof(*api)); return api;
}
static inline void fixture_set_layout(void *vt,const uint8_t *kinds,size_t count,uint8_t rest) {
    payload_layout_set_v1(fixture_payload_layout(),vt,kinds,count,rest);
}
static inline uint8_t fixture_slot_kind(void *vt,size_t word) {
    return payload_layout_slot_v1(fixture_payload_layout(),vt,word);
}
/* Test-only adapters preserve the independent transcript's original callback. */
typedef struct {
    void (*set)(void *,const uint8_t *,size_t,uint8_t);
    uint8_t (*slot)(void *,size_t);
} FixtureLayoutSlots;
static inline void fixture_layout_set_slot(void *context,void *vt,const uint8_t *kinds,size_t count,uint8_t rest) {
    ((FixtureLayoutSlots *)context)->set(vt,kinds,count,rest);
}
static inline uint8_t fixture_layout_kind_slot(void *context,void *vt,size_t word) {
    return ((FixtureLayoutSlots *)context)->slot(vt,word);
}
#define FIXTURE_LAYOUTS(slots) {1,sizeof(PayloadLayoutV1),&(slots),fixture_layout_set_slot,fixture_layout_kind_slot}
#endif
