#include "idapi_v12.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"

static IdApi *api;
static Oop (*original_string)(void *, const uint8_t *, size_t);
static IdApi nil_api;
static unsigned strings;
static void mount(const char *path, IdApi *selected) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    init(selected,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration());
}
static Oop send0(Oop receiver, const char *name) {
    return api->send(receiver,api->intern(name),NULL,0);
}
static Method lookup(Oop behavior, const char *name) {
    Oop selector=api->intern(name);
    return (Method)api->send(behavior,api->intern("lookup"),&selector,1);
}
static Oop counted_string(void *context, const uint8_t *bytes, size_t count) {
    ++strings; return original_string(context,bytes,count);
}
static void expect_text(Oop string) {
    size_t length=0; const uint8_t *bytes=fixture_string_bytes(string,&length);
    assert(length==3 && !memcmp(bytes,"nil",length));
}
static Oop identity(Oop self, const Oop *args, size_t count) {
    (void)args; assert(count==0); return self;
}
int main(int argc, char **argv) {
    assert(argc==5 || (argc==6 && !strcmp(argv[5],"baseline")));
    int baseline=argc==6;
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    mount(argv[2],api); /* object-model for Collection in the baseline/later check */
    Oop nil_vt=api->vtable_of(api->nil_obj);
    assert(!lookup(api->object_vt,"isNil") && !lookup(nil_vt,"printString"));
    nil_api=*api; mount(argv[3],&nil_api);
    Method methods[]={lookup(api->object_vt,"isNil"),lookup(api->object_vt,"notNil"),
        lookup(nil_vt,"isNil"),lookup(nil_vt,"notNil"),lookup(nil_vt,"printString")};
    Oop ordinary=api->alloc(api->object_vt,0), inherited=api->alloc(nil_vt,0);
    fixture_namespace_export("NilTestOrdinary",ordinary); fixture_namespace_export("NilTestInherited",inherited);
    Oop receivers[]={api->nil_obj,api->true_obj,api->false_obj,ordinary,inherited,NULL};
    Oop expected[]={api->false_obj,api->true_obj,api->true_obj,api->false_obj};
    unsigned checked=0;
    for(unsigned i=0;i<5;i++) {
        assert(methods[i]);
        for(unsigned j=0;j<6;j++) {
            Oop result=methods[i](receivers[j],NULL,0);
            if(i<4) assert(result==expected[i]); else expect_text(result);
            assert(methods[i](receivers[j],NULL,1)==api->nil_obj);
            assert(methods[i](receivers[j],receivers,2)==api->nil_obj);
            checked++;
        }
    }
    assert(send0(api->nil_obj,"isNil")==api->true_obj && send0(api->nil_obj,"notNil")==api->false_obj);
    assert(send0(ordinary,"isNil")==api->false_obj && send0(ordinary,"notNil")==api->true_obj);
    assert(send0(inherited,"isNil")==api->true_obj && send0(inherited,"notNil")==api->false_obj);
    Oop first=send0(api->nil_obj,"printString"), second=send0(api->nil_obj,"printString");
    assert(first!=second); expect_text(first); expect_text(second);
    original_string=fixture_string->make; fixture_string->make=counted_string; expect_text(send0(api->nil_obj,"printString")); assert(strings==1);
    fixture_string->make=original_string;
    nil_api.true_obj=ordinary; assert(send0(api->nil_obj,"isNil")==ordinary); nil_api.true_obj=api->true_obj;
    nil_api.false_obj=ordinary; assert(send0(api->nil_obj,"notNil")==ordinary); nil_api.false_obj=api->false_obj;
    api->method(nil_vt,api->intern("isNil"),identity);
    assert(send0(api->nil_obj,"isNil")==api->nil_obj);
    fixture_namespace_export("nil",ordinary);
    mount(argv[3],&nil_api);
    assert(lookup(nil_vt,"isNil")==methods[2] && api->vtable_of(api->nil_obj)==nil_vt);
    assert(fixture_namespace_import("nil")==ordinary); fixture_namespace_export("nil",api->nil_obj);
    api->method(nil_vt,api->intern("isNil"),identity);
    api->method(api->object_vt,api->intern("notNil"),identity);
    if(!baseline) {
        mount(argv[4],api); /* Later Collection leaves Object/nil replacements alone. */
        assert(lookup(nil_vt,"isNil")==identity && lookup(api->object_vt,"notNil")==identity);
        assert(send0(api->nil_obj,"isNil")==api->nil_obj && send0(ordinary,"notNil")==ordinary);
    }
    mount(argv[3],&nil_api);
    api->gc_set_precise(1); (void)api->gc_collect();
    assert(send0(api->nil_obj,"isNil")==api->true_obj && send0(ordinary,"notNil")==api->true_obj);
    expect_text(send0(inherited,"printString"));
    printf("nil-module: %u method/receiver cases, inherited behavior, arity, fresh Strings, live callbacks, reinitialization, replacement and GC passed%s\n",checked,baseline?" (Collection baseline)":"");
}
