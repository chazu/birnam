/* Check every real consumer before dependency attachment or initialization. */
#include "idapi_v11.h"
#include "namespace-support.h"
#include "../../abi/fixtures/error-value-v3/error_value_v3.h"
int main(int argc,char **argv) {
    assert(argc==12);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);
    IdInitFn initialize=(IdInitFn)dlsym(floor,"id_init");assert(initialize);
    IdApi *api=initialize();assert(api);
    void *provider=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(provider);
    ErrorValueProviderV3 provide=(ErrorValueProviderV3)dlsym(provider,"birnam_error_value_provider_v3");
    ModuleInitFn init=(ModuleInitFn)dlsym(provider,"module_init");assert(provide && init && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());const ErrorValueApiV3 *operations=provide();assert(operations && operations==provide());
    assert(operations->version==3 && operations->byte_size==sizeof(*operations));
    assert(!dlsym(provider,"birnam_error_value_provider_v2"));
    for(int i=3;i<argc;i++) {
        void *image=dlopen(argv[i],RTLD_NOW|RTLD_LOCAL);assert(image);
        ErrorValueDependencyV3 attach=(ErrorValueDependencyV3)dlsym(image,"birnam_error_value_dependency_v3");
        assert(attach && !dlsym(image,"birnam_error_value_dependency_v2"));
        ErrorValueApiV3 bad=*operations;assert(!attach(NULL));
        bad.version=2;assert(!attach(&bad));bad=*operations;
        bad.byte_size--;assert(!attach(&bad));bad=*operations;
        bad.is_error=NULL;assert(!attach(&bad));bad=*operations;
        bad.from_bytes=NULL;assert(!attach(&bad));
        assert(attach(operations)==1 && attach(operations)==1);
        bad=*operations;assert(!attach(&bad));
    }
    puts("error presence admission: all nine consumers require v3, reject malformed descriptors and retargeting, accept repeat attachment");
}
