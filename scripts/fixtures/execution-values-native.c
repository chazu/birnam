/* Exercise shipped native values/control with no parser/compiler modules. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/idapi-v20/idapi_v20.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <stdio.h>
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
static Oop module_argument_count(Oop self,const Oop *args,size_t count) {
    (void)self; (void)args; assert(!count); return integer.make(integer.context,2);
}
static Oop module_argument_at(Oop self,const Oop *args,size_t count) {
    (void)self; assert(count==1); int64_t index=number(args[0]); assert(index==0 || index==1);
    api->gc_collect();
    /* No fixture root: the loader must retain each result across the next send. */
    return string.make(string.context,(const uint8_t *)(index?"second":"first"),index?6:5);
}
static void check_native_loading(Oop system) {
    size_t saved=root_count;
    char buffer[4096];
    snprintf(buffer,sizeof(buffer),"%s/system-native-0.dylib",getenv("BIRNAM_83_NATIVE_FIXTURES"));
    Oop path=text(buffer);
    assert(send1(system,"loadModule:",path)==system);
    Oop behavior=keep(api->proto(named("ObjectVt")));
    api->method(behavior,api->intern("size"),module_argument_count);
    api->method(behavior,api->intern("at:"),module_argument_at);
    Oop sequence=keep(api->alloc(behavior,0));
    Oop arguments[]={path,sequence};
    assert(send1(system,"preciseGc:",api->true_obj)==system);
    Oop result=keep(api->send(system,api->intern("loadModule:arguments:"),arguments,2));
    size_t length=0; const uint8_t *bytes=string.bytes(string.context,result,&length);
    assert(length==5 && !memcmp(bytes,"first",5));
    api->method(behavior,api->intern("size"),body);
    Oop error=keep(api->send(system,api->intern("loadModule:arguments:"),arguments,2));
    assert(send0(error,"kind")==api->intern("typeError"));
    for(int mode=1;mode<=4;mode++) {
        snprintf(buffer,sizeof(buffer),"%s/system-native-%d.dylib",getenv("BIRNAM_83_NATIVE_FIXTURES"),mode);
        error=keep(send1(system,"loadModule:",text(buffer)));
        assert(send0(error,"kind")==api->intern(mode==1?"moduleError":mode==2?"typeError":"notAModule"));
    }
    root_count=saved;
}
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
/* Cross-module calls must observe replacement, including collection during it. */
static size_t dependency_calls, dependency_arity;
static Oop dependency_answer;
static Oop dependency_probe(Oop receiver,const Oop *args,size_t count) {
    (void)args; assert(count==dependency_arity); dependency_calls++;
    api->gc_collect();
    return dependency_answer ? dependency_answer : receiver;
}
static Oop replace_vector(const char *name,size_t arity,Oop answer) {
    Oop type=named("VectorVt"), selector=api->intern(name);
    Oop original=keep(send1(type,"lookup",selector));
    dependency_calls=0; dependency_arity=arity; dependency_answer=answer;
    api->method(type,selector,dependency_probe);
    return original;
}
static void restore_vector(const char *name,Oop original) {
    Oop args[]={original,api->intern(name)};
    Oop restored=api->send(named("VectorVt"),api->intern("addMethod:as:"),args,2);
    assert(send0(restored,"isError")!=api->true_obj);
}
static void check_collection_dispatch(Oop map,Oop range,Oop vector,Oop visitor,Oop marker) {
    size_t saved=root_count;
    Oop empty=keep(send0(named("Vector"),"new"));
    Oop input=text("a,b"), separator=text(",");
    Oop original=replace_vector("new",0,empty);
    assert(number(send0(send1(input,"split:",separator),"size"))==2);
    send0(map,"keys"); send0(range,"reversed");
    assert(dependency_calls==3); restore_vector("new",original);
    original=replace_vector("add:",1,NULL);
    send1(input,"split:",separator); send0(map,"values"); send0(range,"reversed");
    assert(dependency_calls==5); restore_vector("add:",original);
    original=replace_vector(",",1,marker);
    assert(send1(range,",",vector)==marker && dependency_calls==1);
    restore_vector(",",original);
    original=replace_vector("do:",1,api->nil_obj);
    assert(send1(map,"do:",visitor)==map && dependency_calls==1);
    restore_vector("do:",original);
    original=replace_vector("inject:into:",2,marker);
    Oop args[]={marker,visitor};
    assert(api->send(map,api->intern("inject:into:"),args,2)==marker && dependency_calls==1);
    restore_vector("inject:into:",original);
    root_count=saved;
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
static Oop lookup_answer;
static Oop object_lookup(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(count==1);
    /* A nested ordinary send and collection must preserve the owned answer. */
    assert(send0(api->nil_obj,"isNil")==api->true_obj);
    api->gc_collect();
    return lookup_answer;
}
static void check_methods(const BirnamProtocol *id) {
    size_t saved_roots=root_count;
    Oop (*language_send)(Oop,Oop,const Oop *,size_t)=api->send;
    assert(language_send);
    Oop behavior=keep(api->proto(named("ObjectVt")));
    Oop receiver=keep(api->alloc(behavior,0));
    Oop plain=keep(api->intern("plainNativeMethodProbe"));
    api->method(behavior,plain,body);
    Oop callable=keep(send1(behavior,"lookup",plain));
    assert(api->vtable_of(callable)==named("MethodVt"));
    Oop removed=keep(send1(behavior,"removeMethod:",plain));
    assert(api->vtable_of(removed)==named("MethodVt"));
    assert(send1(behavior,"lookup",plain)==api->nil_obj);
    api->gc_collect();
    size_t before=calls;
    assert(send1(callable,"invokeOn:",receiver)==api->nil_obj && calls==before+1);
    Oop raw_install[]={plain,(Oop)body};
    Oop rejected=keep(api->send(behavior,api->intern("addMethod"),raw_install,2));
    assert(send0(rejected,"isError")==api->true_obj);
    assert(send1(behavior,"lookup",plain)==api->nil_obj);
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
    Oop meta=keep(api->proto(api->vtable_vt));
    api->method(meta,api->intern("lookup"),object_lookup);
    Oop reassociate[]={behavior,meta};
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),reassociate,2);
    lookup_answer=callable;
    before=calls;
    assert(api->send(receiver,plain,NULL,0)==api->nil_obj && calls==before+1);
    lookup_answer=(Oop)body;
    rejected=keep(api->send(receiver,plain,NULL,0));
    assert(send0(rejected,"kind")==api->intern("nativeInvalidRequest"));
    assert(calls==before+1); /* The address was rejected, never invoked. */
    lookup_answer=api->nil_obj;
    root_count=saved_roots;
    api->gc_collect();
}
static void check_compiler(const BirnamProtocol *id,const BirnamOperand *path,int install) {
    assert(path->kind==BIRNAM_BYTES);
    char *name=malloc(path->data.bytes.length+1);assert(name);
    memcpy(name,path->data.bytes.pointer,path->data.bytes.length);name[path->data.bytes.length]=0;
    void *image=dlopen(name,RTLD_NOW|RTLD_LOCAL);free(name);assert(image);
    assert(!dlsym(image,"birnam_runtime_compiler_provider_v1"));
    const BirnamModule *module=dlsym(image,"birnam_native_module_v1");assert(module);
    BirnamOperand request[2]={{.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)"compiler",8}},
        {.kind=BIRNAM_WORD,.data.word=0}};
    assert(module->initialize(id,request,2).status==BIRNAM_INVALID_REQUEST);
    request[1].data.word=(size_t)api;
    BirnamProtocol foreign=*id;foreign.receiver=api->nil_obj;
    assert(module->initialize(&foreign,request,2).status==BIRNAM_INVALID_REQUEST);
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
        check_methods(id);
        BirnamOutcome result=module->initialize(id,request,2);
        assert(result.status==BIRNAM_LOADED && result.value.kind==BIRNAM_WORD);
        const RuntimeCompilerV1 *compiler=(const void *)result.value.data.word;
        assert(compiler && compiler->version==1 && compiler->byte_size==32);
        assert(compiler->context && compiler->resolve);
        const char *symbols[]={
            "birnam_register_resumable_v1","birnam_register_resumable_v2",
            "birnam_id_begin_resumable_unit_v1","birnam_id_begin_portable_resumable_unit_v1",
            "birnam_id_finish_resumable_unit_v1","birnam_id_cancel_resumable_unit_v1",
            "birnam_id_try_acquire_stackless_generation_lease_v1","birnam_id_release_stackless_generation_lease_v1",
            "birnam_id_retire_stackless_code_generation_v1","birnam_id_stackless_block_vtable_image_reference_v1",
            "birnam_id_stackless_block_vtable_matches_v1","birnam_id_staged_portable_block_vtable_v1",
            "birnam_id_stackless_resumable_manifest_matches_v1",
            "birnam_id_try_closure_elision_v2"};
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
    if(args[6].data.word==7 || args[6].data.word==8) {
        if(!registered) { root_registry_register_v1(registry,mark); registered=1; }
        assert(!namespace_import_v1(imports,"Module"));
        Oop system=named("System");
        if(args[6].data.word==7) {
            absent(id,api->vtable_of(system),"saveImage:");
            assert(named("db")==named("Store"));
            Oop entity=keep(send0(named("db"),"entity"));
            assert(api->vtable_of(entity)==named("EntityVt"));
            Oop name=api->intern("nativeVariable");
            Oop variable=keep(send1(named("LogicVar"),"named:",name));
            assert(send0(variable,"name")==name);
            assert(send1(named("LogicVar"),"named:",name)==variable);
            Oop address=keep(send1(named("Alien"),"alloc:",integer.make(integer.context,16)));
            assert(address!=api->nil_obj);
            assert(send0(address,"isNull")==api->false_obj);
            (void)send0(address,"free");
            Oop missing=keep(send1(named("Alien"),"dlopen:",text("/no/birnam/library.dylib")));
            assert(missing==api->nil_obj);
            Oop bad_size=keep(send1(named("Alien"),"alloc:",integer.make(integer.context,0)));
            assert(send0(bad_size,"isError")==api->true_obj);
            Oop bad=keep(send0(named("Isolate"),"start:selector:image:"));
            assert(send0(bad,"isError")==api->true_obj);
            api->gc_collect();
            assert(send0(variable,"name")==name);
            root_count=0;
            return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=system}};
        }
        Oop report=send0(system,"imageSaveReport");
        assert(report==named("ImageSaveReport"));
        assert(send1(system,"loadImage:",text("/no/birnam/image"))!=api->true_obj);
        root_count=0;
        return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=report}};
    }
    if(args[6].data.word==6) {
        if(!registered) { root_registry_register_v1(registry,mark); registered=1; }
        assert(!namespace_import_v1(imports,"Vector"));
        assert(!namespace_import_v1(imports,"Store"));
        assert(!namespace_import_v1(imports,"Lang"));
        assert(!namespace_import_v1(imports,"Tcc"));
        assert(!namespace_import_v1(imports,"Codegen"));
        check_compiler(id,&args[5],0);
        Oop system=named("System");
        absent(id,api->vtable_of(system),"load:");
        Oop value=keep(send1(system,"getEnv:",text("BIRNAM_83_SYSTEM_VALUE")));
        size_t length=0; const uint8_t *bytes=string.bytes(string.context,value,&length);
        assert(length==13 && !memcmp(bytes,"native-system",13));
        Oop path=text(getenv("BIRNAM_83_SYSTEM_FILE"));
        Oop payload=text("native file contents"), write_args[]={path,payload};
        assert(api->send(system,api->intern("writeFile:contents:"),write_args,2)==system);
        value=keep(send1(system,"readFile:",path));
        bytes=string.bytes(string.context,value,&length);
        assert(length==20 && !memcmp(bytes,"native file contents",20));
        assert(remove(getenv("BIRNAM_83_SYSTEM_FILE"))==0);
        Oop error=keep(send1(system,"readFile:",path));
        assert(send0(error,"kind")==api->intern("ioError"));
        error=keep(send1(system,"loadModule:",path));
        assert(send0(error,"kind")==api->intern("ioError"));
        check_native_loading(system);
        assert(send1(system,"readLine",api->nil_obj)==api->nil_obj);
        assert(send1(system,"print:",text(""))==system);
        assert(number(send0(system,"liveObjects"))>0);
        assert(send1(system,"preciseGc:",api->true_obj)==system);
        assert(number(send0(system,"gc"))>=0);
        assert(send1(system,"preciseGc:",api->false_obj)==system);
        root_count=0;
        return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=system}};
    }
    if(args[6].data.word==5) {
        if(!registered) { root_registry_register_v1(registry,mark); registered=1; }
        assert(!namespace_import_v1(imports,"System"));
        assert(named("CodeObject") && named("CodeProgram"));
        Oop unit=keep(send1(named("Tcc"),"compile:",text("long answer(void) { return 42; }")));
        assert(unit!=api->nil_obj);
        Oop address=keep(send1(unit,"symbol:",text("answer")));
        assert(address!=api->nil_obj);
        long (*answer)(void)=(void *)*(uintptr_t *)address;
        assert(answer && answer()==42);
        Oop codegen=named("RustCodegen");
        assert(number(send1(codegen,"eval:",text("[1 + 2]")))==3);
        Oop closure=keep(send1(codegen,"eval:",text("let x := 40; [| [x + 2]]")));
        if(closure==api->nil_obj) {
            Oop error=send0(codegen,"lastError"); size_t length=0;
            const uint8_t *message=string.bytes(string.context,error,&length);
            fprintf(stderr,"compiler prefix closure failed: %.*s\n",(int)length,message);
        }
        assert(closure!=api->nil_obj);
        api->gc_collect();
        assert(number(send0(closure,"value"))==42);
        assert(send1(named("Tcc"),"compile:",text("this is invalid C"))==api->nil_obj);
        root_count=0;
        return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=codegen}};
    }
    assert(!namespace_import_v1(imports,"RustCodegen"));
    assert(!namespace_import_v1(imports,"Tcc"));
    if(args[6].data.word==4) {
        if(!registered) { root_registry_register_v1(registry,mark); registered=1; }
        check_compiler(id,&args[5],0);
        Oop source=text("[1 + 2]"), lang=named("Lang");
        Oop tree=keep(send1(lang,"parse:",source));
        assert(api->vtable_of(tree)==named("PegCstVt"));
        assert(number(send0(tree,"start"))==0 && number(send0(tree,"end"))==7);
        assert(strcmp(api->symbol_name(send0(tree,"rule")),"program")==0);
        Oop children=keep(send0(tree,"children"));
        assert(number(send0(children,"size"))==number(send0(tree,"childCount")));
        api->gc_collect();
        assert(number(send0(tree,"end"))==7);
        Oop bad=keep(send1(lang,"parse:",text("[1 +]")));
        assert(send0(bad,"isError")==api->true_obj);
        assert(strcmp(api->symbol_name(send0(bad,"kind")),"syntaxError")==0);
        Oop artifact=named("Artifact");
        assert(send1(artifact,"inspect:",api->nil_obj)==api->nil_obj);
        assert(strcmp(api->symbol_name(send0(artifact,"lastFailure")),"artifactPathInvalid")==0);
        assert(send1(artifact,"inspect:",text("/birnam-test-missing-artifact"))==api->nil_obj);
        assert(strcmp(api->symbol_name(send0(artifact,"lastFailure")),"artifactUnavailable")==0);
        check_compiler(id,&args[5],0);
        root_count=0;
        return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=lang}};
    }
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
    Oop range=keep(send1(one,"to:",two));
    Oop joined=keep(send1(range,",",vector));
    printed(joined,"#[1 2 1 2]");
    check_collection_dispatch(map,range,vector,visitor,expected);
    if(args[6].data.word==3) {
        assert(!namespace_import_v1(imports,"Store"));
        assert(!namespace_import_v1(imports,"Globals"));
        root_count=0;
        return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=block_proto}};
    }
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
