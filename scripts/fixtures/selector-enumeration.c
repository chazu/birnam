/* Ordered native reflection through the production Object Model and Id. */
#include "idapi_v20.h"
#include "reflection-fixture.h"


#include "../../abi/fixtures/idapi-v20/storage_v1.h"
static const VtableEnumerationV2 *enumeration;
static Oop roots[70],candidate;
static size_t checked_index,root_calls;
static Oop at(Oop vtable,size_t index) {
    return vtable_selector_at_v2(enumeration,vtable,index);
}
static Oop answer(Oop self,const Oop *args,size_t count) { (void)args;(void)count;return self; }
static Oop forbidden(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;assert(!"enumeration invoked lookup");return NULL; }
static void mark_roots(MarkFn mark) { for(size_t i=0;i<70;i++)mark(roots[i]); }
static void check(void) { assert(at(candidate,checked_index)==roots[2]); }
static void probe_roots(MarkFn mark) { (void)mark;root_calls++;check(); }
static int inspect(void *context,const StorageViewV1 *view) { (void)context;(void)view;check();return STORAGE_OK; }
__attribute__((noinline)) static void collect_now(void) {
    uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));api->gc_collect();
}
static BirnamOutcome exercise(const BirnamProtocol *id,const BirnamOperand *args,size_t argc) {
    (void)id;assert(argc==7);reflection_start(args,argc);enumeration=fixture_word(args[4]);fixture_set_auto(0);
    fixture_register_root(mark_roots);
    roots[0]=api->proto(api->object_vt);roots[1]=api->proto(roots[0]);
    for(size_t i=0;i<64;i++) { char name[48];snprintf(name,sizeof(name),"selector-enumeration-%zu",i);roots[i+2]=api->intern(name); }
    candidate=roots[1];api->method(candidate,roots[2],answer);
    if(!strcmp(mode,"observation")) {
        StorageRequestV1 request={1,sizeof(request),NULL,inspect};assert(observe(&request)==STORAGE_OK);
        puts("native selector enumeration remains callable during storage observation");return (BirnamOutcome){0};
    }
    if(!strcmp(mode,"collection")) { fixture_register_root(probe_roots);collect_now();assert(root_calls);puts("selector observation during root collection passed");return (BirnamOutcome){0}; }
    if(!strcmp(mode,"typed")) {
        Oop selector=api->intern("selectorAt:in:");roots[68]=api->alloc(roots[0],0);
        for(size_t i=0;i<2;i++) {
            BirnamOperand query[]={{.kind=BIRNAM_WORD,.data.word=i?SIZE_MAX:0},
                {.kind=BIRNAM_OBJECT,.data.object=roots[68]}};
            assert(id->message(id->context,id->receiver,selector,query,2).status==BIRNAM_INVALID_REQUEST);
            query[1].data.object=candidate;
            BirnamOutcome result=id->message(id->context,id->receiver,selector,query,2);
            assert(result.status==BIRNAM_LOADED);
            assert(i?result.value.kind==BIRNAM_UNIT:result.value.kind==BIRNAM_OBJECT && result.value.data.object==roots[2]);
            query[0]=(BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=roots[2]};
            assert(id->message(id->context,id->receiver,selector,query,2).status==BIRNAM_INVALID_REQUEST);
        }
        puts("typed selector requests reject non-behaviors before bounds and reject non-Word indices");
        return (BirnamOutcome){0};
    }
    assert(!strcmp(mode,"ordered"));
    Dl_info owner;assert(dladdr((void *)enumeration->selector_at,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_object_model.dylib"));
    api->method(roots[0],roots[65],answer);
    for(size_t i=1;i<64;i++)api->method(candidate,roots[i+2],answer);
    for(size_t i=0;i<64;i++)assert(at(candidate,i)==roots[i+2]);
    assert(!at(candidate,64) && !at(candidate,SIZE_MAX));
    for(size_t i=0;i<64;i++)api->method(candidate,roots[i+2],forbidden);
    for(size_t i=0;i<64;i++)assert(at(candidate,i)==roots[i+2]);
    Oop remove=api->intern("removeMethod:");
    for(size_t i=0;i<64;i+=2)api->send(candidate,remove,&roots[i+2],1);
    for(size_t i=0;i<32;i++)assert(at(candidate,i)==roots[i*2+3]);
    for(size_t i=0;i<64;i+=2)api->method(candidate,roots[i+2],answer);
    for(size_t i=0;i<32;i++)assert(at(candidate,i+32)==roots[i*2+2]);
    fixture_set_precise(1);collect_now();
    for(size_t i=0;i<32;i++)assert(at(candidate,i)==roots[i*2+3]);
    /* Raw reflection does not send to the behavior's replaceable lookup. */
    Oop lookup=api->intern("lookup");
    api->method(api->vtable_vt,lookup,forbidden);
    for(size_t i=0;i<32;i++)assert(at(candidate,i+32)==roots[i*2+2]);
    assert(!at(candidate,64));
    puts("selector enumeration: ordered growth, overwrite, removal, append, bounds, GC and raw observation passed");
    return (BirnamOutcome){0};
}

FIXTURE_NATIVE_ENTRY(exercise)
