/* Compile against each actual ABI: never cast an old initializer/table. */
#ifdef FIXTURE_IDAPI_V9
#include "idapi_v9.h"
#else
#include "idapi_v20.h"
#endif
#include "namespace-support.h"
#include "trace-support.h"
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
#include <pthread.h>
static IdApi *api;
static unsigned root_calls;
#ifndef FIXTURE_IDAPI_V9
static NamespaceImportV1 *imports;
#endif
static Oop check(const char *name) {
#ifdef FIXTURE_IDAPI_V9
    return api->import_fn(name);
#else
    return namespace_import_v1(imports,name);
#endif
}
static void *worker(void *name) { (void)check(name);return NULL; }
static void roots(MarkFn mark) { (void)mark;root_calls++;assert(check("Object")); }
static int inspect(void *context,const StorageViewV1 *view) { (void)view;return check(context)!=NULL; }
int main(int argc,char **argv) {
    assert(argc==3);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
#ifndef FIXTURE_IDAPI_V9
    imports=fixture_namespace_imports();
#endif
    if(!strcmp(argv[2],"null"))return check(NULL)!=NULL;
    if(!strcmp(argv[2],"owner") || !strcmp(argv[2],"null-owner")) {
        pthread_t thread;void *name=!strcmp(argv[2],"owner")?(void *)"Object":NULL;
        assert(!pthread_create(&thread,NULL,worker,name));assert(!pthread_join(thread,NULL));return 0;
    }
    if(!strcmp(argv[2],"observation") || !strcmp(argv[2],"null-observation")) {
        StorageObserveV1 observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
        StorageRequestV1 request={1,sizeof(request),!strcmp(argv[2],"observation")?(void *)"Object":NULL,inspect};
        (void)observe(&request);assert(!"observation did not invoke the import visitor");
    }
    assert(!strcmp(argv[2],"collection"));
    fixture_mount_trace(api,argv[1],fixture_root_registry());
    uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_register_root(roots);(void)api->gc_collect();assert(root_calls);puts("import remains callable during root collection");return 0;
}
