/* Observe identities issued by the actual production Id allocator. */
#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
#include "namespace-support.h"
#include "trace-support.h"
static IdApi *api;
static Oop retained[16],final_owner;
static uint64_t identities[16],final_identity;
static Oop next_object;
static unsigned cleaned;
static void roots(MarkFn mark) { for(unsigned i=0;i<16;i++)mark(retained[i]); }
static void cleanup(Oop object) { assert(object==final_owner && !api->vtable_of(object));assert(*(uint64_t *)object==0xa110ca7e);cleaned++; }
__attribute__((noinline)) static void allocate_final_owner(void) {
    final_owner=api->alloc(api->object_vt,8);*(uint64_t *)final_owner=0xa110ca7e;
    fixture_set_finalizer(final_owner,cleanup);
}
__attribute__((noinline)) static void collect_after_return(void) {
    volatile uintptr_t scratch[8192];for(size_t i=0;i<8192;i++)scratch[i]=0;
    api->gc_collect();assert(scratch[8191]==0);
}
static int visitor(void *context,const StorageViewV1 *view) {
    int after=*(int *)context;unsigned found=0;uint64_t previous=0;
    for(size_t i=0;i<view->allocation_count;i++) {
        const StorageAllocationV1 *entry=&view->allocations[i];assert(entry->identity>previous);previous=entry->identity;
        assert(entry->identity <= (UINT64_MAX>>5));
        if(entry->object==next_object) { assert(after==2 && entry->identity==final_identity+1);found++; }
        for(unsigned j=0;j<16;j++)if(entry->object==retained[j]) {
            if(!identities[j]) identities[j]=entry->identity;
            assert(entry->identity==identities[j] && entry->requested_size==j);
            if(j) assert(identities[j]==identities[j-1]+1);
            assert(entry->padded_size==((j+7u)&~7u) && entry->allocation_bytes==8+entry->padded_size);
            assert(entry->behavior==api->object_vt);found++;
        }
        if(after) assert(entry->identity!=final_identity);
        if(!after && entry->object==final_owner) { assert(entry->flags & STORAGE_HAS_FINALIZER);final_identity=entry->identity;assert(final_identity==identities[15]+1);found++; }
    }
    assert(found==(after==1?16:17));return STORAGE_OK;
}
int main(int argc,char **argv) {
    assert(argc==3);void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    assert(!dlsym(floor,"birnam_allocation_identity_dependency_v1"));
    fixture_prepare_namespace(floor,argv[1]);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);fixture_set_auto(0);
    assert(!strcmp(argv[2],"normal"));
    fixture_set_precise(1);fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_register_root(roots);
    uintptr_t anchor=0;native_stack_set_v1(fixture_native_stack(),(uintptr_t)&anchor);
    for(unsigned i=0;i<16;i++) retained[i]=api->alloc(api->object_vt,i);
    allocate_final_owner();
    StorageObserveV1 observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
    int after=0;StorageRequestV1 request={1,sizeof(request),&after,visitor};
    assert(observe(&request)==STORAGE_OK);
    collect_after_return();assert(cleaned==1);after=1;
    assert(observe(&request)==STORAGE_OK);
    api->gc_collect();assert(cleaned==1);
    next_object=api->alloc(api->object_vt,0);after=2;
    assert(observe(&request)==STORAGE_OK);
    puts("allocation-identity loaded: Id-owned issuance, exact storage identities, padding, retained aliases and finalization survive GC without resetting identities");
}
