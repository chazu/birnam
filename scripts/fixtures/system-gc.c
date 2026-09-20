#include "idapi_v17.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>
static FixtureRootSlot watched_root_slot;
static RootRegistryV1 watched_roots=FIXTURE_ROOTS(watched_root_slot);
static FixtureRootSlot selected_root_slot;
static RootRegistryV1 selected_roots=FIXTURE_ROOTS(selected_root_slot);

static FixtureSettingsSlots selected_settings_slots,other_settings_slots;
static GcSettingsV1 selected_settings=FIXTURE_SETTINGS(selected_settings_slots);
static GcSettingsV1 other_settings=FIXTURE_SETTINGS(other_settings_slots);
static IdApi *api, selected, other;
static ModuleInitFn prelude_init;
static Method controls[4];
static const char *names[4]={"gc","liveObjects","autoGc:","preciseGc:"};
static Oop subject,result,alias,alternate,system_type,system_object,expected_target;
static Oop selectors[2][4],seen_selectors[4],seen_targets[4];
static size_t bindings,registrations,queries,constructions,cases,query_value;
static int capture,reentry_mode,reentered,map_selectors,map_generation,change_snapshot;
static int last_auto,last_precise,make_mode,replace_make,nested,make_nested,collect,mutate_setter;
static Oop (*original_make)(void *,int64_t);
static void *original_context;
static void *changed_context=(void *)(uintptr_t)0x1234;
static int64_t observed;
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static void roots(MarkFn mark) {
    mark(subject);mark(result);mark(alias);mark(alternate);mark(system_type);mark(system_object);mark(expected_target);
    for(size_t i=0;i<2;i++) for(size_t j=0;j<4;j++) mark(selectors[i][j]);
}
static void registered(RootProvider provider) { registrations++;fixture_register_root(provider); }
static Oop allocate(Oop type,size_t size) { if(capture) printf("alloc %zu\n",size);return api->alloc(type,size); }
static int control_index(Oop selector) {
    for(size_t i=0;i<4;i++) if(selector==api->intern(names[i]) || (selectors[0][i] && (selector==selectors[0][i] || selector==selectors[1][i]))) return (int)i;
    return -1;
}
static void reenter(void) { reentry_mode=0;reentered=1;prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,&other_settings,fixture_payload_layout()); }
static Oop intern(const char *name) {
    if(capture) printf("intern %s\n",name);
    if(reentry_mode==1 && !strcmp(name,"gc")) reenter();
    if(map_selectors) for(size_t i=0;i<4;i++) if(!strcmp(name,names[i])) return selectors[map_generation][i];
    return api->intern(name);
}
static void method(Oop type,Oop selector,Method implementation) {
    const char *name=api->symbol_name(selector);
    if(capture) printf("method %s\n",name);
    int index=control_index(selector);
    if(index>=0) {
        bindings++;seen_selectors[index]=selector;seen_targets[index]=type;
        assert(implementation(NULL,NULL,2)==api->nil_obj);
        if(expected_target) assert(type==expected_target);
    }
    if(!strcmp(name,"readLine")) {
        if(reentry_mode==2) reenter();
        if(change_snapshot) { change_snapshot=0;map_generation=1;fixture_namespace_export("SystemVt",alternate); }
    }
    api->method(type,selector,implementation);
}
static Oop make(void *context,int64_t value) {
    assert(context==original_context);constructions++;observed=value;
    if(capture) printf("make %" PRId64 "\n",value);
    if(make_nested) { make_nested=0;assert(controls[2](subject,&selected.true_obj,1)==subject); }
    if(collect) api->gc_collect();
    if(make_mode==1) return NULL;
    if(make_mode==2) return alias;
    return original_make(context,value);
}
static Oop changed_make(void *context,int64_t value) {
    assert(context==changed_context);constructions++;observed=value;
    if(capture) printf("changed make %" PRId64 "\n",value);
    return alias;
}
static size_t query(int kind) {
    queries++;size_t value=query_value;if(capture) printf("query %d %zu\n",kind,value);
    if(collect) api->gc_collect();
    if(nested) {
        nested=0;query_value=17;Oop inside=controls[1](subject,NULL,0);assert(fixture_integer_value(inside)==17);query_value=value;
    }
    if(replace_make) { fixture_integer->make=changed_make;fixture_integer->context=changed_context; }
    return value;
}
static size_t gc(void) { return query(0); }
static size_t live(void) { return query(1); }
static void auto_gc(int enabled) {
    last_auto=enabled;if(capture) printf("auto %d\n",enabled);
    if(collect) api->gc_collect();
    if(mutate_setter) { selected.true_obj=api->nil_obj;selected.false_obj=NULL; }
}
static void precise_gc(int enabled) { last_precise=enabled;if(capture) printf("precise %d\n",enabled);if(collect) api->gc_collect(); }
static size_t forbidden_query(void) { assert(!"registration table became execution table");return 0; }
static void forbidden_setter(int enabled) { (void)enabled;assert(!"registration table became execution table"); }
static Oop replacement(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return alias; }
static void begin(const char *name) { printf("case %s\n",name);capture=1;queries=constructions=0;last_auto=last_precise=-1; }
static void end(void) { capture=0;printf("done query=%zu make=%zu auto=%d precise=%d\n",queries,constructions,last_auto,last_precise); }
static void validate(void *image,int attached) {
    SystemGcDependencyV2 attach=(SystemGcDependencyV2)api->dlsym_fn(image,"birnam_system_gc_dependency_v2");if(!attach) return;
    SystemGcApiV2 bad=*fixture_system_gc;
    assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_system_gc;
    bad.byte_size--;assert(!attach(&bad));bad=*fixture_system_gc;bad.install=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_system_gc;assert(!attach(&bad) && attach(fixture_system_gc)==1); }
}
static void mount(const char *path,IdApi *table,int prelude,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_system_gc_dependency_v2") && !fixture_system_gc) {
        IdApi watched=*api;watched.alloc=allocate;watched.method=method;watched_root_slot=registered;
        fixture_prepare_system_gc(&watched,&watched_roots);assert(!bindings && !registrations && !fixture_namespace_contains("System"));validate(image,0);
    }
    fixture_attach_integer(api,image,fixture_root_registry());ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
    if(prelude) { validate(image,1);prelude_init=init; } init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,settings,layouts);
}
int main(int argc,char **argv) {
    assert(argc==5);reentry_mode=!strcmp(argv[4],"first-gc-intern-reentry")?1:(!strcmp(argv[4],"first-readline-binding-reentry")?2:0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);fixture_set_auto(0);
    fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,0,fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout());fixture_register_root(roots);
    selected=*api;selected.intern=intern;selected.method=method;selected.alloc=allocate;selected_root_slot=registered;
    selected.gc_collect=gc;selected.gc_live_count=live;selected_settings_slots.automatic=auto_gc;selected_settings_slots.precise=precise_gc;
    other=selected;other.nil_obj=api->true_obj;other.gc_collect=other.gc_live_count=forbidden_query;other_settings_slots.automatic=other_settings_slots.precise=forbidden_setter;
    begin("initial registration");mount(argv[3],&selected,1,&selected_roots,&selected_settings,fixture_payload_layout());end();assert(bindings==(reentered?8u:4u) && !registrations);
    system_type=fixture_namespace_import("SystemVt");system_object=fixture_namespace_import("System");assert(system_type && system_object);
    for(size_t i=0;i<4;i++) {
        controls[i]=lookup(system_type,names[i]);Dl_info owner;assert(controls[i] && dladdr((const void *)controls[i],&owner));
        assert(strstr(owner.dli_fname,fixture_system_gc?"/libbirnam_system_gc.dylib":"_prelude.dylib"));
    }
    alias=api->alloc(api->object_vt,0);alternate=api->alloc(api->object_vt,0);
    original_make=fixture_integer->make;original_context=fixture_integer->context;fixture_integer->make=make;
    subject=system_object;
    begin("invalid arity and carriers");
    for(size_t i=0;i<4;i++) {
        for(size_t count=2;count<=3;count++) assert(controls[i](subject,(Oop *)(uintptr_t)1,count)==api->nil_obj);
        if(i<2) assert(controls[i](subject,NULL,1)==api->nil_obj);
        else assert(controls[i](subject,NULL,1)==api->nil_obj && controls[i](subject,(Oop *)(uintptr_t)1,0)==api->nil_obj);
    }
    end();assert(!queries && !constructions && last_auto==-1 && last_precise==-1);
    begin("live nil identity");selected.nil_obj=alias;for(size_t i=0;i<4;i++) assert(controls[i](subject,NULL,2)==alias);selected.nil_obj=api->nil_obj;end();
    const size_t counts[]={0,1,31,INT64_MAX,(size_t)INT64_MAX+1,SIZE_MAX};
    Oop receivers[]={NULL,api->nil_obj,alias,system_object};
    for(size_t i=0;i<6;i++) for(size_t r=0;r<4;r++) for(size_t op=0;op<2;op++) {
        query_value=counts[i];subject=receivers[r];queries=constructions=0;
        result=controls[op](subject,(Oop *)(uintptr_t)1,0);
        int64_t expected;memcpy(&expected,&counts[i],sizeof(expected));
        assert(queries==1 && constructions==1 && observed==expected && fixture_integer_value(result)==expected);cases++;
    }
    Oop values[]={NULL,api->nil_obj,api->true_obj,api->false_obj,alias,alternate,original_make(original_context,0),api->alloc(api->vtable_of(api->true_obj),0)};
    const unsigned identities[][2]={{2,3},{4,5},{4,4},{0,4},{4,0},{0,0},{1,1}};
    const int answers[][8]={{-1,-1,1,0,-1,-1,-1,-1},{-1,-1,-1,-1,1,0,-1,-1},{-1,-1,-1,-1,1,-1,-1,-1},{1,-1,-1,-1,0,-1,-1,-1},{0,-1,-1,-1,1,-1,-1,-1},{1,-1,-1,-1,-1,-1,-1,-1},{-1,1,-1,-1,-1,-1,-1,-1}};
    for(size_t row=0;row<7;row++) for(size_t r=0;r<4;r++) for(size_t v=0;v<8;v++) for(size_t op=2;op<4;op++) {
        selected.true_obj=values[identities[row][0]];selected.false_obj=values[identities[row][1]];subject=receivers[r];last_auto=last_precise=-1;
        result=controls[op](subject,&values[v],1);
        assert(result==(answers[row][v]<0?api->nil_obj:subject));
        assert((op==2?last_auto:last_precise)==answers[row][v] && (op==2?last_precise:last_auto)==-1);cases++;
    }
    assert(cases==496);selected.true_obj=api->true_obj;selected.false_obj=api->false_obj;subject=system_object;
    for(make_mode=1;make_mode<=2;make_mode++) { begin("live constructor returns null or alias");query_value=SIZE_MAX;assert(controls[0](NULL,NULL,0)==(make_mode==1?NULL:alias));assert(observed==-1 && queries==1 && constructions==1);end(); }make_mode=0;
    replace_make=1;begin("constructor and context selected after query");query_value=31;assert(controls[0](subject,NULL,0)==alias && observed==31);end();replace_make=0;fixture_integer->make=make;fixture_integer->context=original_context;
    mutate_setter=1;begin("setter keeps receiver while callback changes identities");assert(controls[2](alias,&selected.true_obj,1)==alias && last_auto==1);end();mutate_setter=0;
    begin("subsequent setter reads changed identities");assert(controls[3](NULL,(Oop[]){NULL},1)==NULL && last_precise==0);end();selected.true_obj=api->true_obj;selected.false_obj=api->false_obj;
    nested=make_nested=collect=1;begin("nested controls and callback GC");query_value=31;result=controls[0](subject,NULL,0);end();collect=0;assert(fixture_integer_value(result)==31 && queries==2 && constructions==2 && last_auto==1);
    for(size_t i=0;i<4;i++) api->method(system_type,api->intern(names[i]),replacement);
    begin("copied methods survive public replacement");query_value=17;result=controls[1](subject,NULL,0);assert(fixture_integer_value(result)==17);assert(controls[3](alias,&selected.false_obj,1)==alias);end();
    begin("reload retains original execution table");prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,&other_settings,fixture_payload_layout());end();for(size_t i=0;i<4;i++) assert(lookup(system_type,names[i])==controls[i]);
    if(fixture_system_gc) fixture_system_gc->install(&other,&other,&other_settings,system_type,api->intern(names[0]),api->intern(names[1]),api->intern(names[2]),api->intern(names[3]));
    begin("retained execution after reload");query_value=31;result=controls[0](subject,NULL,0);assert(fixture_integer_value(result)==31);end();
    for(size_t g=0;g<2;g++) for(size_t i=0;i<4;i++) { char name[40];snprintf(name,sizeof(name),"gc-test-%zu-%zu",g,i);selectors[g][i]=api->intern(name); }
    alternate=api->proto(api->object_vt);expected_target=system_type;map_selectors=change_snapshot=1;map_generation=0;
    begin("selected selectors and behavior precede earlier binding callback");prelude_init(&selected,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,&selected_settings,fixture_payload_layout());end();
    for(size_t i=0;i<4;i++) { assert(seen_selectors[i]==selectors[0][i] && seen_targets[i]==system_type);assert((Method)send1(system_type,"lookup",selectors[0][i])==controls[i]); }
    expected_target=alternate;begin("next installation adopts current behavior and selectors");prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,&other_settings,fixture_payload_layout());end();
    for(size_t i=0;i<4;i++) assert(seen_selectors[i]==selectors[1][i] && seen_targets[i]==alternate);
    fixture_namespace_export("SystemVt",system_type);map_selectors=0;expected_target=NULL;
    assert(!registrations);fixture_set_precise(1);api->gc_collect();fixture_set_precise(0);assert(fixture_integer_value(result)==31);
    fixture_integer->make=original_make;fixture_integer->context=original_context;
    printf("system-gc: %zu count/identity/receiver cases, callback ordering, live constructor, reentry, snapshots, GC and reload passed mode=%s\n",cases,argv[4]);
}
