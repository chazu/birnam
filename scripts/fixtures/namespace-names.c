#include "idapi_v14.h"
#include "namespace-support.h"
/* Independent admission checks for every declared reverse-name consumer. */
int main(int argc,char **argv) {
    assert(argc==6);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);
    IdInitFn init=(IdInitFn)dlsym(floor,"id_init");assert(init);IdApi *api=init();assert(api);
    NamespaceNamesApiV1 selected[4];
    for(int i=2;i<argc;i++) {
        void *image=dlopen(argv[i],RTLD_NOW|RTLD_LOCAL);assert(image);
        NamespaceNamesDependencyV1 attach=(NamespaceNamesDependencyV1)dlsym(image,"birnam_namespace_names_dependency_v1");assert(attach);
        selected[i-2]=*fixture_namespace_names();NamespaceNamesApiV1 bad=selected[i-2];
        assert(!attach(NULL));bad.version++;assert(!attach(&bad));
        bad=selected[i-2];bad.byte_size--;assert(!attach(&bad));
        bad=selected[i-2];bad.context=NULL;assert(!attach(&bad));
        bad=selected[i-2];bad.name_of=NULL;assert(!attach(&bad));
        assert(attach(&selected[i-2])==1 && attach(&selected[i-2])==1);
        assert(!attach(fixture_namespace_names()));
    }
    puts("namespace names: all four consumers reject malformed capabilities and retargeting, accept independent copies and repeated attachment");
    return 0;
}
