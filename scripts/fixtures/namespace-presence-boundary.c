/* Compile against each actual ABI: never cast an old initializer/table. */
#ifdef FIXTURE_IDAPI_V8
#include "idapi_v8.h"
#else
#include "idapi_v12.h"
#endif
#include "namespace-support.h"
#include "trace-support.h"
#include "../../abi/fixtures/storage-v1/storage_v1.h"
#include <pthread.h>
static IdApi *api;
static unsigned root_calls;
#ifndef FIXTURE_IDAPI_V8
static NamespacePresenceV1 *presence;
#endif
static int check(const char *name) {
#ifdef FIXTURE_IDAPI_V8
    return api->is_exported(name);
#else
    return namespace_presence_contains_v1(presence,name);
#endif
}
static void *worker(void *name) { (void)check(name);return NULL; }
static void roots(MarkFn mark) { (void)mark;root_calls++;assert(check("Object")); }
static int inspect(void *context,const StorageViewV1 *view) { (void)view;return check(context); }
int main(int argc,char **argv) {
    assert(argc==3);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
#ifndef FIXTURE_IDAPI_V8
    presence=fixture_namespace_presence();
#endif
    if(!strcmp(argv[2],"null"))return check(NULL);
    if(!strcmp(argv[2],"owner") || !strcmp(argv[2],"null-owner")) {
        pthread_t thread;void *name=!strcmp(argv[2],"owner")?(void *)"Object":NULL;
        assert(!pthread_create(&thread,NULL,worker,name));assert(!pthread_join(thread,NULL));return 0;
    }
    if(!strcmp(argv[2],"observation") || !strcmp(argv[2],"null-observation")) {
        StorageObserveV1 observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
        StorageRequestV1 request={1,sizeof(request),!strcmp(argv[2],"observation")?(void *)"Object":NULL,inspect};
        (void)observe(&request);assert(!"observation did not invoke the presence visitor");
    }
    assert(!strcmp(argv[2],"collection"));
    fixture_mount_trace(api,argv[1]);
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    api->gc_register_root_provider(roots);(void)api->gc_collect();assert(root_calls);puts("presence remains callable during root collection");return 0;
}
