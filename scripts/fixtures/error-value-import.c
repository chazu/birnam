#include "idapi_v9.h"
#include "namespace-support.h"
#include "integer-support.h"
int main(int argc,char **argv) {
 assert(argc==5);void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);fixture_prepare_namespace(floor,argv[1]);
 IdApi *api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);fixture_mount_integer(api,argv[1],fixture_namespace_presence());
 for(int i=2;i<5;i++) { void *image=api->dlopen_fn(argv[i],RTLD_NOW|RTLD_LOCAL);assert(image);fixture_attach_integer(api,image);ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);init(api,NULL,fixture_namespace_presence()); }
 Oop system=api->import_fn("System");assert(system);Oop selector=api->intern("errorValueImportProbe");
 assert(api->send(system,selector,NULL,0)==api->true_obj);assert(api->send(system,selector,NULL,0)==api->true_obj);
 puts("explicit import oracle: two complete independent runs passed");
}
