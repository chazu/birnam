#include "../../abi/fixtures/idapi-v17/idapi_v17.h"
#include "namespace-support.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include "collect-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

static IdApi *api;
static TraceLiveV1 actual_trace;
static TraceLiveDependencyV1 attach;
static TraceLiveApiV1 mounted;
static TraceLiveSourceV1 original;
static uintptr_t pending[65536];
static size_t pending_count, trace_calls, edge_calls, provider_calls, hook_calls;
static Oop roots[2], child;
static int fail_trace;
static Oop failure_victim;
static int finalizer_pipe=-1;

static int admission(void *context, uintptr_t address) {
    /* On the injected failing run, deliberately leave a victim unmarked. Any
       accidental sweep must then invoke its finalizer and become observable. */
    if(fail_trace && address==(uintptr_t)failure_victim) return 0;
    int result=original.admit(context,address);
    if(result==1) { assert(pending_count<65536); pending[pending_count++]=address; }
    return result;
}
static int edges(void *context, uintptr_t address, const TraceLiveSinkV1 *sink) {
    /* Independent event-order oracle observes actual floor first-mark decisions. */
    assert(pending_count && pending[--pending_count]==address);
    edge_calls++;
    return original.edges(context,address,sink);
}
static int traced(const TraceLiveSourceV1 *source) {
    assert(source && source->version==1 && source->byte_size==sizeof(*source));
    trace_calls++;
    original=*source;
    TraceLiveSourceV1 watched=*source;
    watched.admit=admission; watched.edges=edges;
    pending_count=0;
    int result=actual_trace(&watched);
    assert(result==0 && !pending_count);
    return fail_trace?-77:0;
}
static void provider(MarkFn mark) {
    provider_calls++;
    assert(api->gc_collect()==0); /* existing collection nesting rule */
    assert(actual_trace(NULL)==2); /* live driver cannot reenter */
    assert(attach(&mounted)==0); /* mounting is not allowed while marking */
    mark(roots[0]); mark(roots[1]); mark(roots[0]);
}
static void hook(Oop object, MarkFn mark) {
    assert(object==roots[0] || object==roots[1]);
    assert(api->vtable_of(object)); /* legacy hook retains ordinary Id reads */
    hook_calls++;
    mark(child);
}
static void failure_finalizer(Oop object) {
    assert(object==failure_victim);
    if(finalizer_pipe>=0) assert(write(finalizer_pipe,"F",1)==1);
}
static void must_abort_collection(void) {
    int pipes[2]; assert(pipe(pipes)==0); finalizer_pipe=pipes[1];
    pid_t pid=fork(); assert(pid>=0);
    if(pid==0) {
        close(pipes[0]);
        struct rlimit limit={0,0}; setrlimit(RLIMIT_CORE,&limit);
        (void)freopen("/dev/null","w",stderr);
        api->gc_collect(); _exit(99);
    }
    close(pipes[1]); finalizer_pipe=-1;
    int status; assert(waitpid(pid,&status,0)==pid);
    assert(WIFSIGNALED(status) && WTERMSIG(status)==SIGABRT);
    char byte; assert(read(pipes[0],&byte,1)==0); close(pipes[0]);
}
int main(int argc, char **argv) {
    assert(argc==3);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    IdInitFn initialize=(IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")); assert(initialize);
    api=initialize(); assert(api); fixture_set_auto(0);
    uintptr_t stack_anchor=0;
    api->gc_set_stack_bottom((uintptr_t)&stack_anchor+sizeof(stack_anchor));
    attach=(TraceLiveDependencyV1)dlsym(floor,"birnam_trace_dependency_v1"); assert(attach);
    fixture_mount_collect(api,argv[1],fixture_root_registry());
    must_abort_collection(); /* no in-Id fallback */
    void *image=api->dlopen_fn(argv[2],RTLD_NOW|RTLD_LOCAL); assert(image);
    ((ModuleInitFn)api->dlsym_fn(image,"module_init"))(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout());
    TraceLiveProviderV1 provide=(TraceLiveProviderV1)api->dlsym_fn(image,"birnam_trace_provider_v1"); assert(provide);
    const TraceLiveApiV1 *implementation=provide(); assert(implementation);
    actual_trace=implementation->trace; assert(actual_trace);
    mounted=*implementation; mounted.trace=traced;
    TraceLiveApiV1 invalid=mounted; invalid.version++;
    assert(!attach(NULL) && !attach(&invalid));
    invalid=mounted; invalid.trace=NULL; assert(!attach(&invalid));
    assert(attach(&mounted)==1 && attach(&mounted)==1);
    assert(attach(implementation)==0); /* immutable selection; no silent replacement */
    Oop vtable=api->proto(api->object_vt);
    roots[0]=api->alloc(vtable,sizeof(Oop)); roots[1]=api->alloc(vtable,sizeof(Oop));
    child=api->alloc(api->object_vt,0);
    *(Oop *)roots[0]=roots[1]; *(Oop *)roots[1]=roots[0];
    api->set_trace_hook(vtable,hook);
    fixture_register_root(provider);
    /* Keep a root only on the native stack, separate from the provider graph. */
    Oop volatile native_root=api->alloc(api->object_vt,sizeof(uintptr_t));
    *(uintptr_t *)native_root=0x12345678;
    for(int precise=0; precise<=1; precise++) {
        uint8_t kind=1; fixture_set_layout(vtable,&kind,1,0);
        fixture_set_precise(precise);
        size_t before=trace_calls;
        (void)api->gc_collect();
        assert(trace_calls==before+1 && provider_calls==trace_calls);
        assert(hook_calls==2*trace_calls);
        assert(api->vtable_of(child)==api->object_vt);
        assert(api->vtable_of(native_root)==api->object_vt);
        assert(*(uintptr_t *)native_root==0x12345678);
    }
    size_t before=trace_calls;
    fixture_set_auto(1);
    (void)api->alloc(api->object_vt,16u<<20);
    (void)api->alloc(api->object_vt,0);
    assert(trace_calls==before+1 && provider_calls==trace_calls && edge_calls>0);
    fixture_set_auto(0);
    failure_victim=api->alloc(api->object_vt,0);
    api->set_finalizer(failure_victim,failure_finalizer);
    fail_trace=1; must_abort_collection(); /* failure cannot fall through to sweep */
    fail_trace=0;
    (void)api->gc_collect(); assert(trace_calls==before+2);
    puts("live-tracing: loaded GC traversal, admission/LIFO oracle, native roots, legacy hooks, automatic collection, mount rejection and failure-before-sweep passed");
    return 0;
}
