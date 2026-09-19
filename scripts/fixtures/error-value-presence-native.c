/* Independent v2 presence-handoff oracle, usable in all execution forms. */
#define _DARWIN_C_SOURCE
#include "idapi_v9.h"
#include "../../abi/fixtures/error-value-v2/error_value_v2.h"
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static IdApi *floor_api, watched;
static const ErrorValueApiV2 *errors;
static StringApiV1 *strings;
static uintptr_t identities[8];
#define ID(n) ((Oop)&identities[n])
static char events[128];
static size_t used;
static int mode, nested;
static NamespacePresenceV1 *active_presence;
static struct Choice { char event; int result; } left, right;
static void event(char value) { assert(used+1<sizeof(events));events[used++]=value;events[used]=0; }
static void expect(const char *expected) { assert(!strcmp(events,expected));used=0;events[0]=0; }
static int contains(void *context,const char *name) {
    struct Choice *choice=context;assert(!strcmp(name,"Error") || !strcmp(name,"ErrorVt"));
    event(choice->event);return choice->result;
}
static int changed_contains(void *context,const char *name) {
    assert(context==&right);assert(!strcmp(name,"Error"));event('b');return right.result;
}
static void replace(void) { active_presence->context=&right;active_presence->contains=changed_contains; }
static Oop intern(const char *name) {
    if(!strcmp(name,"kind:message:")) { event('s');return ID(1); }
    assert(!strcmp(name,"kind"));event('i');if(mode==1)replace();return ID(2);
}
static Oop imported(const char *name) {
    event('r');if(!strcmp(name,"ErrorVt"))return ID(3);assert(!strcmp(name,"Error"));return ID(4);
}
static Oop vtable(Oop value) { assert(value==ID(5));event('v');return ID(3); }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    assert(self==ID(4) && selector==ID(1) && count==2 && args[0]==ID(2) && args[1]==ID(6));
    event('d');return ID(7);
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    assert(context==&mode && length==3 && !memcmp(bytes,"msg",3));event('m');
    if(mode==2)replace();
    if(mode==3 && !nested) {
        nested=1;
        NamespacePresenceV1 other={1,sizeof(other),&right,contains};
        IdApi other_api=watched;other_api.nil_obj=ID(0);
        assert(errors->from_bytes(&other_api,&other,(const uint8_t *)"kind",5,bytes,length)==ID(0));
        nested=0;
    }
    return ID(6);
}
static Oop construct(const NamespacePresenceV1 *presence) {
    return errors->from_bytes(&watched,presence,(const uint8_t *)"kind",5,(const uint8_t *)"msg",3);
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(count==0);
    watched=*floor_api;watched.intern=intern;
    watched.import_fn=imported;watched.vtable_of=vtable;watched.send=send;
    StringApiV1 original=*strings;strings->context=&mode;strings->make=make;
    left=(struct Choice){'A',1};right=(struct Choice){'B',0};
    NamespacePresenceV1 presence={1,sizeof(presence),&left,contains};active_presence=&presence;
    mode=nested=0;used=0;events[0]=0;
    assert(!errors->is_error(&watched,NULL,NULL));expect("");
    assert(errors->is_error(&watched,&presence,ID(5))==1);expect("Arv");
    presence.context=&right;assert(!errors->is_error(&watched,&presence,ID(5)));expect("B");
    presence.context=&left;
    for(int result=-1;result<=1;result++) {
        left.result=result;
        assert(construct(&presence)==(result?ID(7):watched.nil_obj));expect(result?"imArsd":"imA");
    }
    right.result=1;
    for(mode=1;mode<=2;mode++) {
        presence.context=&left;presence.contains=contains;
        assert(construct(&presence)==ID(7));expect("imbrsd");
    }
    mode=3;right.result=0;presence.context=&left;presence.contains=contains;
    assert(construct(&presence)==ID(7));expect("imimBArsd");
    mode=0;assert(construct(&presence)==ID(7));expect("imArsd");
    *strings=original;active_presence=NULL;
    return floor_api->true_obj;
}
static void *open_sibling(const char *floor_path,const char *name) {
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/%s",(int)(slash-floor_path),floor_path,name);
    assert(length>0 && (size_t)length<sizeof(path));void *image=floor_api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);return image;
}
void *module_init(IdApi *api,void *state,const NamespacePresenceV1 *presence) {
    (void)presence;
    assert(api && !state);if(floor_api)assert(floor_api==api);else floor_api=api;
    Dl_info info;assert(dladdr((void *)api->dlsym_fn,&info) && info.dli_fname);
    void *image=open_sibling(info.dli_fname,"libbirnam_error_value.dylib");
    ErrorValueProviderV2 provider=(ErrorValueProviderV2)api->dlsym_fn(image,"birnam_error_value_provider_v2");
    assert(provider);errors=provider();assert(errors && errors->version==2 && errors->byte_size==sizeof(*errors));
    image=open_sibling(info.dli_fname,"libbirnam_string.dylib");
    StringProviderV1 string_provider=(StringProviderV1)api->dlsym_fn(image,"birnam_string_provider_v1");
    assert(string_provider);strings=string_provider();assert(strings);
    assert(dladdr((void *)errors->is_error,&info) && strstr(info.dli_fname,"/libbirnam_error_value.dylib"));
    assert(dladdr((void *)errors->from_bytes,&info) && strstr(info.dli_fname,"/libbirnam_error_value.dylib"));
    api->method(api->import_fn("SystemVt"),api->intern("errorValuePresenceProbe"),exercise);
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
