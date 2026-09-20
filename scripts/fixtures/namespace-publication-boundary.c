#include "idapi_v19.h"
#include "namespace-support.h"
#include "trace-support.h"
#include "../../abi/fixtures/idapi-v19/storage_v1.h"
#include <pthread.h>
static IdApi *api;
static unsigned root_calls;
static int publish(const char *name) { return fixture_namespace_export(name,api->nil_obj)==api->nil_obj; }
static void *worker(void *name) { (void)publish(name);return NULL; }
static void roots(MarkFn mark) { (void)mark;root_calls++;assert(publish("namespace-root-publication")); }
static int inspect(void *context,const StorageViewV1 *view) { (void)view;return publish(context); }
int main(int argc,char **argv) {
    assert(argc==3);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
    (void)fixture_namespace_exports();
    if(!strcmp(argv[2],"null"))return publish(NULL);
    if(!strcmp(argv[2],"owner") || !strcmp(argv[2],"null-owner")) {
        pthread_t thread;void *name=!strcmp(argv[2],"owner")?(void *)"Object":NULL;
        assert(!pthread_create(&thread,NULL,worker,name));assert(!pthread_join(thread,NULL));return 0;
    }
    if(!strcmp(argv[2],"observation") || !strcmp(argv[2],"null-observation")) {
        StorageObserveV1 observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
        StorageRequestV1 request={1,sizeof(request),!strcmp(argv[2],"observation")?(void *)"Object":NULL,inspect};
        (void)observe(&request);assert(!"observation did not invoke the publication visitor");
    }
    assert(!strcmp(argv[2],"collection"));assert(publish("namespace-root-publication"));
    fixture_mount_trace(api,argv[1],fixture_root_registry());
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_register_root(roots);(void)api->gc_collect();assert(root_calls);
    assert(fixture_namespace_import("namespace-root-publication")==api->nil_obj);
    puts("publication remains callable during root collection");return 0;
}
