#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "namespace-support.h"
#include "collect-support.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include <pthread.h>
#include <stdio.h>

static IdApi *api;
static TraceLiveV1 actual_trace;
static TraceLiveSourceV1 original;
static TraceLiveSinkV1 selected;
static MarkFn saved;
static Oop root,child;
static const char *mode;
static unsigned phase,root_calls,hook_calls,event_count;
static unsigned events[32];
static int reemitted;
static void *wrong_thread(void *unused) { (void)unused;saved(NULL);return NULL; }
static int watched_emit(void *unused,uintptr_t value) {
    (void)unused;
    if(phase) {
        assert(event_count<32);
        events[event_count++]=phase*10+(value==(uintptr_t)root?1:value==(uintptr_t)child?2:3);
        if(!strcmp(mode,"reject"))return -77;
        if(phase==1 && !reemitted) { reemitted=1;saved(child); }
    }
    return selected.emit(selected.context,value);
}
static int watched_roots(void *context,const TraceLiveSinkV1 *sink) {
    selected=*sink;
    TraceLiveSinkV1 observed={NULL,watched_emit};
    return original.roots(context,&observed);
}
static int watched_edges(void *context,uintptr_t address,const TraceLiveSinkV1 *sink) {
    selected=*sink;
    TraceLiveSinkV1 observed={NULL,watched_emit};
    return original.edges(context,address,&observed);
}
static int traced(const TraceLiveSourceV1 *source) {
    original=*source;
    TraceLiveSourceV1 watched=*source;watched.roots=watched_roots;watched.edges=watched_edges;
    return actual_trace(&watched);
}
static void provider(MarkFn mark) {
    saved=mark;root_calls++;
    assert(api->gc_collect()==0);
    if(!strcmp(mode,"thread")) { pthread_t thread;assert(!pthread_create(&thread,NULL,wrong_thread,NULL));pthread_join(thread,NULL); }
    if(!strcmp(mode,"nested")) {
        TraceLiveSinkV1 observed={NULL,watched_emit};
        original.roots(original.context,&observed);
        assert(0);
    }
    phase=1;unsigned before=event_count;mark(NULL);assert(event_count==before);
    mark(root);mark(root);mark((Oop)(uintptr_t)1);phase=0;
}
static void hook(Oop object,MarkFn mark) {
    assert(object==root);hook_calls++;phase=2;
    assert(mark==saved);assert(api->gc_collect()==0);
    mark(child);mark(child);mark(NULL);phase=0;
}
static void other_floor_provider(MarkFn mark) { (void)mark;saved(NULL); }
int main(int argc,char **argv) {
    assert(argc==4 || argc==5);mode=argv[3];
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
    fixture_set_auto(0);fixture_set_precise(1);
    uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_mount_collect(api,argv[1],fixture_root_registry());
    void *trace=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(trace);
    TraceLiveProviderV1 provide=(TraceLiveProviderV1)dlsym(trace,"birnam_trace_provider_v1");assert(provide);
    const TraceLiveApiV1 *implementation=provide();assert(implementation);
    actual_trace=implementation->trace;TraceLiveApiV1 watched=*implementation;watched.trace=traced;
    TraceLiveDependencyV1 attach=(TraceLiveDependencyV1)dlsym(floor,"birnam_trace_dependency_v1");assert(attach(&watched)==1);
    Oop vt=api->proto(api->object_vt);fixture_set_layout(vt,NULL,0,IDAPI_V20_SLOT_RAW);
    fixture_set_trace_hook(vt,hook);root=api->alloc(vt,sizeof(Oop));child=api->alloc(api->object_vt,0);*(Oop *)root=child;
    fixture_register_root(provider);
    for(unsigned round=0;round<2;round++) {
        event_count=0;reemitted=0;api->gc_collect();
        assert(root_calls==round+1 && hook_calls==round+1);
        const unsigned expected[]={11,12,11,13,22,22};
        assert(event_count==6 && !memcmp(events,expected,sizeof(expected)));
        assert(api->vtable_of(root)==vt && api->vtable_of(child)==api->object_vt);
    }
    if(!strcmp(mode,"other-floor")) {
        assert(argc==5);
        void *second=dlopen(argv[4],RTLD_NOW|RTLD_LOCAL);assert(second && second!=floor);
        fixture_prepare_namespace(second,argv[4]);
        api=((IdInitFn)dlsym(second,"id_init"))();assert(api);
        fixture_set_auto(0);fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
        fixture_mount_collect(api,argv[4],fixture_root_registry());
        attach=(TraceLiveDependencyV1)dlsym(second,"birnam_trace_dependency_v1");assert(attach(&watched)==1);
        fixture_register_root(other_floor_provider);api->gc_collect();assert(0);
    }
    if(!strcmp(mode,"escaped-null"))saved(NULL);
    if(!strcmp(mode,"escaped-value"))saved(root);
    puts("native-mark: copied active sink, immediate reentry, ordered duplicates, nulls, hooks and nested collection preserved");
    return 0;
}
