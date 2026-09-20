#ifdef FIXTURE_IDAPI_V11
#include "idapi_v11.h"
#else
#include "idapi_v18.h"
#endif
#include "namespace-support.h"
#include "trace-support.h"
#include <stdlib.h>
static IdApi *api;
static Oop roots[64];
static Oop created[64];
static void mark_roots(MarkFn mark) { for(size_t i=0;i<64;i++)mark(roots[i]); }
__attribute__((noinline)) static void collect_now(void) {
    uintptr_t anchor=0;
    api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    (void)api->gc_collect();
}
static size_t count(void) {
#ifdef FIXTURE_IDAPI_V11
    return api->vtable_count();
#else
    return vtable_count_v2(fixture_vtable_enumeration());
#endif
}
static Oop at(size_t index) {
#ifdef FIXTURE_IDAPI_V11
    return api->vtable_at(index);
#else
    return vtable_at_v2(fixture_vtable_enumeration(),index);
#endif
}
int main(int argc,char **argv) {
    assert(argc==2);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);
    IdInitFn initialize=(IdInitFn)dlsym(floor,"id_init");assert(initialize);api=initialize();assert(api);
    fixture_set_auto(0);fixture_mount_trace(api,argv[1],fixture_root_registry());
    fixture_register_root(mark_roots);fixture_set_precise(1);collect_now();
#ifndef FIXTURE_IDAPI_V11
    VtableEnumerationV2 *selected=fixture_vtable_enumeration();
    Dl_info owner;
    assert(dladdr((void *)selected->count,&owner) && strstr(owner.dli_fname,"libbirnam_object_model"));
    assert(dladdr((void *)selected->at,&owner) && strstr(owner.dli_fname,"libbirnam_object_model"));
#endif
    size_t baseline=count();assert(baseline && at(0)==api->vtable_vt);
    Oop *original=malloc(baseline*sizeof(Oop));assert(original);
    for(size_t i=0;i<baseline;i++)original[i]=at(i);
    assert(!at(baseline) && !at(SIZE_MAX));
    for(size_t i=0;i<64;i++) {
        created[i]=api->proto(api->object_vt);assert(created[i]);
        if(i%2==0)roots[i]=created[i];
        assert(count()==baseline+i+1 && at(baseline+i)==created[i]);
    }
    /* The copied count is not a snapshot; new indices are immediately visible. */
    assert(at(baseline)==created[0]);
    collect_now();assert(count()==baseline+32);
    for(size_t i=0;i<baseline;i++)assert(at(i)==original[i]);
    for(size_t i=0;i<32;i++)assert(at(baseline+i)==created[i*2]);
    assert(!at(baseline+32) && !at(SIZE_MAX));
    for(size_t i=0;i<64;i++)roots[i]=NULL;
    collect_now();assert(count()==baseline);
    for(size_t i=0;i<baseline;i++)assert(at(i)==original[i]);
    free(original);
    puts("vtable enumeration: live append, bounds, weak membership, GC and survivor order passed");
}
