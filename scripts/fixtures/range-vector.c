#include "idapi_v13.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>

typedef struct { int64_t start,end; } Range;
static IdApi *api,selected;
static Oop range_type,active_range,other_vector,replacement_vector;
static Oop *active_argument;
static Method reverse_method,comma_method;
static int capture, mutate_bounds, mutate_argument, reject_second;
static unsigned allocations, integers, exacts, sends;
static int64_t offset;
static Oop (*original_integer)(void *,int64_t);
static Oop send1(Oop object,const char *name,Oop arg) { return api->send(object,api->intern(name),&arg,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static Oop forbidden(Oop self,const Oop *args,size_t count) { (void)self; (void)args; (void)count; assert(!"direct native call replaced by a send or reloaded callback"); return NULL; }
static Oop forbidden_alloc(Oop type,size_t size) { (void)type; (void)size; assert(!"reloaded table replaced the first selected table"); return NULL; }
static Oop counted_alloc(Oop type,size_t size) {
    if(capture) {
        allocations++; printf("alloc %s %zu\n",type==fixture_vector->behavior()?"Vector":"Node",size);
        assert(size==(type==fixture_vector->behavior()?40:256));
        if(mutate_bounds) { *(Range *)active_range=(Range){100,99}; mutate_bounds=0; }
        api->gc_collect();
    }
    return api->alloc(type,size);
}
static Oop counted_integer(void *context,int64_t value) {
    if(capture) {
        integers++; printf("integer %" PRId64 "\n",value);
        if(mutate_argument) { *active_argument=replacement_vector; mutate_argument=0; }
        api->gc_collect();
    }
    return original_integer(context,value+offset);
}
static Oop counted_vtable(Oop value) {
    if(capture) { exacts++; printf("vtable %s\n",value==other_vector?"original":(value==replacement_vector?"replacement":"other")); }
    if(capture && reject_second && exacts==2) return api->object_vt;
    return api->vtable_of(value);
}
static Oop counted_send(Oop self,Oop selector,const Oop *args,size_t count) {
    if(capture) sends++;
    return api->send(self,selector,args,count);
}
static void validate_dependency(void *collection,int attached) {
    RangeDependencyV4 attach=(RangeDependencyV4)api->dlsym_fn(collection,"birnam_range_dependency_v4"); assert(attach);
    RangeApiV4 bad=*fixture_range_v2; assert(!attach(NULL)); bad.version=1; assert(!attach(&bad));
    bad=*fixture_range_v2; bad.byte_size++; assert(!attach(&bad));
#define MISSING(field) bad=*fixture_range_v2; bad.field=NULL; assert(!attach(&bad))
    MISSING(prepare); MISSING(finish_prototype); MISSING(set_layout); MISSING(install); MISSING(publish);
#undef MISSING
    if(attached) { bad=*fixture_range_v2; assert(!attach(&bad) && attach(fixture_range_v2)==1); }
}
static void mount(const char *path,IdApi *table) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    if(strstr(path,"collection") && !fixture_range_v2 && api->dlsym_fn(image,"birnam_range_dependency_v4")) {
        const char *slash=strrchr(fixture_floor_path,'/'); char library[PATH_MAX]; assert(slash);
        int n=snprintf(library,sizeof(library),"%.*s/libbirnam_range.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
        assert(n>0 && (size_t)n<sizeof(library));
        void *range=api->dlopen_fn(library,RTLD_NOW|RTLD_LOCAL); assert(range); fixture_attach_integer(api,range);
        RangeProviderV4 provide=(RangeProviderV4)api->dlsym_fn(range,"birnam_range_provider_v4"); assert(provide && !provide());
        ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(range,"module_init"); assert(init); init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration());
        fixture_range_v2=provide(); assert(fixture_range_v2); validate_dependency(image,0);
    }
    fixture_attach_integer(api,image);
    if(strstr(path,"collection") && fixture_range_v2) validate_dependency(image,1);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration());
}
static Oop vector(const int64_t *values,size_t count) {
    Oop result=fixture_vector->empty();
    for(size_t i=0;i<count;i++) result=fixture_vector->push(result,fixture_integer_make(values[i]));
    return result;
}
static void expect(Oop result,const int64_t *values,size_t count) {
    assert(result && api->vtable_of(result)==fixture_vector->behavior() && fixture_vector->count(result)==count);
    for(size_t i=0;i<count;i++) assert(fixture_integer_value(fixture_vector->at(result,i))==values[i]);
}
static void begin(const char *label) {
    allocations=integers=exacts=sends=0; capture=1; printf("case %s\n",label);
}
static void end(unsigned expected_integers,unsigned expected_exacts) {
    capture=0; assert(integers==expected_integers && exacts==expected_exacts && sends==0);
    printf("done allocations=%u integers=%u exacts=%u sends=%u\n",allocations,integers,exacts,sends);
}
int main(int argc,char **argv) {
    assert(argc==4);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api);
    selected=*api; selected.alloc=counted_alloc; selected.vtable_of=counted_vtable; selected.send=counted_send; mount(argv[3],&selected);
    range_type=fixture_namespace_import("RangeVt"); reverse_method=lookup(range_type,"reversed"); comma_method=lookup(range_type,",");
    for(unsigned i=0;i<2;i++) {
        Dl_info owner; assert(dladdr((const void *)(i?comma_method:reverse_method),&owner));
        assert(strstr(owner.dli_fname,fixture_range_v2?"/libbirnam_range.dylib":"_collection.dylib"));
    }
    active_range=api->alloc(range_type,sizeof(Range)); fixture_namespace_export("RangeVectorRoot",active_range);
    const int64_t suffix[]={41,42},replacement[]={51,52,53,54};
    other_vector=vector(suffix,2); replacement_vector=vector(replacement,4);
    fixture_namespace_export("RangeVectorSuffix",other_vector); fixture_namespace_export("RangeVectorReplacement",replacement_vector);
    original_integer=fixture_integer->make; fixture_integer->make=counted_integer;
    /* Public Vector overrides must never intercept these private operations. */
    const char *selectors[]={"new","add:","at:","size",","}; Method previous[5];
    for(size_t i=0;i<5;i++) { previous[i]=lookup(fixture_vector->behavior(),selectors[i]); api->method(fixture_vector->behavior(),api->intern(selectors[i]),forbidden); }
    const int64_t starts[]={INT64_MIN,INT64_MIN+1,-2,0,1,INT64_MAX-2,INT64_MAX,3,INT64_MAX};
    const int64_t ends[]={INT64_MIN,INT64_MIN+2,2,0,3,INT64_MAX,INT64_MAX,1,INT64_MIN};
    for(size_t c=0;c<9;c++) {
        *(Range *)active_range=(Range){starts[c],ends[c]};
        unsigned length=ends[c]<starts[c]?0:(unsigned)((__int128)ends[c]-starts[c]+1);
        int64_t expected[8]; for(unsigned i=0;i<length;i++) expected[i]=(int64_t)((__int128)ends[c]-i);
        begin("reverse"); Oop result=reverse_method(active_range,NULL,0); end(length,0); expect(result,expected,length);
        for(unsigned i=0;i<length;i++) expected[i]=(int64_t)((__int128)starts[c]+i);
        expected[length]=41; expected[length+1]=42;
        Oop argument=other_vector; begin("comma"); result=comma_method(active_range,&argument,1); end(length,2); expect(result,expected,length+2);
    }
    *(Range *)active_range=(Range){1,3};
    begin("invalid arity/carrier");
    assert(reverse_method(active_range,NULL,1)==api->nil_obj && comma_method(active_range,NULL,1)==api->nil_obj);
    assert(comma_method(active_range,&other_vector,0)==api->nil_obj && comma_method(active_range,&other_vector,2)==api->nil_obj);
    end(0,0); assert(!allocations);
    Oop invalids[]={NULL,api->nil_obj,api->true_obj,api->alloc(api->proto(fixture_vector->behavior()),40)};
    for(size_t i=0;i<4;i++) { begin("inexact rhs"); assert(comma_method(active_range,&invalids[i],1)==api->nil_obj); end(0,1); assert(!allocations); }
    int64_t reversed[]={3,2,1}; mutate_bounds=1;
    begin("bounds snapshot"); Oop result=reverse_method(active_range,NULL,0); end(3,0); expect(result,reversed,3);
    *(Range *)active_range=(Range){1,3};
    Oop argument=other_vector; active_argument=&argument; mutate_argument=1;
    const int64_t changed[]={1,2,3,51,52,53,54};
    begin("rhs carrier reentry"); result=comma_method(active_range,&argument,1); end(3,2); expect(result,changed,7); assert(argument==replacement_vector);
    argument=other_vector; reject_second=1;
    begin("second exact check"); assert(comma_method(active_range,&argument,1)==api->nil_obj); end(3,2); reject_second=0;
    offset=100; const int64_t shifted[]={103,102,101};
    begin("live Integer make"); result=reverse_method(active_range,NULL,0); end(3,0); expect(result,shifted,3); offset=0;
    Oop inherited=api->alloc(api->proto(range_type),sizeof(Range)); *(Range *)inherited=(Range){-1,1};
    const int64_t inherited_values[]={1,0,-1};
    begin("inherited Range"); result=reverse_method(inherited,NULL,0); end(3,0); expect(result,inherited_values,3);
    for(size_t i=0;i<5;i++) api->method(fixture_vector->behavior(),api->intern(selectors[i]),previous[i]);
    IdApi other=selected; other.nil_obj=active_range; other.alloc=forbidden_alloc;
    fixture_namespace_export("Range",api->nil_obj); fixture_namespace_export("RangeVt",api->nil_obj); mount(argv[3],&other);
    assert(reverse_method(active_range,NULL,1)==api->nil_obj);
    if(fixture_range_v2) {
        /* The initially supplied native comma is immutable across reinstallation. */
        fixture_range_v2->install(&other,api->intern("value:"),api->intern("value:value:"),api->intern("printString"),forbidden);
    }
    api->method(fixture_vector->behavior(),api->intern(","),forbidden);
    api->gc_set_precise(1); api->gc_collect(); api->gc_set_precise(0);
    argument=other_vector; const int64_t joined[]={1,2,3,41,42};
    begin("reload keeps native comma and table"); result=comma_method(active_range,&argument,1); end(3,2); expect(result,joined,5);
    assert(lookup(range_type,"reversed")==reverse_method && lookup(range_type,",")==comma_method);
    fixture_integer->make=original_integer;
    puts("range-vector: extreme/descending bounds, native dispatch, callback order, reentry, GC and reload passed");
}
