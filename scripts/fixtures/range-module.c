#include "idapi_v14.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>

/* Independent payload/arithmetic oracle; no Rust layout or method code. */
typedef struct { int64_t start, end; } Range;
_Static_assert(sizeof(Range)==16 && offsetof(Range,end)==8,"two RAW i64 bounds");
static IdApi *api, selected;
static Oop behavior, prototype, callback_type, callback, active_range;
static RootProvider range_roots;
static unsigned registrations, bindings, publications, collections, marks, sends, calls;
static unsigned integer_calls, string_calls, byte_calls;
static int64_t seen[8];
static const char *order[]={"size","includes:","do:","inject:into:","at:","reversed","printString",","};
static Oop send0(Oop object,const char *name) { return api->send(object,api->intern(name),NULL,0); }
static Oop send1(Oop object,const char *name,Oop value) { return api->send(object,api->intern(name),&value,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static void mount(const char *path,IdApi *table,const NamespaceExportV1 *exports) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),exports,fixture_vtable_enumeration(),fixture_method_observation());
}
static void roots(RootProvider provider) {
    registrations++;
    Dl_info owner; assert(dladdr((const void *)provider,&owner));
    if(strstr(owner.dli_fname,"/libbirnam_range.dylib")) range_roots=provider;
    api->gc_register_root_provider(provider);
}
static void record_mark(Oop object) {
    if(object==api->intern("value:") || object==api->intern("value:value:") || object==api->intern("printString")) marks++;
}
static void layout(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    if(count==2 && kinds[0]==0 && kinds[1]==0 && rest==0) behavior=type;
    api->set_layout(type,kinds,count,rest);
}
static void method(Oop type,Oop selector,Method body) {
    if(type==behavior) {
        assert(!strcmp(api->symbol_name(selector),order[bindings%8])); bindings++;
        collections++; api->gc_collect();
    }
    api->method(type,selector,body);
}
static Oop publish(void *context,const char *name,Oop value) {
    (void)context;
    if(!strcmp(name,"Range")) {
        publications++; prototype=value;
        assert(bindings%8==0 && api->vtable_of(value)==behavior);
        for(unsigned i=0;i<8;i++) assert(lookup(behavior,order[i]));
        api->gc_collect(); assert(fixture_integer_value(send0(value,"size"))==1);
    }
    return fixture_namespace_export(name,value);
}
static Oop counted_send(Oop object,Oop selector,const Oop *args,size_t count) {
    sends++; return api->send(object,selector,args,count);
}
static Oop make_range(int64_t start,int64_t end) {
    Oop args=fixture_integer_make(end);
    Oop result=api->send(fixture_integer_make(start),api->intern("to:"),&args,1);
    assert(api->vtable_of(result)==behavior && api->object_payload_size(result)==16);
    assert(((Range *)result)->start==start && ((Range *)result)->end==end);
    return result;
}
static void integer_result(Oop object,__int128 value) {
    assert(fixture_is_integer(object)); assert(fixture_integer_value(object)==(int64_t)value);
}
static void bytes_result(Oop object,const uint8_t *expected,size_t count) {
    size_t length=0; const uint8_t *bytes=fixture_string_bytes(object,&length);
    assert(api->vtable_of(object)==fixture_string->behavior && length==count && (!count || !memcmp(bytes,expected,count)));
}
static Oop next_value(Oop self,const Oop *args,size_t count) {
    assert(self==callback && count==1); seen[calls++]=fixture_integer_value(args[0]); return NULL;
}
static Oop first_value(Oop self,const Oop *args,size_t count) {
    assert(self==callback && count==1); seen[calls++]=fixture_integer_value(args[0]);
    *(Range *)active_range=(Range){-999,999};
    api->method(callback_type,api->intern("value:"),next_value); api->gc_collect(); return api->nil_obj;
}
static Oop fold(Oop self,const Oop *args,size_t count) {
    assert(self==callback && count==2); seen[calls]=fixture_integer_value(args[1]);
    if(calls==0) { assert(args[0]==api->true_obj); calls++; api->gc_collect(); return api->nil_obj; }
    if(calls==1) { assert(args[0]==api->nil_obj); calls++; return api->false_obj; }
    assert(calls==2 && args[0]==api->false_obj); calls++; return NULL;
}
static Oop (*original_make_integer)(void *,int64_t);
static Oop counted_integer(void *context,int64_t value) {
    integer_calls++; return original_make_integer(context,value);
}
static Oop (*original_make_string)(void *,const uint8_t *,size_t);
static Oop counted_string(void *context,const uint8_t *bytes,size_t length) {
    string_calls++; api->gc_collect(); return original_make_string(context,bytes,length);
}
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static const uint8_t *counted_bytes(void *context,Oop object,size_t *length) {
    byte_calls++; return original_bytes(context,object,length);
}
static const uint8_t *invalid_bytes(void *context,Oop object,size_t *length) {
    (void)context; (void)object; *length=4; return NULL;
}
static Oop print_next(Oop self,const Oop *args,size_t count) {
    (void)self; (void)args; assert(count==0); calls++; return fixture_string_make((const uint8_t *)"B",1);
}
static Oop print_first(Oop self,const Oop *args,size_t count) {
    (void)self; (void)args; assert(count==0); calls++;
    api->method(fixture_integer->behavior,api->intern("printString"),print_next);
    api->gc_collect(); return fixture_string_make((const uint8_t *)"A\0",2);
}
static Oop print_null(Oop self,const Oop *args,size_t count) { (void)self; (void)args; (void)count; return NULL; }
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args; (void)count; return self; }
int main(int argc,char **argv) {
    assert(argc==4 || (argc==5 && (!strcmp(argv[4],"baseline") || !strcmp(argv[4],"methods-baseline"))));
    int baseline=argc==5 && !strcmp(argv[4],"baseline");
    int methods_baseline=argc==5 && !strcmp(argv[4],"methods-baseline");
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    mount(argv[2],api,fixture_namespace_exports());
    selected=*api; selected.set_layout=layout; selected.method=method;
    NamespaceExportV1 selected_exports={1,sizeof(NamespaceExportV1),NULL,publish}; selected.gc_register_root_provider=roots;
    mount(argv[3],&selected,&selected_exports);
    assert(bindings==8 && publications==1 && collections==8 && registrations==(unsigned)(baseline?1:((fixture_map || fixture_map_v2)?4:(fixture_vector?3:2))));
    assert(fixture_namespace_import("Range")==prototype && fixture_namespace_import("RangeVt")==behavior);
    assert(vtable_parent(api,behavior)==fixture_namespace_import("CollectionVt"));
    for(size_t i=0;i<3;i++) assert(api->slot_kind(behavior,i)==IDAPI_V14_SLOT_RAW);
    if(!baseline) {
        assert(range_roots); range_roots(record_mark); assert(marks==3);
        if(!methods_baseline && !fixture_range_v2) {
            void *collection=api->dlopen_fn(argv[3],RTLD_NOW|RTLD_LOCAL); assert(collection);
            RangeDependencyV1 attach=(RangeDependencyV1)api->dlsym_fn(collection,"birnam_range_dependency_v1"); assert(attach);
            RangeApiV1 bad=*fixture_range;
            assert(!attach(NULL)); bad.version=9; assert(!attach(&bad)); bad=*fixture_range;
            bad.byte_size--; assert(!attach(&bad)); bad=*fixture_range;
            bad.prepare=NULL; assert(!attach(&bad)); bad=*fixture_range;
            bad.finish_prototype=NULL; assert(!attach(&bad)); bad=*fixture_range;
            bad.set_layout=NULL; assert(!attach(&bad)); bad=*fixture_range;
            bad.install=NULL; assert(!attach(&bad)); bad=*fixture_range;
            bad.publish=NULL; assert(!attach(&bad)); bad=*fixture_range;
            bad.bounds=NULL; assert(!attach(&bad)); bad=*fixture_range;
            assert(!attach(&bad) && attach(fixture_range)==1);
        }
    }
    Method implementations[8]; for(unsigned i=0;i<8;i++) implementations[i]=lookup(behavior,order[i]);
    void *range_module=NULL;
    for(unsigned i=0;i<8;i++) {
        Dl_info owner; assert(dladdr((const void *)implementations[i],&owner));
        if(!baseline && ((i!=5 && i!=7) || fixture_range_v2)) {
            assert(strstr(owner.dli_fname,"/libbirnam_range.dylib"));
            range_module=api->dlopen_fn(owner.dli_fname,RTLD_NOW|RTLD_LOCAL); assert(range_module);
        } else assert(strstr(owner.dli_fname,"_collection.dylib"));
    }
    const int64_t bounds[]={INT64_MIN,INT64_MIN+1,-2,-1,0,1,2,INT64_MAX-1,INT64_MAX};
    const int64_t indices[]={INT64_MIN,-1,0,1,2,3,INT64_MAX-1,INT64_MAX};
    unsigned checked=0;
    for(size_t a=0;a<9;a++) for(size_t b=0;b<9;b++) {
        Oop r=make_range(bounds[a],bounds[b]); __int128 length=(__int128)bounds[b]-bounds[a]+1;
        Oop size=send0(r,"size");
        if(bounds[b]<bounds[a]) integer_result(size,0); else if(length>INT64_MAX) assert(size==api->nil_obj); else integer_result(size,length);
        for(size_t i=0;i<8;i++) {
            Oop index=fixture_integer_make(indices[i]), at=send1(r,"at:",index);
            __int128 value=(__int128)bounds[a]+indices[i];
            if(indices[i]<0 || bounds[a]>bounds[b] || value>bounds[b] || value>INT64_MAX) assert(at==api->nil_obj); else integer_result(at,value);
            Oop includes=send1(r,"includes:",index);
            assert(includes==((bounds[a]<=bounds[b] && indices[i]>=bounds[a] && indices[i]<=bounds[b])?api->true_obj:api->false_obj));
            checked++;
        }
        if(bounds[b]<bounds[a]) bytes_result(send0(r,"printString"),(const uint8_t *)"#[]",3);
        else if(length<=3) {
            char expected[128]; size_t n=0; expected[n++]='#'; expected[n++]='[';
            for(__int128 x=bounds[a];x<=bounds[b];x++) n+=(size_t)snprintf(expected+n,sizeof(expected)-n,"%s%" PRId64,x==bounds[a]?"":" ",(int64_t)x);
            expected[n++]=']'; bytes_result(send0(r,"printString"),(const uint8_t *)expected,n);
        }
    }
    Oop r=make_range(1,3); active_range=r; fixture_namespace_export("RangeTestRoot",r);
    assert(implementations[0](r,NULL,1)==api->nil_obj && implementations[6](r,NULL,1)==api->nil_obj);
    assert(implementations[1](r,NULL,1)==api->nil_obj && implementations[4](r,NULL,1)==api->nil_obj);
    assert(implementations[2](r,NULL,1)==api->nil_obj && implementations[3](r,NULL,2)==api->nil_obj);
    assert(implementations[1](r,NULL,2)==api->nil_obj && implementations[2](r,NULL,0)==api->nil_obj);
    assert(implementations[3](r,NULL,1)==api->nil_obj && implementations[4](r,NULL,0)==api->nil_obj);
    assert(send1(r,"includes:",api->true_obj)==api->false_obj && send1(r,"at:",api->true_obj)==api->nil_obj);
    Oop child=api->proto(behavior), inherited=api->alloc(child,sizeof(Range)); *(Range *)inherited=(Range){2,4};
    integer_result(send0(inherited,"size"),3); integer_result(send1(inherited,"at:",fixture_integer_make(1)),3);
    callback_type=api->proto(api->object_vt); callback=api->alloc(callback_type,0); fixture_namespace_export("RangeTestCallback",callback);
    api->method(callback_type,api->intern("value:"),first_value); selected.send=counted_send;
    assert(send1(r,"do:",callback)==r && calls==3 && sends==3 && seen[0]==1 && seen[1]==2 && seen[2]==3);
    *(Range *)r=(Range){1,5}; calls=sends=0; api->method(callback_type,api->intern("value:value:"),fold);
    Oop fold_args[]={api->true_obj,callback}; assert(implementations[3](r,fold_args,2)==api->nil_obj);
    assert(calls==3 && sends==3 && seen[0]==1 && seen[2]==3);
    *(Range *)r=(Range){3,1}; calls=sends=0;
    assert(implementations[3](r,fold_args,2)==api->true_obj && send1(r,"do:",callback)==r && !calls && !sends);
    *(Range *)r=(Range){1,3};
    Method original_print=lookup(fixture_integer->behavior,"printString");
    api->method(fixture_integer->behavior,api->intern("printString"),print_first); calls=sends=0;
    const uint8_t printed[]={'#','[','A',0,' ','B',' ','B',']'};
    bytes_result(send0(r,"printString"),printed,sizeof(printed)); assert(calls==3 && sends==3);
    api->method(fixture_integer->behavior,api->intern("printString"),print_null);
    assert(send0(r,"printString")==api->nil_obj);
    api->method(fixture_integer->behavior,api->intern("printString"),original_print); selected.send=api->send;
    original_bytes=fixture_string->bytes; fixture_string->bytes=invalid_bytes;
    assert(send0(r,"printString")==api->nil_obj); fixture_string->bytes=counted_bytes;
    original_make_string=fixture_string->make; fixture_string->make=counted_string;
    bytes_result(send0(r,"printString"),(const uint8_t *)"#[1 2 3]",8);
    /* Three Integer prints plus one Range print; assertion reads bytes once. */
    assert(string_calls==4 && byte_calls==4); fixture_string->make=original_make_string; fixture_string->bytes=original_bytes;
    original_make_integer=fixture_integer->make; fixture_integer->make=counted_integer;
    integer_result(send0(r,"size"),3); assert(integer_calls==1); fixture_integer->make=original_make_integer;
    selected.true_obj=r; assert(send1(r,"includes:",fixture_integer_make(2))==r); selected.true_obj=api->true_obj;
    api->method(behavior,api->intern("size"),identity); assert(send0(r,"size")==r);
    if(!baseline) {
        ModuleInitFn prepare=(ModuleInitFn)api->dlsym_fn(range_module,"module_init"); assert(prepare);
        prepare(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation()); assert(send0(r,"size")==r && bindings==8);
    }
    fixture_namespace_export("Range",api->nil_obj); fixture_namespace_export("RangeVt",api->nil_obj);
    mount(argv[3],&selected,&selected_exports); assert(bindings==16 && publications==1 && registrations==(unsigned)(baseline?1:((fixture_map || fixture_map_v2)?4:(fixture_vector?3:2))));
    assert(fixture_namespace_import("Range")==api->nil_obj && fixture_namespace_import("RangeVt")==api->nil_obj);
    for(unsigned i=0;i<8;i++) assert(lookup(behavior,order[i])==implementations[i]);
    api->gc_set_precise(1); api->gc_collect(); integer_result(send0(r,"size"),3);
    if(!baseline) { marks=0; range_roots(record_mark); assert(marks==3); }
    fixture_namespace_export("Vector",NULL); mount(argv[3],&selected,&selected_exports);
    assert(bindings==24 && publications==2 && fixture_namespace_import("Range")==prototype && fixture_namespace_import("RangeVt")==behavior);
    Oop reversed=send0(r,"reversed"); integer_result(send1(reversed,"at:",fixture_integer_make(0)),3);
    Oop combined=send1(r,",",reversed); integer_result(send0(combined,"size"),6);
    printf("range-module: %u boundary/index cases, ordered installation, publication reentry, native iteration, live callbacks, GC, roots and reinitialization passed%s\n",checked,baseline?" (Collection baseline)":(methods_baseline?" (methods baseline)":""));
}
