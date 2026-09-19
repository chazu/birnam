/* Independent ordered-binding oracle, exercised through the loaded floor. */
#include "idapi_v9.h"
#include "namespace-support.h"
#include "trace-support.h"
#include <stdlib.h>

static const NamespaceApiV5 *storage;
static NamespaceApiV5 observed;
static size_t creates, binds, finds, reverses, views, counts, names_at, lookups;
static void *floor_context;
static void *create(void) { creates++; return floor_context=storage->create(); }
static Oop bind(void *context,Oop name,Oop value) { assert(context==floor_context);binds++;return storage->bind(context,name,value); }
static Oop find(const void *context,Oop name) { assert(context==floor_context);finds++;return storage->find(context,name); }
static Oop reverse(const void *context,Oop value) { assert(context==floor_context);reverses++;return storage->reverse(context,value); }
static NamespaceViewV1 bindings(const void *context) { assert(context==floor_context);views++;return storage->bindings(context); }
static size_t selected_count(const void *context) { assert(context==floor_context);counts++;return storage->count(context); }
static Oop selected_name_at(const void *context,size_t index) { assert(context==floor_context);names_at++;return storage->name_at(context,index); }
static int selected_lookup(const void *context,Oop (*selected_find)(const void *,Oop),void *interner,Oop (*intern)(void *,const uint8_t *,size_t),const uint8_t *bytes,size_t length,Oop *result) {
    assert(context==floor_context && selected_find==find && interner && intern && result);
    lookups++;return storage->lookup(context,selected_find,interner,intern,bytes,length,result);
}
static Oop forbidden_intern(const char *name) { (void)name;abort(); }
static Oop forbidden_alloc(Oop type,size_t size) { (void)type;(void)size;abort(); }
static void forbidden_method(Oop type,Oop selector,Method method) { (void)type;(void)selector;(void)method;abort(); }
static void forbidden_roots(RootProvider roots) { (void)roots;abort(); }

static void lookup_oracle(void) {
    uintptr_t identities[3]={0};
    void *context=storage->create();assert(context);
    /* The real interner's invalid-name policy is separately tested through Id. */
    extern Oop lookup_probe_intern(void *,const uint8_t *,size_t);
    Oop symbol=(Oop)&identities[0],value=(Oop)&identities[1],sentinel=(Oop)&identities[2];
    storage->bind(context,symbol,value);
    Oop output=sentinel;
    assert(storage->lookup(context,storage->find,&symbol,lookup_probe_intern,(const uint8_t *)"raw\xff",4,&output)==1 && output==value);
    storage->bind(context,symbol,NULL);
    assert(storage->lookup(context,storage->find,&symbol,lookup_probe_intern,(const uint8_t *)"raw\xff",4,&output)==1 && !output);
    symbol=NULL;output=sentinel;
    assert(storage->lookup(context,storage->find,&symbol,lookup_probe_intern,(const uint8_t *)"raw\xff",4,&output)==0 && output==sentinel);
    storage->destroy(context);
}
Oop lookup_probe_intern(void *context,const uint8_t *bytes,size_t length) {
    assert(length==4 && !memcmp(bytes,"raw\xff",4));return *(Oop *)context;
}

__attribute__((noinline)) static void collect_now(IdApi *api) {
    uintptr_t anchor=0;
    api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    assert(api->gc_collect()>0);
}
static void storage_oracle(void) {
    uintptr_t objects[80]={0};
    Oop names[32],values[16];
    for(size_t i=0;i<32;i++) names[i]=i?(Oop)&objects[i]:NULL;
    for(size_t i=0;i<16;i++) values[i]=i?(Oop)&objects[i+32]:NULL;
    void *left=storage->create(),*right=storage->create();assert(left && right && left!=right);
    Oop keys[32],current[32];size_t count=0;
    for(size_t step=0;step<4096;step++) {
        Oop name=names[(step*7)%32],value=values[(step/11)%16];size_t slot=0;
        while(slot<count && keys[slot]!=name)slot++;
        if(slot==count)keys[count++]=name;
        current[slot]=value;
        assert(storage->bind(left,name,value)==value);
        assert(storage->count(left)==count && storage->count(right)==0);
        NamespaceViewV1 view=storage->bindings(left);assert(view.count==count && view.bindings);
        for(size_t i=0;i<count;i++) {
            assert(storage->name_at(left,i)==keys[i] && storage->find(left,keys[i])==current[i]);
            assert(view.bindings[i].name==keys[i] && view.bindings[i].value==current[i]);
            assert(!storage->find(right,keys[i]));
        }
        assert(!storage->name_at(left,count) && !storage->name_at(left,SIZE_MAX));
        for(size_t i=0;i<16;i++) {
            Oop expected=NULL;
            if(values[i])for(size_t j=0;j<count;j++)if(current[j]==values[i]){expected=keys[j];break;}
            assert(storage->reverse(left,values[i])==expected);
        }
    }
    storage->bind(right,names[1],values[1]);assert(storage->count(right)==1 && storage->count(left)==32);
    storage->destroy(left);assert(storage->find(right,names[1])==values[1]);storage->destroy(right);
}

