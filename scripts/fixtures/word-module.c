#include "idapi_v13.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>

static IdApi *api;
static IdApi word_api;
static size_t reads, boxes;
static Oop (*original_make)(void *, int64_t);
static uint64_t (*original_read)(void *, Oop);
static uint64_t counted_read(void *context, Oop value) { ++reads; return original_read(context,value); }
static Oop counted_box(void *context, int64_t value) { ++boxes; return original_make(context,value); }
static unsigned preparation_collections, publication_reentries, allocations;
static Oop collecting_alloc(Oop behavior, size_t size) {
    ++preparation_collections; (void)api->gc_collect();
    return api->alloc(behavior,size);
}
static Oop counted_alloc(Oop behavior, size_t size) {
    ++allocations; return api->alloc(behavior,size);
}
static Oop reenter_publication(Oop receiver, Oop selector, const Oop *args, size_t count) {
    if(receiver==api->vtable_vt && selector==api->intern("ownerOf:put:")) {
        ++publication_reentries;
        Oop value=fixture_word_make(UINT64_MAX);
        assert(fixture_is_word(value) && fixture_word_value(value)==UINT64_MAX);
        (void)api->gc_collect();
        assert(api->vtable_of(fixture_word->prototype)==fixture_word->behavior);
    }
    return api->send(receiver,selector,args,count);
}
static void mount(const char *path, IdApi *selected) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    init(selected,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration());
}
static Oop send0(Oop receiver, const char *selector) {
    return api->send(receiver,api->intern(selector),NULL,0);
}
static Oop send(Oop receiver, const char *selector, Oop arg) {
    return api->send(receiver,api->intern(selector),&arg,1);
}
static Method lookup(const char *selector) {
    return (Method)send(fixture_word->behavior,"lookup",api->intern(selector));
}
static void expect_text(Oop receiver, const char *expected) {
    Oop value=send0(receiver,"printString"); size_t count=0;
    const uint8_t *bytes=fixture_string_bytes(value,&count);
    assert(count==strlen(expected) && !memcmp(bytes,expected,count));
}
static Oop right(Oop receiver, const Oop *args, size_t count) {
    (void)receiver; assert(count==1); return args[0];
}
int main(int argc, char **argv) {
    assert(argc==6);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]);
    assert(!fixture_namespace_contains("Word") && !fixture_namespace_contains("WordVt"));
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    word_api=*api; word_api.alloc=collecting_alloc;
    fixture_prepare_word(&word_api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    assert(preparation_collections==1); word_api.alloc=api->alloc;
    fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    assert(!lookup("asInteger"));
    word_api.send=reenter_publication;
    mount(argv[2],&word_api);
    assert(publication_reentries==1); word_api.send=api->send;
    WordProviderV1 provide=(WordProviderV1)api->dlsym_fn(fixture_word_image,"birnam_word_provider_v1");
    WordPrepareV4 prepare=(WordPrepareV4)api->dlsym_fn(fixture_word_image,"birnam_word_prepare_v4");
    assert(provide && prepare && provide()==fixture_word && prepare(&word_api,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports())==fixture_word);
    WordDependencyV1 attach=(WordDependencyV1)api->dlsym_fn(floor,"birnam_word_dependency_v1");
    assert(attach && !attach(NULL) && attach(fixture_word)==1);
    WordApiV1 copy=*fixture_word; copy.version=999; assert(!attach(&copy));
    copy=*fixture_word; assert(!attach(&copy));
    word_api.alloc=counted_alloc; (void)fixture_word_make(UINT64_MAX); assert(allocations==1); word_api.alloc=api->alloc;
    Method convert=lookup("asInteger"), equal=lookup("="), print=lookup("printString");
    assert(convert && equal && print);
    Oop high=fixture_word_make(UINT64_MAX);
    assert(send0(high,"asInteger")==api->nil_obj); /* lazy Error lookup */
    mount(argv[3],api); mount(argv[5],api); /* object-model and Error */
    const uint64_t inputs[]={0,1,17,UINT64_C(0xffffffff),UINT64_C(0x100000000),
        INT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0xfedcba9876543210),UINT64_MAX};
    unsigned pairs=0;
    for(size_t i=0;i<sizeof(inputs)/sizeof(*inputs);i++) {
        Oop word=fixture_word_make(inputs[i]);
        assert(api->object_payload_size(word)==8 && api->slot_kind(fixture_word->behavior,0)==IDAPI_V13_SLOT_RAW);
        assert(!memcmp(word,&inputs[i],8));
        uint8_t raw[10]; memset(raw,0xab,sizeof(raw));
        fixture_word->storage->initialize(raw+1,inputs[i]);
        assert(fixture_word->storage->read(raw+1)==inputs[i] && raw[0]==0xab && raw[9]==0xab);
        char expected[32]; snprintf(expected,sizeof(expected),"0x%" PRIx64,inputs[i]);
        expect_text(word,expected);
        Oop value=send0(word,"asInteger");
        if(inputs[i]<=INT64_MAX) {
            assert(fixture_is_integer(value) && fixture_integer_value(value)==(int64_t)inputs[i]);
            assert(send0(word,"asInteger")!=value);
        } else {
            assert(api->vtable_of(value)==fixture_namespace_import("ErrorVt"));
            assert(send0(value,"kind")==api->intern("overflow"));
        }
        for(size_t j=0;j<sizeof(inputs)/sizeof(*inputs);j++) {
            assert(send(word,"=",fixture_word_make(inputs[j]))==(inputs[i]==inputs[j]?api->true_obj:api->false_obj));
            ++pairs;
        }
        assert(send(word,"=",fixture_integer_make((int64_t)inputs[i]))==api->false_obj);
        assert(convert(word,NULL,1)==api->nil_obj && print(word,NULL,1)==api->nil_obj);
        assert(equal(word,NULL,0)==api->nil_obj && equal(word,NULL,1)==api->nil_obj);
    }
    assert(convert(api->true_obj,NULL,0)==api->nil_obj);
    assert(print(api->true_obj,NULL,0)==api->nil_obj);
    assert(equal(api->true_obj,&high,1)==api->nil_obj);
    assert(!fixture_is_word(NULL) && !fixture_is_word((Oop)(uintptr_t)1));
    Oop inherited=fixture_word_make(UINT64_MAX), child=api->proto(fixture_word->behavior);
    Oop reassociate[]={inherited,child};
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),reassociate,2);
    assert(!fixture_is_word(inherited) && fixture_word_value(inherited)==UINT64_MAX);
    Oop seven=fixture_word_make(7);
    original_read=fixture_word->read; fixture_word->read=counted_read;
    original_make=fixture_integer->make; fixture_integer->make=counted_box;
    assert(fixture_integer_value(send0(seven,"asInteger"))==7 && boxes==1 && reads==1);
    fixture_integer->make=original_make; fixture_word->read=original_read;
    api->method(fixture_word->behavior,api->intern("="),right);
    assert(send(seven,"=",high)==high);
    Oop prototype=fixture_word->prototype, behavior=fixture_word->behavior;
    fixture_namespace_export("Word",api->nil_obj);
    mount(argv[2],&word_api); /* Existing reinitialization reinstalls methods only. */
    assert(lookup("=")==equal && fixture_word->prototype==prototype && fixture_word->behavior==behavior);
    assert(fixture_namespace_import("Word")==api->nil_obj); fixture_namespace_export("Word",prototype);
    api->method(behavior,api->intern("="),right);
    mount(argv[4],api); /* Prelude preserves the installed Word replacement. */
    assert(lookup("=")==right && send(seven,"=",high)==high);
    api->method(fixture_word->behavior,api->intern("="),equal);
    Oop volatile kept=fixture_word_make(UINT64_C(0xfedcba9876543210));
    api->gc_set_precise(1); (void)api->gc_collect();
    expect_text(kept,"0xfedcba9876543210");
    printf("word-module: %u equality cases, hex printing, conversion bounds, errors, raw bits, preparation GC, publication reentry, live operations, reinitialization and method replacement, GC passed\n",pairs);
}
