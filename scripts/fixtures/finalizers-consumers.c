/* Independent saved-selection and real native-owner cleanup oracle. */
#define _DARWIN_C_SOURCE
#ifdef FINALIZERS_BASELINE
#include "idapi_v18.h"
#define INIT(fn,table,cap) fn(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks())
#else
#include "idapi_v19.h"
#define INIT(fn,table,cap) fn(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),cap)
#endif
#include "namespace-support.h"
#include "integer-support.h"
#include "collect-support.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include <errno.h>
#include <signal.h>
#include <stdlib.h>
#include <unistd.h>

static IdApi *api,selected,other;
static const char *mode,*library;
static unsigned phase,capture,registered,finished;
static struct { Oop object; Finalizer callback; unsigned done; } owners[64];
static TraceLiveV1 actual_trace;
static TraceLiveSourceV1 original_source;
static void (*isolate_drain)(void);
static pid_t worker_pid;
static Oop failed_behavior;

static Oop send0(Oop object,const char *selector) { return api->send(object,api->intern(selector),NULL,0); }
static Oop send1(Oop object,const char *selector,Oop arg) { return api->send(object,api->intern(selector),&arg,1); }
static Oop send2(Oop object,const char *selector,Oop a,Oop b) { Oop args[]={a,b};return api->send(object,api->intern(selector),args,2); }
static Oop text(const char *value) { return fixture_string_make((const uint8_t *)value,strlen(value)); }
static Oop named(const char *name) { Oop value=fixture_namespace_import(name);assert(value);return value; }
static int is(const char *name) { return !strcmp(mode,name); }
static void apply(Oop object,Finalizer callback) {
#ifdef FINALIZERS_BASELINE
    api->set_finalizer(object,callback);
#else
    fixture_set_finalizer(object,callback);
#endif
}
static int admit(void *context,uintptr_t address) {
    /* These chosen owners are deliberately dead for the independent oracle;
       native stack copies must not accidentally keep them alive. */
    for(unsigned i=0;i<registered;i++) if(address==(uintptr_t)owners[i].object) return 0;
    return original_source.admit(context,address);
}
static int traced(const TraceLiveSourceV1 *source) {
    original_source=*source;TraceLiveSourceV1 watched=*source;watched.admit=admit;return actual_trace(&watched);
}
static void cleanup(Oop object) {
    unsigned index=0;while(index<registered && owners[index].object!=object)index++;
    assert(index<registered && !owners[index].done && !api->vtable_of(object));
    owners[index].done=1;
    if(is("tcc")) assert(*(void **)object && *((uint8_t *)object+8)==4);
    owners[index].callback(object);
    if(is("builder")) { uintptr_t *payload=object;assert(!payload[0] && !payload[1] && !payload[2]); }
    if(is("alien")) assert(!*(uintptr_t *)object);
    if(is("tcc")) assert(!*(void **)object && *((uint8_t *)object+8)==5);
    if(is("isolate")) assert(*(uint64_t *)object && kill(worker_pid,0)==0);
    finished++;
}
static void record(Oop object,Finalizer callback) {
    if(!capture) { apply(object,callback);return; }
    assert(registered<64 && object && callback && api->object_payload_size(object)>0);
    Dl_info owner;assert(dladdr((void *)callback,&owner));
    const char *base=strrchr(library,'/');assert(base && strstr(owner.dli_fname,base+1));
    if(is("builder")) { uintptr_t *payload=object;assert(!payload[0] && !payload[1] && !payload[2]); }
    if(is("alien")) { uintptr_t *payload=object;assert(payload[0] && payload[1]);for(size_t i=0;i<32;i++)assert(!((uint8_t *)payload[0])[i]); }
    if(is("tcc")) assert(*(void **)object && *((uint8_t *)object+8)==4);
    if(is("portable") || is("isolate")) assert(*(uint64_t *)object);
    owners[registered].object=object;owners[registered].callback=callback;owners[registered].done=0;registered++;
    apply(object,cleanup);
}
static void first(Oop object,Finalizer callback) { assert(!phase);record(object,callback); }
static void saved(Oop object,Finalizer callback) { assert(phase);record(object,callback); }
static void forbidden(Oop object,Finalizer callback) { (void)object;(void)callback;assert(!"incoming finalizer selection was used"); }
#ifndef FINALIZERS_BASELINE
static int first_context,saved_context,other_context;
static void first_set(void *context,void *object,FinalizerFnV1 callback) { assert(context==&first_context);first(object,callback); }
static void saved_set(void *context,void *object,FinalizerFnV1 callback) { assert(context==&saved_context);saved(object,callback); }
static void other_set(void *context,void *object,FinalizerFnV1 callback) { assert(context==&other_context);forbidden(object,callback); }
#endif
static void *load_sibling(const char *floor,const char *name) {
    const char *slash=strrchr(floor,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_%s.dylib",(int)(slash-floor),floor,name);assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);
    if(!image) { n=snprintf(path,sizeof(path),"%.*s/libbirnam_id_%s.dylib",(int)(slash-floor),floor,name);assert(n>0 && (size_t)n<sizeof(path));image=dlopen(path,RTLD_NOW|RTLD_LOCAL); }
    assert(image);return image;
}
static void mount(void *image) {
    fixture_attach_integer(api,image,fixture_root_registry());
    ModuleInitFn init=(ModuleInitFn)dlsym(image,"module_init");assert(init);INIT(init,api,fixture_finalizers());
}
static Oop allocation(Oop behavior,size_t size) { return behavior==failed_behavior?NULL:api->alloc(behavior,size); }
static void read_worker_pid(void) {
    FILE *file=fopen(getenv("FINALIZER_WORKER_PID_FILE"),"r");assert(file);int pid=0;assert(fscanf(file,"%d",&pid)==1);fclose(file);worker_pid=pid;assert(pid>0);
}
static void exercise(void) {
    if(is("peg")) {
        Oop pattern=send1(named("Peg"),"literal:",text("ab"));
        Oop grammar=send0(named("Peg"),"grammar"),rule=api->intern("entry");
        assert(send2(grammar,"rule:is:",rule,pattern)==grammar);
        Oop cst=send2(grammar,"parse:from:",text("ab"),rule);
        assert(api->vtable_of(cst)==named("PegCstVt"));assert(send0(cst,"rule")==rule);
        assert(fixture_integer_value(send0(cst,"end"))==2);
        assert(registered==3);
    } else if(is("builder")) {
        Oop builder=send0(named("StringBuilder"),"new");assert(registered==1);
        assert(send1(builder,"add:",text("owned bytes"))==builder);
        assert(fixture_integer_value(send0(builder,"size"))==11);
    } else if(is("tcc")) {
        Oop unit=send0(named("Tcc"),"new");assert(!registered && unit!=api->nil_obj);
        assert(send0(unit,"disposable")==unit && registered==1);
        assert(send0(unit,"disposable")==api->nil_obj && registered==1);
        Oop empty=api->alloc(named("TccUnitVt"),24);assert(send0(empty,"disposable")==api->nil_obj && registered==1);
    } else if(is("portable")) {
        Oop node=send1(named("CodeNode"),"string:",text("owned literal"));
        Oop statement=send1(named("CodeStatement"),"expression:",node);
        Oop empty=send0(named("Vector"),"new"),statements=send1(empty,"add:",statement);
        Oop args[]={empty,empty,statements};
        Oop program=api->send(named("CodeProgram"),api->intern("parameters:captures:statements:"),args,3);
        assert(api->vtable_of(program)==named("CodeProgramVt") && registered==3);
        size_t size=0;const uint8_t *bytes=fixture_string_bytes(send0(node,"string"),&size);assert(size==13 && !memcmp(bytes,"owned literal",13));
    } else if(is("alien")) {
        Oop address=send1(named("Alien"),"alloc:",fixture_integer_make(32));assert(registered==1);
        memset((void *)*(uintptr_t *)address,0xa5,32);
        Oop explicit_free=send1(named("Alien"),"alloc:",fixture_integer_make(32));assert(registered==2);
        assert(send0(explicit_free,"free")==api->nil_obj && !*(uintptr_t *)explicit_free);
        assert(send0(explicit_free,"free")!=api->nil_obj && !*(uintptr_t *)explicit_free);
    } else if(is("isolate")) {
        Oop args[]={api->intern("Fixture"),api->intern("echo:"),text(getenv("BIRNAM_WORKER_EXECUTABLE"))};
        Oop handle=api->send(named("Isolate"),api->intern("start:selector:image:"),args,3);
        assert(api->vtable_of(handle)==named("IsolateHandleVt") && registered==1);read_worker_pid();assert(kill(worker_pid,0)==0);
    } else { assert(!"unknown consumer"); }
}
int main(int argc,char **argv) {
    assert(argc==4);library=argv[2];mode=argv[3];
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
    fixture_set_auto(0);fixture_set_precise(1);
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    mount(load_sibling(argv[1],"object_model"));
    if(!is("builder"))mount(load_sibling(argv[1],"collection"));
    mount(load_sibling(argv[1],"error"));
    void *trace=load_sibling(argv[1],"trace");mount(trace);
    const TraceLiveApiV1 *tracer=((TraceLiveProviderV1)dlsym(trace,"birnam_trace_provider_v1"))();assert(tracer);actual_trace=tracer->trace;
    TraceLiveApiV1 watched={1,sizeof(watched),traced};assert(((TraceLiveDependencyV1)dlsym(floor,"birnam_trace_dependency_v1"))(&watched)==1);
    fixture_mount_collect(api,argv[1],fixture_root_registry());
    void *image=dlopen(library,RTLD_NOW|RTLD_LOCAL);assert(image);fixture_attach_integer(api,image,fixture_root_registry());
    ModuleInitFn initialize=(ModuleInitFn)dlsym(image,"module_init");assert(initialize);
    selected=*api;other=*api;
#ifdef FINALIZERS_BASELINE
    selected.set_finalizer=first;other.set_finalizer=forbidden;
#else
    FinalizersV1 selected_cap={1,sizeof(FinalizersV1),&first_context,first_set},other_cap={1,sizeof(FinalizersV1),&other_context,other_set};
#endif
    INIT(initialize,&selected,&selected_cap);
    if(is("alien-replace")) { INIT(initialize,&other,&other_cap);abort(); }
    if(is("isolate")) { isolate_drain=(void (*)(void))dlsym(image,"birnam_isolate_drain");assert(isolate_drain); }
    for(phase=0;phase<3;phase++) {
        if(phase) {
#ifdef FINALIZERS_BASELINE
            selected.set_finalizer=saved;
#else
            selected_cap.context=&saved_context;selected_cap.set=saved_set;
#endif
            INIT(initialize,is("alien")?&selected:&other,&other_cap);
        }
        registered=finished=0;capture=1;exercise();capture=0;
        assert(registered);api->gc_collect();assert(finished==registered);
        for(unsigned i=0;i<registered;i++)assert(owners[i].done && !api->vtable_of(owners[i].object));
        api->gc_collect();assert(finished==registered);
        if(is("isolate")) { assert(kill(worker_pid,0)==0);isolate_drain();assert(kill(worker_pid,0)==-1 && errno==ESRCH); }
        printf("%s phase %u: %u initialized owners, saved selection, detachment and one cleanup each\n",mode,phase,registered);
        registered=0;
    }
    if(is("isolate")) {
        failed_behavior=named("IsolateHandleVt");selected.alloc=allocation;capture=1;
        Oop args[]={api->intern("Fixture"),api->intern("echo:"),text(getenv("BIRNAM_WORKER_EXECUTABLE"))};
        assert(api->send(named("Isolate"),api->intern("start:selector:image:"),args,3)==api->nil_obj);
        read_worker_pid();assert(!registered && kill(worker_pid,0)==-1 && errno==ESRCH);
        puts("isolate failed allocation terminates the registered worker without attaching cleanup");
    }
}
