/* The normal initializer stays at twelve arguments; the host owns this entry. */
#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/native-stack-v2/native_stack_v2.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>

static IdApi *api;
static const NativeStackV1 *stack;
static const NativeStackFactoryV2 *factory;
static Oop retained;
static unsigned calls,reset,registered;
static uintptr_t scan_words[3];
static size_t scan_calls;
static int scan_emit(void *context,uintptr_t value) {
    assert(context==scan_words);
    const uintptr_t expected[]={0,17,99};assert(scan_calls<3 && value==expected[scan_calls]);
    if(scan_calls==0)scan_words[2]=99;
    scan_calls++;return 0;
}
static void roots(MarkFn mark) {
    mark(retained);calls++;
    if(reset) { native_stack_set_v1(stack,0);assert(api->gc_collect()==0); }
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args[0]);retained=args[0];
    size_t anchor=factory->get(stack);assert(anchor);
    scan_calls=0;scan_words[0]=0;scan_words[1]=17;scan_words[2]=7;
    TraceLiveSinkV1 sink={scan_words,scan_emit};
    factory->scan(&sink,0,0);assert(!scan_calls);
    factory->scan(&sink,(uintptr_t)scan_words,(uintptr_t)scan_words+sizeof(scan_words));assert(scan_calls==3);
    volatile Oop native_only=api->alloc(api->object_vt,0);
    calls=0;native_stack_set_v1(stack,0);assert(factory->get(stack)==0 && api->gc_collect()==0 && !calls);
    native_stack_set_v1(stack,SIZE_MAX);assert(factory->get(stack)==SIZE_MAX && !calls);
    native_stack_set_v1(stack,anchor);assert(!calls);
    reset=1;api->gc_collect();reset=0;
    assert(calls==1 && factory->get(stack)==0 && api->vtable_of(retained));
    assert(api->gc_collect()==0 && calls==1);
    native_stack_set_v1(stack,anchor);api->gc_collect();assert(calls==2 && api->vtable_of(retained));
    assert(api->vtable_of(native_only)==api->object_vt);
    Oop result=retained;retained=NULL;return result;
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizers) {
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;(void)settings;(void)layouts;(void)hooks;(void)finalizers;
    if(api)assert(api==selected);api=selected;
    Dl_info owner;assert(dladdr((void *)api->dlsym_fn,&owner));
    void *floor=dlopen(owner.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(floor);
    NativeStackProviderV1 provide=(NativeStackProviderV1)dlsym(floor,"birnam_native_stack_provider_v1");assert(provide);stack=provide();assert(stack);
    assert(dladdr((void *)stack->set,&owner) && strstr(owner.dli_fname,"libbirnam_native_stack.dylib"));
    void *image=dlopen(owner.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(image);
    NativeStackFactoryProviderV2 operations=(NativeStackFactoryProviderV2)dlsym(image,"birnam_native_stack_factory_v2");assert(operations);factory=operations();assert(factory && factory->version==2 && factory->byte_size==sizeof(*factory) && factory->scan);
    assert(dladdr((void *)factory->scan,&owner) && strstr(owner.dli_fname,"libbirnam_native_stack.dylib"));
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("nativeStackExercise:"),exercise);return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
