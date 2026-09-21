/* Exercise shipped native values/control with no parser/compiler modules. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/idapi-v20/idapi_v20.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <string.h>
static IdApi *api;
static const NamespaceImportV1 *imports;
static IntegerApiV1 integer;
static StringApiV1 string;
static Oop roots[32];
static size_t root_count, calls, limit;
static Oop keep(Oop object) { assert(object && root_count<32); roots[root_count++]=object; return object; }
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
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    (void)id; assert(count==5);
    for(size_t i=0;i<count;i++) assert(args[i].kind==BIRNAM_WORD);
    api=(void *)args[0].data.word; imports=(void *)args[1].data.word;
    const RootRegistryV1 *registry=(void *)args[2].data.word;
    integer=*(IntegerApiV1 *)args[3].data.word; string=*(StringApiV1 *)args[4].data.word;
    assert(!namespace_import_v1(imports,"RustCodegen"));
    assert(!namespace_import_v1(imports,"TCC"));
    assert(!namespace_import_v1(imports,"Peg"));
    root_registry_register_v1(registry,mark);
    Oop block_proto=named("Block"), block_vt=named("BlockVt");
    assert(api->vtable_of(block_proto)==block_vt && vtable_owner(api,block_vt)==block_proto);
    Oop expected=text("native capture");
    Oop closure=native_block("value",capture,expected);
    roots[0]=NULL; /* The Block capture is now the only registered root path. */
    api->gc_collect();
    assert(send0(closure,"value")==expected);
    assert(send1(api->true_obj,"ifTrue:",closure)==expected);
    assert(send1(api->false_obj,"ifTrue:",closure)==api->nil_obj);
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
    Oop predicate=native_block("value",condition,api->nil_obj);
    Oop work=native_block("value",body,api->nil_obj); calls=0; limit=3;
    assert(send1(predicate,"whileTrue:",work)==api->nil_obj && calls==3);
    Oop invalid=native_block("value",bad_condition,api->nil_obj);
    Oop error=keep(send1(invalid,"whileTrue:",work));
    assert(send0(error,"isError")==api->true_obj);
    assert(strcmp(api->symbol_name(send0(error,"kind")),"mustBeBoolean")==0);
    Oop globals=named("Globals"), key=keep(api->intern("earlyNativeValue"));
    Oop binding[]={key,expected}; api->send(globals,api->intern("at:put:"),binding,2);
    assert(send1(globals,"at:",key)==expected);
    Oop machine=keep(send0(named("Machine"),"new"));
    assert(number(send0(machine,"liveFrameCount"))==0);
    api->gc_collect();
    assert(send0(closure,"value")==expected && send1(globals,"at:",key)==expected);
    return (BirnamOutcome){.value={.kind=BIRNAM_OBJECT,.data.object=block_proto}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