int main(int argc,char **argv) {
    assert(argc==2 || argc==3);setvbuf(stdout,NULL,_IONBF,0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_namespace_floor=floor;
    NamespaceDependencyV5 attach=(NamespaceDependencyV5)dlsym(floor,"birnam_namespace_dependency_v5");
    assert(argc==2 || attach);
    void *namespace_image=NULL,*extra=NULL;ModuleInitFn reload=NULL;
    if(attach) {
        const char *slash=strrchr(argv[1],'/');assert(slash);char path[PATH_MAX];
        snprintf(path,sizeof(path),"%.*s/libbirnam_namespace.dylib",(int)(slash-argv[1]),argv[1]);
        namespace_image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(namespace_image);
        extra=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(extra);
        NamespaceProviderV5 provide=(NamespaceProviderV5)dlsym(namespace_image,"birnam_namespace_provider_v5");assert(provide);
        storage=provide();assert(storage && storage==provide());
        assert(storage->version==5 && storage->byte_size==sizeof(*storage));
        storage_oracle();lookup_oracle();
        assert(!attach(NULL));NamespaceApiV5 bad=*storage;bad.version++;assert(!attach(&bad));
        bad=*storage;bad.byte_size--;assert(!attach(&bad));
#define REJECT(field) bad=*storage;bad.field=NULL;assert(!attach(&bad))
        REJECT(create);REJECT(destroy);REJECT(bind);REJECT(find);REJECT(reverse);REJECT(count);REJECT(name_at);REJECT(bindings);REJECT(create_names);REJECT(destroy_names);REJECT(create_enumeration);REJECT(destroy_enumeration);REJECT(lookup);REJECT(create_presence);REJECT(destroy_presence);
#undef REJECT
        observed=*storage;observed.create=create;observed.bind=bind;observed.find=find;observed.reverse=reverse;observed.bindings=bindings;observed.count=selected_count;observed.name_at=selected_name_at;observed.lookup=selected_lookup;
        assert(attach(&observed)==1 && attach(&observed)==1 && !attach(storage));
        reload=(ModuleInitFn)dlsym(namespace_image,"module_init");assert(reload);
    }
    IdInitFn initialize=(IdInitFn)dlsym(floor,"id_init");assert(initialize);
    IdApi *api=initialize();assert(api);api->gc_set_auto(0);
    if(attach)assert(creates==1 && binds>0 && floor_context);
    fixture_mount_trace(api,argv[1]);
    size_t (*count)(void)=fixture_namespace_count;
    Oop (*name_at)(size_t)=fixture_namespace_name_at;
    size_t (*symbol_count)(void)=(size_t(*)(void))dlsym(floor,"birnam_id_live_symbol_count_v1");
    int (*compaction)(int,size_t,size_t)=(int(*)(int,size_t,size_t))dlsym(floor,"birnam_id_symbol_compaction_configure_v1");
    assert(count && name_at && symbol_count && compaction && compaction(1,0,0)==1);
    size_t base=count();Oop *original=calloc(base,sizeof(Oop));assert(original);
    for(size_t i=0;i<base;i++)original[i]=name_at(i);
    size_t symbols=symbol_count();assert(!api->import_fn("namespace-absent-probe"));assert(symbol_count()==symbols+1);
    assert(!fixture_namespace_contains("namespace-second-absent"));assert(symbol_count()==symbols+2);
    /* Canonical presence/import use the primitive interner, never live slots. */
    Oop (*original_intern)(const char *)=api->intern;
    Oop (*original_import)(const char *)=api->import_fn;
    api->intern=forbidden_intern;api->import_fn=forbidden_intern;
    assert(!fixture_namespace_contains("namespace-canonical-presence"));
    assert(!original_import("namespace-canonical-import"));
    assert(symbol_count()==symbols+4);
    api->intern=original_intern;api->import_fn=original_import;
    assert(count()==base);
    char names[32][40];Oop current[32]={0},keys[32]={0},values[16];size_t order[32],used=0;
    for(size_t i=0;i<32;i++)snprintf(names[i],sizeof(names[i]),"namespace-probe-%zu",i);
    names[0][0]='\0';names[31][0]=(char)0xff;
    for(size_t i=0;i<16;i++)values[i]=i?api->alloc(api->object_vt,0):NULL;
    for(size_t step=0;step<2048;step++) {
        size_t index=(step*7)%32;Oop value=values[(step/13)%16];
        if(!keys[index]){keys[index]=api->intern(names[index]);order[used++]=index;}
        current[index]=value;assert(api->export_obj(names[index],value)==value);
        assert(count()==base+used);
        for(size_t i=0;i<used;i++) {
            size_t which=order[i];assert(name_at(base+i)==keys[which]);
            assert(api->import_fn(names[which])==current[which]);
            assert(fixture_namespace_contains(names[which])==(current[which]!=NULL));
        }
        for(size_t i=0;i<16;i++) {
            const char *expected=NULL;
            if(values[i])for(size_t j=0;j<used;j++)if(current[order[j]]==values[i]){expected=names[order[j]];break;}
            const char *actual=fixture_namespace_name_of(values[i]);assert((!actual)==(!expected));
            if(actual)assert(!strcmp(actual,expected));
        }
    }
    for(size_t i=0;i<base;i++)assert(name_at(i)==original[i]);free(original);
    assert(!name_at(count()) && !name_at(SIZE_MAX));assert(!fixture_namespace_name_of(NULL));
    Oop retained=api->alloc(api->object_vt,0),discarded=api->alloc(api->object_vt,0);
    api->export_obj("namespace-first-alias",retained);api->export_obj("namespace-second-alias",retained);
    assert(!strcmp(fixture_namespace_name_of(retained),"namespace-first-alias"));
    api->export_obj("namespace-first-alias",discarded);
    assert(!strcmp(fixture_namespace_name_of(retained),"namespace-second-alias"));
    api->export_obj("namespace-first-alias",NULL);
    Oop root_name=api->intern("namespace-second-alias"),null_name=api->intern("namespace-first-alias");
    api->gc_set_precise(1);collect_now(api);
    assert(api->vtable_of(retained)==api->object_vt && !api->vtable_of(discarded));
    assert(api->vtable_of(root_name)==api->symbol_vt && api->vtable_of(null_name)==api->symbol_vt);
    assert(api->import_fn("namespace-second-alias")==retained && !fixture_namespace_contains("namespace-first-alias"));
    if(reload) {
        IdApi probe=*api;probe.intern=forbidden_intern;probe.alloc=forbidden_alloc;probe.method=forbidden_method;probe.gc_register_root_provider=forbidden_roots;
        size_t before=count(),was_binds=binds;assert(!reload(&probe,NULL,fixture_namespace_presence()));assert(!reload(api,NULL,fixture_namespace_presence()));
        assert(count()==before && binds==was_binds && creates==1);
        assert(api->import_fn("namespace-second-alias")==retained);
        assert(finds && reverses && views && counts && names_at && lookups);
        // Descriptor/code remain mapped after relinquishing this extra open.
        assert(dlclose(extra)==0);assert(api->import_fn("namespace-second-alias")==retained);
    }
    printf("namespace: %zu initial exports; 2048 ordered binding cases, absent-name interning, null bindings, aliases, enumeration and precise GC passed\n",base);
    if(argc==3)printf("namespace provider: 4096 independent-state cases, all admission fields, selected context, rooted view and reload passed\n");
    return 0;
}
