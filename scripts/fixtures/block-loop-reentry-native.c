/* First initialization can reenter while selecting names or publishing methods. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/idapi-v20/idapi_v20.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *original, selected, other;
static const NamespacePresenceV1 *presence;
static const NamespaceImportV1 *imports;
static ModuleInitFn initialize_loop;
static size_t mode, entries, sends;
static void enter_again(void) {
    entries++;
    initialize_loop(&other,NULL,presence,imports,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
}
static Oop intern_selected(const char *name) {
    if(mode==1 && !entries && !strcmp(name,"whileTrue:")) enter_again();
    return original->intern(name);
}
static void install_selected(Oop type,Oop selector,Method implementation) {
    if(mode==2 && !entries) enter_again();
    original->method(type,selector,implementation);
}
static Oop send_selected(Oop receiver,Oop selector,const Oop *args,size_t count) {
    sends++; return original->send(receiver,selector,args,count);
}
static Oop send_other(Oop receiver,Oop selector,const Oop *args,size_t count) {
    (void)receiver;(void)selector;(void)args;(void)count;
    assert(!"reentry changed the first execution table"); return NULL;
}
static Oop false_condition(Oop receiver,const Oop *args,size_t count) {
    (void)receiver;(void)args;assert(!count);return original->false_obj;
}
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    (void)id; assert(count==6);
    original=(void *)args[0].data.word; presence=(void *)args[1].data.word;
    imports=(void *)args[2].data.word;
    const void *error=(void *)args[3].data.word;
    assert(args[4].kind==BIRNAM_BYTES && args[4].data.bytes.length<4096);
    char path[4096];memcpy(path,args[4].data.bytes.pointer,args[4].data.bytes.length);path[args[4].data.bytes.length]=0;
    mode=args[5].data.word;assert(mode==1||mode==2);entries=sends=0;
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    int (*attach)(const void *)=(int (*)(const void *))dlsym(image,"birnam_error_value_dependency_v3");
    assert(attach && attach(error)==1);
    initialize_loop=(ModuleInitFn)dlsym(image,"module_init");assert(initialize_loop);
    selected=*original;other=*original;
    selected.intern=intern_selected;selected.method=install_selected;selected.send=send_selected;
    selected.nil_obj=original->true_obj;other.send=send_other;
    initialize_loop(&selected,NULL,presence,imports,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
    assert(entries==1);
    Oop type=original->proto(namespace_import_v1(imports,"BlockVt"));
    original->method(type,original->intern("value"),false_condition);
    Oop condition=original->alloc(type,0), body=original->nil_obj;
    Oop answer=original->send(condition,original->intern("whileTrue:"),&body,1);
    assert(answer==selected.nil_obj && sends>0);
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
