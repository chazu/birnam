#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "native-fixture.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static const NamespaceImportV1 *imports;
static const RootRegistryV1 *roots;
static Oop kept;
static unsigned active,calls[4],call_count,registered,later_registered;
static void later(MarkFn mark) {
    mark(kept);
    if(active) { assert(call_count<4);calls[call_count++]=2; }
}
static void first(MarkFn mark) {
    mark(kept);
    if(active) {
        assert(call_count<4);calls[call_count++]=1;
        assert(api->gc_collect()==0);
        if(!later_registered) { later_registered=1;root_registry_register_v1(roots,later); }
    }
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args && args[0] && registered);
    kept=api->alloc(api->object_vt,0);assert(kept);
    unsigned already=later_registered;
    call_count=0;active=1;api->gc_collect();active=0;
    assert(call_count==2+already && calls[0]==1 && calls[1]==1);
    if(already)assert(calls[2]==2);
    assert(api->vtable_of(kept)==api->object_vt);
    call_count=0;active=1;api->gc_collect();active=0;
    assert(call_count==3 && calls[0]==1 && calls[1]==1 && calls[2]==2);
    assert(api->vtable_of(kept)==api->object_vt);kept=NULL;
    return args[0];
}
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    (void)id;
    if(count) {
        assert(count==3);IdApi *selected=fixture_word(args[0]);
        if(api)assert(api==selected);api=selected;imports=fixture_word(args[1]);
        roots=fixture_word(args[2]);
        if(!registered) { root_registry_register_v1(roots,first);root_registry_register_v1(roots,first);registered=1; }
    }
    assert(api && imports && registered);
    Dl_info owner;
    assert(roots->version==1 && roots->byte_size==sizeof(*roots));
    assert(dladdr((void *)roots->register_root,&owner) && strstr(owner.dli_fname,"libbirnam_root_registry.dylib"));
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("rootRegistryExercise:"),exercise);
    return (BirnamOutcome){.status=BIRNAM_LOADED};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
