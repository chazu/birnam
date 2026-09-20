#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/namespace-enumeration-v1/namespace_enumeration_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
#ifndef ENUMERATION_V1
#define ENUMERATION_V1 1
#endif
/* The old/new adapters differ only at discovery. The behavioral oracle below
   is shared, including its independent Vector payload reader. */
#if ENUMERATION_V1
#define COUNT_NAME "birnam_namespace_count_v1"
#define NAME_AT_NAME "birnam_namespace_name_at_v1"
static NamespaceCountV1 real_count;
static NamespaceNameAtV1 real_name;
#else
#define COUNT_NAME "birnam_id_export_count_v1"
#define NAME_AT_NAME "birnam_id_export_name_at_v1"
static size_t (*real_count)(void);
static Oop (*real_name)(size_t);
#endif
static GcSettingsV1 *gc_settings;
static GcSettingsV1 saved_settings;
static IdApi *api,saved;
static NamespaceImportV1 *imports,saved_imports;
static Oop reflect,rows[5];
static Method enumerate;
static int mode,generation,nested;
static char trace[65536];static size_t used;
static void event(const char *text) { size_t n=strlen(text);assert(used+n<sizeof(trace));memcpy(trace+used,text,n+1);used+=n; }
typedef struct { size_t count,shift;Oop root,tail;size_t tail_len; } VectorPayload;
static Oop item(Oop object,size_t index) {
    VectorPayload *v=object;assert(index<v->count);Oop node=v->tail;
    if(index<v->count-v->tail_len) {
        node=v->root;
        for(size_t level=v->shift;level;level-=5)node=((Oop *)node)[(index>>level)&31];
    }
    return ((Oop *)node)[index&31];
}
static size_t call_count(int which) {
    event(which?"C1;":"C0;");
    if(mode==0) {
#if ENUMERATION_V1
        return real_count.count(real_count.context);
#else
        return real_count();
#endif
    }
    if(mode==10)generation=1;
    return which?2:5;
}
static Oop call_name(int which,size_t index) {
    char buffer[48];snprintf(buffer,sizeof(buffer),"N%d:%zu;",which,index);event(buffer);
    if(mode==0) {
#if ENUMERATION_V1
        return real_name.name_at(real_name.context,index);
#else
        return real_name(index);
#endif
    }
    assert(index<5);return which?rows[3]:rows[index];
}
#if ENUMERATION_V1
static int context0,context1;
static size_t count_op(void *context) { assert(context==&context0 || context==&context1);return call_count(context==&context1); }
static Oop name_op(void *context,size_t index) { assert(context==&context0 || context==&context1);return call_name(context==&context1,index); }
static NamespaceCountV1 counts[]={ {1,sizeof(NamespaceCountV1),&context0,count_op}, {1,sizeof(NamespaceCountV1),&context1,count_op} };
static NamespaceNameAtV1 names[]={ {1,sizeof(NamespaceNameAtV1),&context0,name_op}, {1,sizeof(NamespaceNameAtV1),&context1,name_op} };
static NamespaceCountPublicationV1 count_slots[]={ {1,sizeof(NamespaceCountPublicationV1),&counts[0]}, {1,sizeof(NamespaceCountPublicationV1),&counts[1]} };
static NamespaceNameAtPublicationV1 name_slots[]={ {1,sizeof(NamespaceNameAtPublicationV1),&names[0]}, {1,sizeof(NamespaceNameAtPublicationV1),&names[1]} };
#else
static size_t count0(void) { return call_count(0); }
static size_t count1(void) { return call_count(1); }
static Oop name0(size_t index) { return call_name(0,index); }
static Oop name1(size_t index) { return call_name(1,index); }
#endif
static void verify_fake(Oop vector,int count_generation,int name_generation) {
    assert(vector && vector!=api->nil_obj && saved.object_payload_size(vector)==sizeof(VectorPayload));
    size_t expected=count_generation?2:(name_generation?5:3);
    assert(((VectorPayload *)vector)->count==expected);
    for(size_t i=0;i<expected;i++)assert(item(vector,i)==(name_generation?rows[3]:(i<2?rows[0]:rows[3])));
}
static void *lookup(void *handle,const char *name) {
    if(!strcmp(name,COUNT_NAME)) {
        event("lookup-count;");if(mode==1)return NULL;
#if ENUMERATION_V1
        return &count_slots[generation];
#else
        return generation?(void *)count1:(void *)count0;
#endif
    }
    if(!strcmp(name,NAME_AT_NAME)) {
        event("lookup-name;");if(mode==2)return NULL;
        if(mode==6)generation=1;
        if(mode==9 && !nested) {
            int outer_mode=mode,outer_generation=generation;
            nested=1;mode=5;generation=0;event("reenter;");
            Oop value=enumerate(reflect,NULL,0);verify_fake(value,0,0);
            event("resume;");mode=outer_mode;generation=outer_generation;nested=0;
        }
#if ENUMERATION_V1
        return &name_slots[generation];
#else
        return generation?(void *)name1:(void *)name0;
#endif
    }
    return saved.dlsym_fn(handle,name);
}
static Oop import_name(void *context,const char *name) {
    assert(context==&saved_imports);
    if(!strcmp(name,"Vector")) {
        event("vector;");if(mode==3)return NULL;if(mode==4)return api->nil_obj;
        if(mode==7)generation=1;
    }
    return namespace_import_v1(&saved_imports,name);
}
static void auto_gc(void *context,int enabled) {
    assert(context==&saved_settings);
    event(enabled?"gc-on;":"gc-off;");if(!enabled && mode==8)generation=1;
    gc_settings_auto_v1(&saved_settings,enabled);
}
static void precise_gc(void *context,int enabled) {
    assert(context==&saved_settings);gc_settings_precise_v1(&saved_settings,enabled);
}
static void start(int selected_mode) {
    mode=selected_mode;generation=0;nested=0;used=0;trace[0]=0;
    saved=*api;saved_imports=*imports;api->dlsym_fn=lookup;imports->context=&saved_imports;imports->import=import_name;saved_settings=*gc_settings;gc_settings->context=&saved_settings;gc_settings->set_auto=auto_gc;gc_settings->set_precise=precise_gc;
}
static void stop(void) { api->dlsym_fn=saved.dlsym_fn;*imports=saved_imports;*gc_settings=saved_settings; }
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(count==0);
    for(int selected_mode=0;selected_mode<=11;selected_mode++) {
        start(selected_mode);Oop vector=enumerate(reflect,NULL,selected_mode==11?1:0);stop();
        if(selected_mode==0) {
            assert(vector && vector!=api->nil_obj);VectorPayload *v=vector;
#if ENUMERATION_V1
            size_t total=real_count.count(real_count.context);
#else
            size_t total=real_count();
#endif
            assert(v->count==total);
            for(size_t i=0;i<total;i++) {
#if ENUMERATION_V1
                assert(item(vector,i)==real_name.name_at(real_name.context,i));
#else
                assert(item(vector,i)==real_name(i));
#endif
            }
        } else if(selected_mode<=4 || selected_mode==11) {
            assert(vector==api->nil_obj);
            const char *expected=selected_mode==1?"lookup-count;":selected_mode==2?"lookup-count;lookup-name;":selected_mode==11?"":"lookup-count;lookup-name;vector;";
            assert(!strcmp(trace,expected));
        } else {
            verify_fake(vector,0,selected_mode==6);
            assert(strstr(trace,"lookup-count;lookup-name;")==trace);
            assert(strstr(trace,"gc-off;C0;") && strstr(trace,"gc-on;"));
            if(selected_mode==6 || selected_mode==7 || selected_mode==8 || selected_mode==10) {
                printf("enumeration mode %d first: %s\n",selected_mode,trace);
                /* Retain updated lookup choice, reset only the event stream. */
                used=0;trace[0]=0;saved=*api;saved_imports=*imports;api->dlsym_fn=lookup;imports->context=&saved_imports;imports->import=import_name;saved_settings=*gc_settings;gc_settings->context=&saved_settings;gc_settings->set_auto=auto_gc;gc_settings->set_precise=precise_gc;
                vector=enumerate(reflect,NULL,0);stop();verify_fake(vector,1,1);
                assert(strstr(trace,"gc-off;C1;N1:0;N1:1;gc-on;"));
            }
        }
        printf("enumeration mode %d: %s\n",selected_mode,trace);
    }
