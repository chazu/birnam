/* Independent import-handoff oracle, usable in every execution form. */
#define _DARWIN_C_SOURCE
#include "idapi_v18.h"
#include "../../abi/fixtures/error-value-v3/error_value_v3.h"
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static IdApi *floor_api, watched;
static const ErrorValueApiV3 *errors;
static StringApiV1 *strings;
static uintptr_t identities[11];
#define ID(n) ((Oop)&identities[n])
static char events[128];static size_t used;static int mode,nested;
static NamespaceImportV1 *active_import;
static struct Choice { char event; Oop factory,behavior; } left,right;
static int present;
static void event(char c) { assert(used+1<sizeof(events));events[used++]=c;events[used]=0; }
static void expect(const char *s) { assert(!strcmp(events,s));used=0;events[0]=0; }
static Oop changed_import(void *context,const char *name);
static void replace(void) { active_import->context=&right;active_import->import=changed_import; }
static int contains(void *context,const char *name) {
    assert(!strcmp(name,"Error") || !strcmp(name,"ErrorVt"));event(context==&present?'p':'P');
    if(mode==3)replace();if(mode==8)watched.nil_obj=ID(0);return *(int *)context;
}
static Oop imported(void *context,const char *name) {
    struct Choice *choice=context;event(choice->event);
    Oop result=!strcmp(name,"ErrorVt")?choice->behavior:choice->factory;
    assert(!strcmp(name,"ErrorVt") || !strcmp(name,"Error"));
    if(mode==5)replace();if(mode==9)watched.nil_obj=ID(0);return result;
}
static Oop changed_import(void *context,const char *name) { assert(context==&right);return imported(context,name); }
static Oop intern(const char *name) {
    if(!strcmp(name,"kind:message:")) { event('s');if(mode==6)replace();return ID(1); }
    assert(!strcmp(name,"kind"));event('i');if(mode==1)replace();return ID(2);
}
static Oop vtable(Oop value) { assert(value==ID(5));event('v');if(mode==7)replace();return ID(3); }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    assert((self==ID(4) || self==ID(8)) && selector==ID(1) && count==2 && args[0]==ID(2) && args[1]==ID(6));
    event('d');return self==ID(4)?ID(7):ID(9);
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    assert(context==&mode && length==3 && !memcmp(bytes,"msg",3));event('m');
    if(mode==2)replace();
    if(mode==4 && !nested) {
        nested=1;int yes=-1;
        NamespacePresenceV1 presence={1,sizeof(presence),&yes,contains};
        NamespaceImportV1 imports={1,sizeof(imports),&right,changed_import};
        IdApi other=watched;other.nil_obj=ID(0);
        assert(errors->from_bytes(&other,&presence,&imports,(const uint8_t *)"kind",5,bytes,length)==ID(9));
        nested=0;
    }
    return ID(6);
}
static Oop construct(const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports) {
    return errors->from_bytes(&watched,presence,imports,(const uint8_t *)"kind",5,(const uint8_t *)"msg",3);
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(count==0);
    watched=*floor_api;watched.intern=intern;watched.vtable_of=vtable;watched.send=send;
    StringApiV1 original=*strings;strings->context=&mode;strings->make=make;
    left=(struct Choice){'r',ID(4),ID(3)};right=(struct Choice){'R',ID(8),ID(10)};
    NamespaceImportV1 imports={1,sizeof(imports),&left,imported};active_import=&imports;
    NamespacePresenceV1 presence={1,sizeof(presence),&present,contains};
    mode=nested=0;used=0;events[0]=0;present=1;
    assert(!errors->is_error(&watched,NULL,NULL,NULL));expect("");
    assert(errors->is_error(&watched,&presence,&imports,ID(5)));expect("prv");
    present=0;assert(!errors->is_error(&watched,&presence,NULL,ID(5)));expect("p");
    assert(construct(&presence,NULL)==watched.nil_obj);expect("imp");
    present=-1;assert(construct(&presence,&imports)==ID(7));expect("imprsd");
    left.factory=NULL;assert(construct(&presence,&imports)==watched.nil_obj);expect("impr");left.factory=ID(4);
    left.behavior=NULL;assert(!errors->is_error(&watched,&presence,&imports,ID(5)));expect("pr");left.behavior=ID(3);
    for(mode=1;mode<=3;mode++) {
        imports.context=&left;imports.import=imported;
        assert(construct(&presence,&imports)==ID(9));expect("impRsd");
    }
    mode=4;imports.context=&left;imports.import=imported;
    assert(construct(&presence,&imports)==ID(7));expect("imimPRsdprsd");
    for(mode=5;mode<=6;mode++) {
        imports.context=&left;imports.import=imported;
        assert(construct(&presence,&imports)==ID(7));expect("imprsd");
        assert(imports.context==&right && imports.import==changed_import);
    }
    mode=7;imports.context=&left;imports.import=imported;
    assert(errors->is_error(&watched,&presence,&imports,ID(5)));expect("prv");
    mode=0;assert(!errors->is_error(&watched,&presence,&imports,ID(5)));expect("pRv");
    mode=8;present=0;assert(construct(&presence,NULL)==ID(0));expect("imp");
    mode=9;present=1;watched.nil_obj=floor_api->nil_obj;right.factory=NULL;
    assert(construct(&presence,&imports)==ID(0));expect("impR");
    *strings=original;active_import=NULL;return floor_api->true_obj;
}
static void *open_sibling(const char *floor_path,const char *name) {
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/%s",(int)(slash-floor_path),floor_path,name);
    assert(length>0 && (size_t)length<sizeof(path));void *image=floor_api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);return image;
}
void *module_init(IdApi *api,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks) { (void)hooks; (void)layouts; (void)settings; (void)root_registry; (void)observation; (void)vtables;
    (void)exports;
    (void)presence;assert(api && !state);if(floor_api)assert(floor_api==api);else floor_api=api;
    Dl_info info;assert(dladdr((void *)api->dlsym_fn,&info) && info.dli_fname);
    void *image=open_sibling(info.dli_fname,"libbirnam_error_value.dylib");
    ErrorValueProviderV3 provider=(ErrorValueProviderV3)api->dlsym_fn(image,"birnam_error_value_provider_v3");
    assert(provider);errors=provider();assert(errors && errors->version==3 && errors->byte_size==sizeof(*errors));
    image=open_sibling(info.dli_fname,"libbirnam_string.dylib");
    StringProviderV1 string_provider=(StringProviderV1)api->dlsym_fn(image,"birnam_string_provider_v1");assert(string_provider);strings=string_provider();assert(strings);
    assert(dladdr((void *)errors->is_error,&info) && strstr(info.dli_fname,"/libbirnam_error_value.dylib"));
    assert(dladdr((void *)errors->from_bytes,&info) && strstr(info.dli_fname,"/libbirnam_error_value.dylib"));
    api->method(namespace_import_v1(imports,"SystemVt"),api->intern("errorValueImportProbe"),exercise);return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
