#ifdef FIXTURE_IDAPI_V11
#include "idapi_v11.h"
#else
#include "idapi_v18.h"
#endif
#include "namespace-support.h"
#include "trace-support.h"
#include "../../abi/fixtures/idapi-v18/storage_v1.h"
#include <pthread.h>
static IdApi *api;
static unsigned root_calls;
static int indexed;
#ifndef FIXTURE_IDAPI_V11
static VtableEnumerationV2 *enumeration;
#endif
static void check(void) {
#ifdef FIXTURE_IDAPI_V11
    if(indexed)assert(api->vtable_at(0)==api->vtable_vt);else assert(api->vtable_count()>0);
#else
    if(indexed)assert(vtable_at_v2(enumeration,0)==api->vtable_vt);else assert(vtable_count_v2(enumeration)>0);
#endif
}
static void *worker(void *context) { (void)context;check();return NULL; }
static void roots(MarkFn mark) { (void)mark;root_calls++;check(); }
static int inspect(void *context,const StorageViewV1 *view) { (void)context;(void)view;check();return 1; }
int main(int argc,char **argv) {
    assert(argc==3);indexed=!strncmp(argv[2],"at-",3);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);fixture_prepare_namespace(floor,argv[1]);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
#ifndef FIXTURE_IDAPI_V11
    enumeration=fixture_vtable_enumeration();
#endif
    if(strstr(argv[2],"owner")) {
        pthread_t thread;assert(!pthread_create(&thread,NULL,worker,NULL));assert(!pthread_join(thread,NULL));return 0;
    }
    if(strstr(argv[2],"observation")) {
        StorageObserveV1 observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
        StorageRequestV1 request={1,sizeof(request),NULL,inspect};(void)observe(&request);
        assert(!"observation did not invoke visitor");
    }
    assert(strstr(argv[2],"collection"));fixture_mount_trace(api,argv[1],fixture_root_registry());
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_register_root(roots);(void)api->gc_collect();assert(root_calls);
    puts("enumeration remains callable during root collection");return 0;
}