#if ENUMERATION_V1
    /* New ABI malformed publications/entries decline before later callbacks. */
    for(int broken=0;broken<10;broken++) {
        NamespaceCountPublicationV1 count_saved=count_slots[0];NamespaceNameAtPublicationV1 name_saved=name_slots[0];
        NamespaceCountV1 count_api=counts[0];NamespaceNameAtV1 name_api=names[0];
        if(broken==0)count_slots[0].version++;
        if(broken==1)count_slots[0].byte_size--;
        if(broken==2)count_slots[0].entry=NULL;
        if(broken==3)counts[0].count=NULL;
        if(broken==4)counts[0].context=NULL;
        if(broken==5)name_slots[0].version++;
        if(broken==6)name_slots[0].byte_size--;
        if(broken==7)name_slots[0].entry=NULL;
        if(broken==8)names[0].name_at=NULL;
        if(broken==9)names[0].context=NULL;
        start(5);Oop result=enumerate(reflect,NULL,0);stop();assert(result==api->nil_obj);
        assert(!strcmp(trace,broken<5?"lookup-count;":"lookup-count;lookup-name;"));
        count_slots[0]=count_saved;name_slots[0]=name_saved;counts[0]=count_api;names[0]=name_api;
    }
#endif
    return api->true_obj;
}
static void roots(void (*mark)(Oop)) { for(size_t i=0;i<5;i++)if(rows[i])mark(rows[i]); }
void *module_init(IdApi *selected,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *selected_imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizer_cap) { (void)finalizer_cap; (void)hooks; (void)layouts; (void)settings; (void)root_registry; (void)observation; (void)vtables;
    (void)exports;
    (void)presence;
    assert(selected && !state);if(api){assert(api==selected && imports==selected_imports);return NULL;}api=selected;gc_settings=(GcSettingsV1 *)settings;imports=(NamespaceImportV1 *)selected_imports;
    Dl_info info;assert(dladdr((void *)api->dlsym_fn,&info) && info.dli_fname);
    void *floor=api->dlopen_fn(info.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(floor);
#if ENUMERATION_V1
    const NamespaceCountPublicationV1 *count_slot=api->dlsym_fn(floor,COUNT_NAME);
    const NamespaceNameAtPublicationV1 *name_slot=api->dlsym_fn(floor,NAME_AT_NAME);
    assert(count_slot && name_slot && count_slot->entry && name_slot->entry);real_count=*count_slot->entry;real_name=*name_slot->entry;
    Dl_info count_owner,name_owner;assert(dladdr((void *)real_count.count,&count_owner) && dladdr((void *)real_name.name_at,&name_owner));
    assert(strstr(count_owner.dli_fname,"libbirnam_namespace.dylib") && strstr(name_owner.dli_fname,"libbirnam_namespace.dylib"));
#else
    real_count=(size_t(*)(void))api->dlsym_fn(floor,COUNT_NAME);real_name=(Oop(*)(size_t))api->dlsym_fn(floor,NAME_AT_NAME);assert(real_count && real_name);
#endif
    reflect=namespace_import_v1(imports,"RustReflectAdapter");assert(reflect);
    Oop selector=api->intern("allGlobalNames"),raw=api->send(api->vtable_of(reflect),api->intern("lookup"),&selector,1);assert(raw);memcpy(&enumerate,&raw,sizeof(enumerate));
    rows[0]=rows[2]=api->intern("enumeration-alpha");rows[1]=rows[4]=NULL;rows[3]=api->intern("enumeration-beta");root_registry_register_v1(root_registry,roots);
    api->method(namespace_import_v1(imports,"SystemVt"),api->intern("namespaceEnumerationProbe"),exercise);
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
