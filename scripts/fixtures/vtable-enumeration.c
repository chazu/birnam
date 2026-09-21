#include "idapi_v20.h"
#include "reflection-fixture.h"

#include <stdlib.h>
static const VtableEnumerationV2 *enumeration;
static Oop roots[64];
static Oop created[64];
static void mark_roots(MarkFn mark) { for(size_t i=0;i<64;i++)mark(roots[i]); }
__attribute__((noinline)) static void collect_now(void) {
    uintptr_t anchor=0;
    fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    (void)api->gc_collect();
}
static size_t count(void) {
    return vtable_count_v2(enumeration);
}
static Oop at(size_t index) {
    return vtable_at_v2(enumeration,index);
}
static BirnamOutcome exercise(const BirnamProtocol *id,const BirnamOperand *args,size_t argc) {
    (void)id;assert(argc==7);reflection_start(args,argc);enumeration=fixture_word(args[4]);
    fixture_set_auto(0);
    fixture_register_root(mark_roots);fixture_set_precise(1);collect_now();
    const VtableEnumerationV2 *selected=enumeration;
    Dl_info owner;
    assert(dladdr((void *)selected->count,&owner) && strstr(owner.dli_fname,"libbirnam_object_model"));
    assert(dladdr((void *)selected->at,&owner) && strstr(owner.dli_fname,"libbirnam_object_model"));
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
    return (BirnamOutcome){0};
}

FIXTURE_NATIVE_ENTRY(exercise)
