/* Exercise shipped native values/control with no parser/compiler modules. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/idapi-v20/idapi_v20.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>
static IdApi *api;
static const NamespaceImportV1 *imports;
static IntegerApiV1 integer;
static StringApiV1 string;
static Oop roots[64];
static size_t root_count, calls, limit;
static int registered;
static Oop keep(Oop object) { assert(object && root_count<64); roots[root_count++]=object; return object; }
static void mark(RootRegistryMarkV1 mark_one) { for(size_t i=0;i<root_count;i++) mark_one(roots[i]); }
static Oop named(const char *name) { Oop o=namespace_import_v1(imports,name); assert(o); return o; }
static Oop send0(Oop receiver,const char *name) { return api->send(receiver,api->intern(name),NULL,0); }
static Oop send1(Oop receiver,const char *name,Oop argument) { return api->send(receiver,api->intern(name),&argument,1); }
static Oop text(const char *s) { return keep(string.make(string.context,(const uint8_t *)s,strlen(s))); }
static int64_t number(Oop object) { assert(integer.is_exact(integer.context,object)); return integer.read(integer.context,object); }
static Oop capture(Oop receiver,const Oop *args,size_t count) { (void)args; assert(!count); return *(Oop *)receiver; }
static Oop visit(Oop receiver,const Oop *args,size_t count) { (void)receiver; assert(count==1); calls++; return args[0]; }
static Oop condition(Oop receiver,const Oop *args,size_t count) { (void)receiver;(void)args; assert(!count);return calls<limit?api->true_obj:api->false_obj; }
static Oop body(Oop receiver,const Oop *args,size_t count) { (void)receiver;(void)args;assert(!count);calls++;return api->nil_obj; }
static Oop bad_condition(Oop receiver,const Oop *args,size_t count) { (void)receiver;(void)args;assert(!count);return api->nil_obj; }
static void printed(Oop value,const char *expected) {
    Oop result=send0(value,"printString");size_t size=0;
    const uint8_t *bytes=string.bytes(string.context,result,&size);
    assert(size==strlen(expected) && !memcmp(bytes,expected,size));
}
static Oop native_block(const char *selector,Method method,Oop captured) {
    Oop vt=keep(api->proto(named("BlockVt")));
    api->method(vt,api->intern(selector),method);
    Oop block=keep(api->alloc(vt,sizeof(Oop))); *(Oop *)block=captured; return block;
}
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void *(*resolve)(void *,const uint8_t *,size_t);
} RuntimeCompilerV1;
_Static_assert(sizeof(RuntimeCompilerV1)==32,"compiler descriptor extent");
typedef struct { Oop result; size_t calls; } MethodContext;
static BirnamOutcome method_answer(void *context,Oop receiver,const BirnamOperand *args,size_t count) {
    MethodContext *state=context; assert(receiver && count==2);
    assert(args[0].kind==BIRNAM_OBJECT && number(args[0].data.object)==1);
    assert(args[1].kind==BIRNAM_OBJECT && number(args[1].data.object)==2);
    state->calls++;
    api->gc_collect();
    return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=state->result}};
}
static void check_methods(const BirnamProtocol *id,void *image) {
    size_t saved_roots=root_count;
    Oop (*language_send)(Oop,Oop,const Oop *,size_t)=dlsym(image,"birnam_runtime_send_v1");
    assert(language_send);
    Oop behavior=keep(api->proto(named("ObjectVt")));
    Oop receiver=keep(api->alloc(behavior,0));
    Oop selector=keep(api->intern("nativeMethodProbe"));
    MethodContext first={keep(integer.make(integer.context,71)),0};
    MethodContext second={keep(integer.make(integer.context,92)),0};
    BirnamOperand binding[2]={{.kind=BIRNAM_OBJECT,.data.object=selector},
        {.kind=BIRNAM_NATIVE_ENTRY,.data.entry={method_answer,&first}}};
    assert(id->message(id->context,behavior,api->intern("addMethod"),binding,2).status==BIRNAM_LOADED);
    Oop owned=keep(language_send(receiver,api->intern("methodFor:"),&selector,1));
    assert(api->vtable_of(owned)==named("MethodVt"));
    binding[1].data.entry.context=&second;
    assert(id->message(id->context,behavior,api->intern("addMethod"),binding,2).status==BIRNAM_LOADED);
    Oop replacement=keep(language_send(receiver,api->intern("methodFor:"),&selector,1));
    assert(id->message(id->context,behavior,api->intern("removeMethod:"),binding,1).status==BIRNAM_LOADED);
    assert(language_send(receiver,api->intern("methodFor:"),&selector,1)==api->nil_obj);
    Oop vector=keep(send0(named("Vector"),"new"));
    vector=keep(send1(vector,"add:",integer.make(integer.context,1)));
    vector=keep(send1(vector,"add:",integer.make(integer.context,2)));
    Oop call[]={receiver,vector};
    assert(language_send(owned,api->intern("invokeOn:arguments:"),call,2)==first.result);
    assert(language_send(replacement,api->intern("invokeOn:arguments:"),call,2)==second.result);
    assert(first.calls==1 && second.calls==1);
    Oop alias=keep(api->intern("nativeMethodAlias"));
    Oop install[]={owned,alias};
    Oop installed=keep(language_send(behavior,api->intern("addMethod:as:"),install,2));
    assert(api->vtable_of(installed)==named("MethodVt"));
    Oop operands[]={integer.make(integer.context,1),integer.make(integer.context,2)};
    assert(language_send(receiver,alias,operands,2)==first.result && first.calls==2);
    /* Contexts are native-owned; remove the alias and roots before they expire. */
    binding[0].data.object=alias;
    assert(id->message(id->context,behavior,api->intern("removeMethod:"),binding,1).status==BIRNAM_LOADED);
    root_count=saved_roots;
    api->gc_collect();
}
static void check_compiler(const BirnamProtocol *id,const BirnamOperand *path,int install) {
    assert(path->kind==BIRNAM_BYTES);
    char *name=malloc(path->data.bytes.length+1);assert(name);
    memcpy(name,path->data.bytes.pointer,path->data.bytes.length);name[path->data.bytes.length]=0;
    void *image=dlopen(name,RTLD_NOW|RTLD_LOCAL);free(name);assert(image);
    const RuntimeCompilerV1 *(*provider)(void)=dlsym(image,"birnam_runtime_compiler_provider_v1");
    assert(provider && !provider());
    const BirnamModule *module=dlsym(image,"birnam_native_module_v1");assert(module);
    BirnamOperand request[2]={{.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)"compiler",8}},
        {.kind=BIRNAM_WORD,.data.word=0}};
    assert(module->initialize(id,request,2).status==BIRNAM_INVALID_REQUEST && !provider());
    request[1].data.word=(size_t)api;
    BirnamProtocol foreign=*id;foreign.receiver=api->nil_obj;
    assert(module->initialize(&foreign,request,2).status==BIRNAM_INVALID_REQUEST && !provider());
    const char *services[]={"namespacePresence","namespaceImport"};
    for(size_t i=0;i<2;i++) {
        BirnamOperand selected[2]={{.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)services[i],strlen(services[i])}},request[1]};
        assert(module->initialize(&foreign,selected,2).status==BIRNAM_INVALID_REQUEST);
        BirnamOutcome found=module->initialize(id,selected,2);
        assert(found.status==BIRNAM_LOADED && found.value.kind==BIRNAM_WORD && found.value.data.word);
        if(i==1) assert(found.value.data.word==(size_t)imports);
        selected[1].data.word=0;
        assert(module->initialize(id,selected,2).status==BIRNAM_INVALID_REQUEST);
    }
    if(install) {
        check_methods(id,image);
        BirnamOutcome result=module->initialize(id,request,2);
        assert(result.status==BIRNAM_LOADED && result.value.kind==BIRNAM_WORD);
        const RuntimeCompilerV1 *compiler=(const void *)result.value.data.word;
        assert(compiler==provider() && compiler->version==1 && compiler->byte_size==32);
        assert(compiler->context && compiler->resolve);
        const char *symbols[]={
            "birnam_register_resumable_v1","birnam_register_resumable_v2",
            "birnam_id_begin_resumable_unit_v1","birnam_id_begin_portable_resumable_unit_v1",
            "birnam_id_finish_resumable_unit_v1","birnam_id_cancel_resumable_unit_v1",
            "birnam_id_try_acquire_stackless_generation_lease_v1","birnam_id_release_stackless_generation_lease_v1",
            "birnam_id_retire_stackless_code_generation_v1","birnam_id_stackless_block_vtable_image_reference_v1",
            "birnam_id_stackless_block_vtable_matches_v1","birnam_id_staged_portable_block_vtable_v1",
            "birnam_id_stackless_resumable_manifest_matches_v1",
            "birnam_id_warm_block_construction_v1","birnam_id_try_closure_elision_v2"};
        for(size_t i=0;i<sizeof(symbols)/sizeof(symbols[0]);i++)
            assert(compiler->resolve(compiler->context,(const uint8_t *)symbols[i],strlen(symbols[i]))==dlsym(image,symbols[i]));
        assert(!compiler->resolve(compiler->context,(const uint8_t *)"unknown",7));
        result=module->initialize(id,request,2);
        assert(result.status==BIRNAM_LOADED && result.value.data.word==(size_t)compiler);
    }
    assert(!dlclose(image));
}
static void absent(const BirnamProtocol *id,Oop behavior,const char *name) {
    BirnamOperand selector={.kind=BIRNAM_OBJECT,.data.object=api->intern(name)};
    BirnamOutcome result=id->message(id->context,behavior,api->intern("lookup"),&selector,1);
    assert(result.status==BIRNAM_LOADED && result.value.kind==BIRNAM_UNIT);
}
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    assert(count==7 && args[6].kind==BIRNAM_WORD);
    for(size_t i=0;i<5;i++) assert(args[i].kind==BIRNAM_WORD);
    api=(void *)args[0].data.word; imports=(void *)args[1].data.word;
    const RootRegistryV1 *registry=(void *)args[2].data.word;
    integer=*(IntegerApiV1 *)args[3].data.word; string=*(StringApiV1 *)args[4].data.word;
    assert(!namespace_import_v1(imports,"RustCodegen"));
    assert(!namespace_import_v1(imports,"TCC"));
    assert(!namespace_import_v1(imports,"Peg"));
    if(!args[6].data.word) {
        Oop machine=send0(named("Machine"),"new");
        assert(number(send0(machine,"liveFrameCount"))==0);
        assert(named("Block") && named("BlockVt"));
        absent(id,api->vtable_of(api->true_obj),"ifTrue:");
        absent(id,integer.behavior,"to:do:");
        absent(id,named("BlockVt"),"whileTrue:");
        check_compiler(id,&args[5],0);
        return (BirnamOutcome){.status=BIRNAM_LOADED};
    }
    if(!registered) { root_registry_register_v1(registry,mark); registered=1; }
    Oop block_proto=named("Block"), block_vt=named("BlockVt");
    assert(api->vtable_of(block_proto)==block_vt && vtable_owner(api,block_vt)==block_proto);
    Oop expected=text("native capture");
    Oop closure=native_block("value",capture,expected);
    roots[0]=NULL; /* The Block capture is now the only registered root path. */
    api->gc_collect();
    assert(send0(closure,"value")==expected);
    assert(send1(api->true_obj,"ifTrue:",closure)==expected);
    assert(send1(api->false_obj,"ifTrue:",closure)==api->nil_obj);
    printed(named("Object"),"anObject");
    printed(api->intern("nativeSymbol"),"#nativeSymbol");
    Oop iterator=native_block("value:",visit,api->nil_obj); calls=0;
    Oop start=keep(integer.make(integer.context,1)), end=keep(integer.make(integer.context,3));
    Oop iteration[]={end,iterator};
    api->send(start,api->intern("to:do:"),iteration,2); assert(calls==3);
    Oop predicate=native_block("value",condition,api->nil_obj);
    Oop work=native_block("value",body,api->nil_obj); calls=0; limit=3;
    assert(send1(predicate,"whileTrue:",work)==api->nil_obj && calls==3);
    Oop invalid=native_block("value",bad_condition,api->nil_obj);
    Oop error=keep(send1(invalid,"whileTrue:",work));
    assert(send0(error,"isError")==api->true_obj);
    assert(strcmp(api->symbol_name(send0(error,"kind")),"mustBeBoolean")==0);
    if(args[6].data.word==2) {
        assert(!namespace_import_v1(imports,"Vector"));
        assert(!namespace_import_v1(imports,"Globals"));
        root_count=0;
        return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=block_proto}};
    }
    Oop vector=keep(send0(named("Vector"),"new"));
    Oop one=keep(integer.make(integer.context,1)), two=keep(integer.make(integer.context,2));
    vector=keep(send1(vector,"add:",one)); vector=keep(send1(vector,"add:",two));
    assert(number(send0(vector,"size"))==2);
    printed(vector,"#[1 2]");
    printed(named("Object"),"anObject");
    Oop builder=keep(send0(named("StringBuilder"),"new"));
    send1(builder,"add:",named("Object")); printed(builder,"anObject");
    printed(api->intern("nativeSymbol"),"#nativeSymbol");
    Oop visitor=native_block("value:",visit,api->nil_obj); calls=0;
    send1(vector,"do:",visitor); assert(calls==2);
    Oop map=keep(send0(named("Map"),"new"));
    Oop pair[]={one,expected}; map=keep(api->send(map,api->intern("at:put:"),pair,2));
    assert(send1(map,"at:",one)==expected);
    assert(number(send0(map,"size"))==1);
    Oop parts=keep(send1(text("a,b"),"split:",text(",")));
    assert(number(send0(parts,"size"))==2);
    Oop globals=named("Globals"), key=keep(api->intern("earlyNativeValue"));
    Oop binding[]={key,expected}; api->send(globals,api->intern("at:put:"),binding,2);
    assert(send1(globals,"at:",key)==expected);
    Oop machine=keep(send0(named("Machine"),"new"));
    assert(number(send0(machine,"liveFrameCount"))==0);
    api->gc_collect();
    assert(send0(closure,"value")==expected && send1(globals,"at:",key)==expected);
    check_compiler(id,&args[5],1);
    return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=block_proto}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
