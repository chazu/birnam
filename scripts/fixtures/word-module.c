#include "idapi_v5.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>

static IdApi *api;
static IdApi word_api;
static size_t reads, boxes;
static Oop (*original_make)(void *, int64_t);
static uint64_t counted_read(Oop value) { ++reads; return api->word_value(value); }
static Oop counted_box(void *context, int64_t value) { ++boxes; return original_make(context,value); }
static void mount(const char *path, IdApi *selected) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    init(selected,NULL);
}
static Oop send0(Oop receiver, const char *selector) {
    return api->send(receiver,api->intern(selector),NULL,0);
}
static Oop send(Oop receiver, const char *selector, Oop arg) {
    return api->send(receiver,api->intern(selector),&arg,1);
}
static Method lookup(const char *selector) {
    return (Method)send(api->word_vt,"lookup",api->intern(selector));
}
static void expect_text(Oop receiver, const char *expected) {
    Oop value=send0(receiver,"printString"); size_t count=0;
    const uint8_t *bytes=api->string_bytes(value,&count);
    assert(count==strlen(expected) && !memcmp(bytes,expected,count));
}
static Oop right(Oop receiver, const Oop *args, size_t count) {
    (void)receiver; assert(count==1); return args[0];
}
int main(int argc, char **argv) {
    assert(argc==6);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)dlsym(floor,"id_init"))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1]);
    assert(!lookup("asInteger"));
    word_api=*api; mount(argv[2],&word_api);
    Method convert=lookup("asInteger"), equal=lookup("="), print=lookup("printString");
    assert(convert && equal && print);
    Oop high=api->mk_word(UINT64_MAX);
    assert(send0(high,"asInteger")==api->nil_obj); /* lazy Error lookup */
    mount(argv[3],api); mount(argv[5],api); /* object-model and Error */
    const uint64_t inputs[]={0,1,17,UINT64_C(0xffffffff),UINT64_C(0x100000000),
        INT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0xfedcba9876543210),UINT64_MAX};
    unsigned pairs=0;
    for(size_t i=0;i<sizeof(inputs)/sizeof(*inputs);i++) {
        Oop word=api->mk_word(inputs[i]);
        char expected[32]; snprintf(expected,sizeof(expected),"0x%" PRIx64,inputs[i]);
        expect_text(word,expected);
        Oop value=send0(word,"asInteger");
        if(inputs[i]<=INT64_MAX) {
            assert(fixture_is_integer(value) && fixture_integer_value(value)==(int64_t)inputs[i]);
            assert(send0(word,"asInteger")!=value);
        } else {
            assert(api->vtable_of(value)==api->import_fn("ErrorVt"));
            assert(send0(value,"kind")==api->intern("overflow"));
        }
        for(size_t j=0;j<sizeof(inputs)/sizeof(*inputs);j++) {
            assert(send(word,"=",api->mk_word(inputs[j]))==(inputs[i]==inputs[j]?api->true_obj:api->false_obj));
            ++pairs;
        }
        assert(send(word,"=",fixture_integer_make((int64_t)inputs[i]))==api->false_obj);
        assert(convert(word,NULL,1)==api->nil_obj && print(word,NULL,1)==api->nil_obj);
        assert(equal(word,NULL,0)==api->nil_obj && equal(word,NULL,1)==api->nil_obj);
    }
    assert(convert(api->true_obj,NULL,0)==api->nil_obj);
    assert(print(api->true_obj,NULL,0)==api->nil_obj);
    assert(equal(api->true_obj,&high,1)==api->nil_obj);
    Oop seven=api->mk_word(7);
    word_api.word_value=counted_read;
    original_make=fixture_integer->make; fixture_integer->make=counted_box;
    assert(fixture_integer_value(send0(seven,"asInteger"))==7 && boxes==1 && reads==1);
    fixture_integer->make=original_make; word_api.word_value=api->word_value;
    api->method(api->word_vt,api->intern("="),right);
    assert(send(seven,"=",high)==high);
    mount(argv[4],api); /* Prelude preserves the installed Word replacement. */
    assert(lookup("=")==right && send(seven,"=",high)==high);
    api->method(api->word_vt,api->intern("="),equal);
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    Oop volatile kept=api->mk_word(UINT64_C(0xfedcba9876543210));
    api->gc_set_precise(1); (void)api->gc_collect();
    expect_text(kept,"0xfedcba9876543210");
    printf("word-module: %u equality cases, hex printing, conversion bounds, errors, live accessor/constructor and method replacement, GC passed\n",pairs);
}
