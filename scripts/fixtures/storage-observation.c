#include "../../abi/fixtures/storage-v1/storage_v1.h"
#include "namespace-support.h"
#include "integer-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

static IdApi *api;
static StorageObserveV1 observe;
static StorageDescribeV1 describe;
static Oop samples[5], copied, pair, raw_integer, child, pair_vt;
static Oop storage_selector, lookup_selector, owner_selector, parent_selector;
static Method original, captured_owner, captured_parent;
static uint64_t runtime_identity, pair_identity, integer_identity;
static int wrapper_calls, root_calls, trace_calls, visitor_calls, described_calls;
static int mutation, claim_mode;
static StorageSinkV1 saved_sink;
static IntegerApiV1 *integers;

static const StorageAllocationV1 *allocation(const StorageViewV1 *view, Oop object) {
    for (size_t i=0; i<view->allocation_count; i++) if (view->allocations[i].object==object) return &view->allocations[i];
    abort();
}
static int contains(const Oop *objects, size_t count, Oop object) {
    for (size_t i=0; i<count; i++) if (objects[i]==object) return 1;
    return 0;
}
static void legacy_roots(MarkFn mark) { root_calls++; mark(pair); }
static void legacy_trace(Oop self, MarkFn mark) { (void)self; trace_calls++; mark(child); }
static void finalizer(Oop self) { (void)self; abort(); }
static Method method(Oop selector) {
    return (Method)api->send(api->vtable_vt, lookup_selector, &selector, 1);
}
static Oop wrapper(Oop self, const Oop *args, size_t count) {
    wrapper_calls++;
    return original(self,args,count);
}
static int described(void *context, const StorageDescribedViewV1 *view) {
    (void)context;
    described_calls++;
    assert(view->description_count==2);
    assert(view->payload_edge_count==2);
    for(size_t i=0; i<view->payload_edge_count; i++) assert(view->payload_edges[i].from==pair);
    assert(contains(view->roots,view->root_count,pair));
    assert(view->missing_description_count>0); /* unmigrated production owners */
    assert(view->coverage_gaps & STORAGE_MISSING);
    return STORAGE_OK;
}
static int provide(void *context, const StorageViewV1 *view, const StorageSinkV1 *sink) {
    (void)context; (void)view;
    saved_sink=*sink;
    size_t offsets[]={0, sizeof(Oop)};
    StorageDescriptionV1 pair_desc={runtime_identity,pair_identity,pair,2*sizeof(Oop),offsets,2};
    StorageDescriptionV1 integer_desc={runtime_identity,integer_identity,raw_integer,sizeof(int64_t),NULL,0};
    if (claim_mode==1) pair_desc.allocation_identity++;
    if (claim_mode==3) pair_desc.extent--;
    if (claim_mode==4) offsets[1]=0;
    if (claim_mode==5) offsets[1]=1;
    if (claim_mode==6) offsets[1]=2*sizeof(Oop);
    if (claim_mode==7) (void)sink->root(sink->context,(Oop)(uintptr_t)1);
    if (claim_mode==8) pair_desc.strong_offsets=NULL;
    (void)sink->root(sink->context,pair);
    (void)sink->describe(sink->context,&pair_desc);
    if (claim_mode==2) (void)sink->describe(sink->context,&pair_desc);
    (void)sink->describe(sink->context,&integer_desc);
    return STORAGE_OK; /* ignored sink failures must still reject the report */
}
static int inspect(void *context, const StorageViewV1 *view) {
    (void)context;
    visitor_calls++;
    assert(view->version==1 && view->byte_size==sizeof(*view));
    assert(view->runtime_identity && view->observation_identity);
    assert(view->coverage_gaps & STORAGE_NATIVE_ROOTS);
    assert(view->coverage_gaps & STORAGE_LEGACY_ROOTS);
    assert(view->legacy_root_provider_count>=2);
    assert(root_calls==0 && trace_calls==0);
    if (!runtime_identity) {
        runtime_identity=view->runtime_identity;
        pair_identity=allocation(view,pair)->identity;
        integer_identity=allocation(view,raw_integer)->identity;
    }
    const size_t requested[]={0,1,7,8,9};
    for(size_t i=0; i<5; i++) {
        const StorageAllocationV1 *a=allocation(view,samples[i]);
        assert(a->requested_size==requested[i]);
        assert(a->padded_size==((requested[i]+7)&~(size_t)7));
        assert(a->allocation_bytes==a->padded_size+sizeof(Oop));
    }
    assert(allocation(view,copied)->requested_size==16);
    assert(allocation(view,pair)->identity==pair_identity);
    assert(contains(view->roots,view->root_count,pair));
    StorageRequestV1 nested={1,sizeof(nested),NULL,inspect};
    assert(observe(&nested)==STORAGE_BUSY);
    if(mutation) {
        Oop args[]={pair_vt,pair};
        switch(mutation) {
        case 1: api->intern("forbidden-observation-intern"); break;
        case 2: fixture_namespace_export("forbidden-observation-export",pair); break;
        case 3: api->alloc(pair_vt,0); break;
        case 4: api->send(api->vtable_vt,lookup_selector,&owner_selector,1); break;
        case 5: api->set_layout(pair_vt,NULL,0,0); break;
        case 6: captured_owner(api->vtable_vt,args,2); break;
        case 7: captured_parent(api->vtable_vt,args,2); break;
        case 8: api->method(api->vtable_vt,storage_selector,wrapper); break;
        case 9: api->set_trace_hook(pair_vt,legacy_trace); break;
        case 10: api->gc_collect(); break;
        case 11: api->gc_register_root_provider(legacy_roots); break;
        case 12: api->gc_set_auto(1); break;
        case 13: api->set_finalizer(pair,finalizer); break;
        default: abort();
        }
        _exit(99);
    }
    StorageProviderV1 provider={NULL,provide};
    StorageDescribeRequestV1 request={1,sizeof(request),&provider,1,0,NULL,described};
    int before=described_calls;
    for(claim_mode=1; claim_mode<=8; claim_mode++) assert(describe(view,&request)==STORAGE_INVALID);
    assert(described_calls==before);
    claim_mode=0;
    assert(describe(view,&request)==STORAGE_OK);
    assert(saved_sink.root(saved_sink.context,pair)==STORAGE_BUSY);
    request.reject_gaps=STORAGE_MISSING;
    assert(describe(view,&request)==STORAGE_INCOMPLETE);
    request.reject_gaps=STORAGE_NATIVE_ROOTS;
    assert(describe(view,&request)==STORAGE_INCOMPLETE);
    return STORAGE_OK;
}
static int bare(void *context, const StorageViewV1 *view) {
    (void)context;
    assert(view->version==1 && view->allocation_count>0 && view->root_count>0);
    return STORAGE_OK;
}
static int foreign_reference(void *context, const StorageViewV1 *view) {
    (void)context;
    StorageProviderV1 provider={NULL,provide};
    StorageDescribeRequestV1 request={1,sizeof(request),&provider,1,0,NULL,described};
    return describe(view,&request);
}
static int fail(void *context, const StorageViewV1 *view) { (void)context; (void)view; return -77; }
static Oop nested_method(Oop self, const Oop *args, size_t count) {
    (void)self; (void)args; (void)count;
    StorageRequestV1 request={1,sizeof(request),NULL,fail};
    assert(observe(&request)==STORAGE_BUSY);
    return api->nil_obj;
}
int main(int argc, char **argv) {
    assert(argc==4);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); if(!floor) { fputs(dlerror(),stderr); abort(); }
    IdInitFn init=(IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")); assert(init); api=init();
    observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1"); assert(observe);
    void *storage=api->dlopen_fn(argv[2],RTLD_NOW|RTLD_LOCAL); assert(storage);
    describe=(StorageDescribeV1)api->dlsym_fn(storage,"birnam_storage_describe_v1"); assert(describe);
    ((ModuleInitFn)api->dlsym_fn(storage,"module_init"))(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    StorageRequestV1 bare_request={1,sizeof(bare_request),NULL,bare};
    assert(observe(&bare_request)==STORAGE_OK);
    fixture_mount_integer(api, argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    integers=fixture_integer;
    storage_selector=api->intern("withStorageView:"); lookup_selector=api->intern("lookup");
    owner_selector=api->intern("ownerOf:put:"); parent_selector=api->intern("parentOf:put:");
    original=method(storage_selector); captured_owner=method(owner_selector); captured_parent=method(parent_selector);
    const size_t sizes[]={0,1,7,8,9};
    for(size_t i=0; i<5; i++) samples[i]=api->alloc(api->object_vt,sizes[i]);
    copied=api->send(samples[4],api->intern("clone"),NULL,0);
    pair_vt=api->proto(api->object_vt); pair=api->alloc(pair_vt,2*sizeof(Oop)); child=api->alloc(api->object_vt,0);
    ((Oop*)pair)[0]=child; ((Oop*)pair)[1]=api->nil_obj;
    raw_integer=integers->make(integers->context,(int64_t)(uintptr_t)pair);
    fixture_namespace_export("StorageObservationPair",pair);
    api->gc_register_root_provider(legacy_roots);
    api->set_trace_hook(pair_vt,legacy_trace);
    StorageRequestV1 request={1,sizeof(request),NULL,inspect};
    size_t count=api->gc_live_count();
    assert(observe(&request)==STORAGE_OK); assert(api->gc_live_count()==count);
    ((Oop*)pair)[0]=(Oop)(uintptr_t)1;
    StorageRequestV1 invalid_reference={1,sizeof(invalid_reference),NULL,foreign_reference};
    assert(observe(&invalid_reference)==STORAGE_INVALID);
    ((Oop*)pair)[0]=child;
    Oop reassociate[]={pair,integers->behavior};
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),reassociate,2);
    api->method(api->vtable_vt,storage_selector,wrapper);
    assert(observe(&request)==STORAGE_OK); assert(wrapper_calls==1);
    StorageRequestV1 failed={1,sizeof(failed),NULL,fail};
    assert(observe(&failed)==-77);
    assert(api->intern("after-observer-error"));
    assert(observe(&request)==STORAGE_OK);
    Oop nested=api->intern("storageObservationNested"); api->method(api->vtable_vt,nested,nested_method);
    api->send(api->vtable_vt,nested,NULL,0);
    Oop request_arg=&request;
    assert((uintptr_t)api->send(api->vtable_vt,storage_selector,&request_arg,1)==STORAGE_BUSY); /* no host permit */
    for(mutation=1; mutation<=13; mutation++) {
        pid_t pid=fork(); assert(pid>=0);
        if(pid==0) {
            struct rlimit limit={0,0}; setrlimit(RLIMIT_CORE,&limit);
            (void)freopen("/dev/null","w",stderr);
            (void)observe(&request); _exit(98);
        }
        int status; assert(waitpid(pid,&status,0)==pid);
        assert(WIFSIGNALED(status) && WTERMSIG(status)==SIGABRT);
    }
    assert(root_calls==0 && trace_calls==0 && visitor_calls==3 && described_calls==3);
    puts("storage-observation: extents, identity, reassociation, descriptions, coverage, error cleanup and 13 mutation guards passed");
    return 0;
}
