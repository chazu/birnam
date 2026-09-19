/* Check every real consumer before dependency attachment or initialization. */
#include "idapi_v8.h"
#include "namespace-support.h"
#include "../../abi/fixtures/error-value-v2/error_value_v2.h"
int main(int argc,char **argv) {
    assert(argc==12);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);
    IdInitFn initialize=(IdInitFn)dlsym(floor,"id_init");assert(initialize);
    IdApi *api=initialize();assert(api);
    void *provider=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(provider);
    ErrorValueProviderV2 provide=(ErrorValueProviderV2)dlsym(provider,"birnam_error_value_provider_v2");
    ModuleInitFn init=(ModuleInitFn)dlsym(provider,"module_init");assert(provide && init && !provide());
    init(api,NULL);const ErrorValueApiV2 *operations=provide();assert(operations && operations==provide());
    assert(operations->version==2 && operations->byte_size==sizeof(*operations));
    assert(!dlsym(provider,"birnam_error_value_provider_v1"));
    for(int i=3;i<argc;i++) {
        void *image=dlopen(argv[i],RTLD_NOW|RTLD_LOCAL);assert(image);
        ErrorValueDependencyV2 attach=(ErrorValueDependencyV2)dlsym(image,"birnam_error_value_dependency_v2");
        assert(attach && !dlsym(image,"birnam_error_value_dependency_v1"));
        ErrorValueApiV2 bad=*operations;assert(!attach(NULL));
        bad.version=1;assert(!attach(&bad));bad=*operations;
        bad.byte_size--;assert(!attach(&bad));bad=*operations;
        bad.is_error=NULL;assert(!attach(&bad));bad=*operations;
        bad.from_bytes=NULL;assert(!attach(&bad));
        assert(attach(operations)==1 && attach(operations)==1);
        bad=*operations;assert(!attach(&bad));
    }
    puts("error presence admission: all nine consumers require v2, reject malformed descriptors and retargeting, accept repeat attachment");
}
