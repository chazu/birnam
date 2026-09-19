#include "../../abi/fixtures/idapi-v9/idapi_v9.h"
#include "namespace-support.h"
#include "../../abi/fixtures/collect-v1/collect_v1.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <signal.h>
#include <stdio.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

static IdApi *api;
static CollectV1 actual_collect;
static CollectDependencyV1 attach;
static CollectApiV1 mounted;
static CollectSourceV1 original;
static TraceLiveV1 actual_trace;
static TraceLiveSourceV1 trace_source;
static Oop victims[3], finalized[3];
static size_t calls, finalized_count, initial_calls, replacement_calls;
static unsigned stage;
static const uintptr_t *dead;
static size_t dead_count, cursor, before_detach;
static uintptr_t outstanding;
static void (*current_finalizer)(void *);
static int settlement, fail_trace, finalizer_pipe=-1;

/* Independent traversal wrapper prevents conservative stack accidents from
   hiding the three chosen dead candidates; all other admission stays real. */
static int admission(void *context, uintptr_t address) {
    for(size_t i=0;i<3;i++) if(address==(uintptr_t)victims[i]) return 0;
    return trace_source.admit(context,address);
}
static int traced(const TraceLiveSourceV1 *source) {
    trace_source=*source;
    TraceLiveSourceV1 watched=*source; watched.admit=admission;
    return actual_trace(&watched);
}
static void phase(void *ctx, uint32_t value) {
    if(value==1) { assert(stage==0); stage=1; }
    else if(value==2) { assert(stage==7); stage=8; }
    else { assert(value==0 && stage==12); stage=13; }
    original.phase(ctx,value);
}
static void clear_marks(void *ctx) {
    assert(stage==1 || stage==10); stage++;
    original.clear_marks(ctx);
}
static int trace(void *ctx) {
    assert(stage==2); stage=3;
    assert(api->gc_collect()==0);
    assert(actual_collect(NULL,NULL)==2);
    assert(!attach(&mounted));
    int result=original.trace(ctx); assert(result==0);
    return fail_trace?-77:0;
}
static int inspect(void *ctx) {
    assert(stage==3); stage=4;
    settlement=original.inspect_retirement(ctx);
    assert(settlement==0 || settlement==1); return settlement;
}
static void settle(void *ctx) {
    assert(stage==4 && settlement==1); stage=5;
    original.settle_deferred(ctx);
}
static void retire(void *ctx) {
    assert(stage==(settlement?5u:4u)); stage=6;
    original.retire_metadata(ctx);
}
static void snapshot(void *ctx, CollectDeadV1 *out) {
    assert(stage==6); stage=7;
    original.snapshot_dead(ctx,out);
    dead=out->addresses; dead_count=out->count; cursor=0;
}
static void finalizer(void *object) {
    assert(stage==9 && (uintptr_t)object==outstanding);
    assert(api->gc_live_count()==before_detach-1);
    assert(api->gc_collect()==0);
    assert(actual_collect(NULL,NULL)==2);
    assert(!attach(&mounted));
    current_finalizer(object);
}
static int detach(void *ctx, uintptr_t address, CollectDetachedV1 *out) {
    assert(stage==8 && cursor<dead_count && address==dead[cursor]); stage=9;
    before_detach=api->gc_live_count();
    int result=original.detach(ctx,address,out); assert(result==0);
    assert((uintptr_t)out->object==address);
    outstanding=address; current_finalizer=out->finalizer;
    if(current_finalizer) out->finalizer=finalizer;
    return result;
}
static int reclaim(void *ctx, uintptr_t address) {
    assert(stage==9 && address==outstanding);
    int result=original.reclaim(ctx,address); assert(result==0);
    cursor++; stage=8; return result;
}
static size_t reclaim_deferred(void *ctx) {
    assert(stage==8 && cursor==dead_count); stage=10;
    return original.reclaim_deferred(ctx);
}
static void reset(void *ctx) {
    assert(stage==11); stage=12; original.reset_accounting(ctx);
}
static void prune(void *ctx) {
    assert(stage==13); stage=14; original.prune(ctx);
}
static int collected(const CollectSourceV1 *source, size_t *count) {
    assert(source && source->version==1 && source->byte_size==sizeof(*source));
    calls++; original=*source; stage=0;
    CollectSourceV1 watched=*source;
    watched.phase=phase; watched.clear_marks=clear_marks; watched.trace=trace;
    watched.inspect_retirement=inspect; watched.settle_deferred=settle;
    watched.retire_metadata=retire; watched.snapshot_dead=snapshot;
    watched.detach=detach; watched.reclaim=reclaim;
    watched.reclaim_deferred=reclaim_deferred; watched.reset_accounting=reset; watched.prune=prune;
    int result=actual_collect(&watched,count);
    if(fail_trace) { assert(result==-77 && stage==3); }
    else { assert(result==0 && stage==14 && *count>=dead_count); }
    return result;
}
static void check_victim(Oop object) {
    assert(finalized_count<3);
    assert(*(uintptr_t *)object==0xface);
    size_t index=0;
    for(size_t i=0;i<cursor;i++)
        for(size_t j=0;j<3;j++) if(dead[i]==(uintptr_t)victims[j]) index++;
    assert(index==finalized_count);
    finalized[finalized_count++]=object;
    if(finalizer_pipe>=0) assert(write(finalizer_pipe,"F",1)==1);
}
static void replacement(Oop object) { replacement_calls++; check_victim(object); }
static void initial(Oop object) {
    initial_calls++; check_victim(object);
    for(size_t i=0;i<3;i++) if(victims[i]!=object) {
        /* Later dead allocations remain registered and mutable until their turn. */
        assert(api->vtable_of(victims[i])==api->object_vt);
        api->set_finalizer(victims[i],replacement);
    }
}
static void must_abort(void) {
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
    assert(argc==4);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    void *tracer=api->dlopen_fn(argv[2],RTLD_NOW|RTLD_LOCAL); assert(tracer);
    ((ModuleInitFn)api->dlsym_fn(tracer,"module_init"))(api,NULL,fixture_namespace_presence());
    const TraceLiveApiV1 *trace_api=((TraceLiveProviderV1)api->dlsym_fn(tracer,"birnam_trace_provider_v1"))();
    actual_trace=trace_api->trace;
    TraceLiveApiV1 watched_trace=*trace_api; watched_trace.trace=traced;
    assert(((TraceLiveDependencyV1)dlsym(floor,"birnam_trace_dependency_v1"))(&watched_trace)==1);
    must_abort(); /* trace alone is insufficient; there is no coordinator fallback */
    void *collector=api->dlopen_fn(argv[3],RTLD_NOW|RTLD_LOCAL); assert(collector);
    ((ModuleInitFn)api->dlsym_fn(collector,"module_init"))(api,NULL,fixture_namespace_presence());
    const CollectApiV1 *implementation=((CollectProviderV1)api->dlsym_fn(collector,"birnam_collect_provider_v1"))();
    actual_collect=implementation->collect; mounted=*implementation; mounted.collect=collected;
    attach=(CollectDependencyV1)dlsym(floor,"birnam_collect_dependency_v1"); assert(attach);
    CollectApiV1 bad=mounted; bad.version++;
    assert(!attach(NULL) && !attach(&bad));
    bad=mounted; bad.byte_size--; assert(!attach(&bad));
    bad=mounted; bad.collect=NULL; assert(!attach(&bad));
    assert(attach(&mounted)==1 && attach(&mounted)==1);
    assert(!attach(implementation));
    for(size_t i=0;i<3;i++) {
        victims[i]=api->alloc(api->object_vt,sizeof(uintptr_t));
        *(uintptr_t *)victims[i]=0xface; api->set_finalizer(victims[i],initial);
    }
    fail_trace=1; must_abort(); /* failed mark never retires, detaches or finalizes */
    fail_trace=0;
    size_t before=calls; (void)api->gc_collect();
    assert(calls==before+1 && initial_calls==1 && replacement_calls==2 && finalized_count==3);
    for(size_t i=0;i<3;i++) victims[i]=NULL;
    before=calls; api->gc_set_auto(1);
    (void)api->alloc(api->object_vt,16u<<20);
    (void)api->alloc(api->object_vt,0);
    assert(calls==before+1);
    printf("collection: actual loaded coordinator, explicit/automatic GC, ordered finalizer replacement, nesting, fail-stop passed\n");
    return 0;
}
