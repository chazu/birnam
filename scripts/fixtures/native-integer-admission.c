#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static IdApi *api;
static BirnamProtocol id;
static IntegerApiV1 *integer;
static Oop targets[8], selectors[8], add, lookup;
static BirnamNativeEntry originals[8];
static unsigned mode;
static size_t effects;
static BirnamOperand object(Oop value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOperand send(Oop receiver,Oop selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED);return result.value;
}
static void install(unsigned index,BirnamNativeEntry entry) {
    BirnamOperand args[]={object(selectors[index]),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry=entry}};
    send(targets[index],add,args,2);
}
static BirnamOutcome observe(void *context,Oop receiver,const BirnamOperand *args,size_t count) {
    BirnamNativeEntry original=*(BirnamNativeEntry *)context;
    ++effects;
    return original.method(original.context,receiver,args,count);
}
static Oop select_mode(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1);
    unsigned next=(unsigned)integer->read(integer->context,args[0]);assert(next<=8);
    if(mode) install(mode-1,originals[mode-1]);
    mode=next;
    if(mode) install(mode-1,(BirnamNativeEntry){observe,&originals[mode-1]});
    effects=0;fprintf(stderr,"native-admission-mode %u\n",mode);return api->nil_obj;
}
static Oop report(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(count==0 && mode && effects);
    printf("native-admission-effects %u %zu\n",mode,effects);
    return api->nil_obj;
}
static void roots(MarkFn mark) {
    mark(add);mark(lookup);
    for(unsigned i=0;i<8;i++) { mark(targets[i]);mark(selectors[i]); }
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,
    const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,
    const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,
    const RootRegistryV1 *registry,const GcSettingsV1 *settings,
    const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizers) {
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;
    (void)layouts;(void)hooks;(void)finalizers;
    assert(!api);api=selected;
    Dl_info owner;assert(dladdr((void *)api->dlsym_fn,&owner));
    void *floor=dlopen(owner.dli_fname,RTLD_NOW|RTLD_NOLOAD);assert(floor);
    const BirnamModule *module=dlsym(floor,"birnam_native_module_v1");assert(module);
    BirnamOutcome loaded=module->initialize(NULL,NULL,0);
    assert(loaded.status==BIRNAM_LOADED && loaded.value.kind==BIRNAM_PROTOCOL);
    id=*loaded.value.data.protocol;
    const char *slash=strrchr(owner.dli_fname,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_integer.dylib",(int)(slash-owner.dli_fname),owner.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_NOLOAD);assert(image);
    IntegerProviderV1 provide=(IntegerProviderV1)dlsym(image,"birnam_integer_provider_v1");assert(provide);
    integer=provide();assert(integer);
    add=api->intern("addMethod");lookup=api->intern("lookup");
    Oop namespace=namespace_import_v1(imports,"Namespace");assert(namespace);
    const char *names[]={"allocate","bindingAt:","intern:","symbolBytesOf:","lookup","ownerOf:","parentOf:","+"};
    for(unsigned i=0;i<8;i++) {
        targets[i]=i==1?api->vtable_of(namespace):i==7?integer->behavior:id.receiver;
        selectors[i]=api->intern(names[i]);
        BirnamOperand arg=object(selectors[i]);BirnamOperand entry=send(targets[i],lookup,&arg,1);
        assert(entry.kind==BIRNAM_NATIVE_ENTRY);originals[i]=entry.data.entry;
    }
    root_registry_register_v1(registry,roots);
    gc_settings_auto_v1(settings,0);
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("nativeAdmissionMode:"),select_mode);
    api->method(api->vtable_of(system),api->intern("nativeAdmissionReport"),report);
    return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
