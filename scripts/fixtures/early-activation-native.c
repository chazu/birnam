/* Production activation and saved collecting constructors, without Runtime. */
#include "../../abi/fixtures/collection-state-v2/collection_state_v2.h"
#include "../../abi/fixtures/lifetime-v1/lifetime_v1.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static CollectionStateV2 owner;
static BootstrapOwnedV2 bootstrap;
static RootRegistryOwnedV1 roots;
static LifetimeOwnedV1 lifetime;
static void *held[8],*size_selector;
static IntegerApiV1 *integer;
static size_t visits;
static int verify_initialized;
static void initialized_hook(void *object,void (*mark)(void *)) {
    (void)mark;if(verify_initialized)assert(((uintptr_t *)object)[1]==0x83);
}
static BirnamNativeEntry allocation,delegation,interning,cloning;
static BirnamOperand oop(void *p) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=p}; }
static BirnamOperand word(size_t n) { return (BirnamOperand){.kind=BIRNAM_WORD,.data.word=n}; }
static BirnamOutcome send(void *receiver,void *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome out=id.message(id.context,receiver,selector,args,count);assert(out.status==BIRNAM_LOADED);return out;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static int allocated(void *p) { BirnamOperand arg=oop(p);return send(id.receiver,size_selector,&arg,1).value.kind==BIRNAM_WORD; }
static void provider(void (*mark)(void *)) { visits++;for(size_t i=0;i<8;i++)if(held[i])mark(held[i]); }
static BirnamNativeEntry select(void *receiver,void *selector) {
    BirnamOperand arg=oop(selector);BirnamOutcome out=send(receiver,name("lookup"),&arg,1);
    assert(out.value.kind==BIRNAM_NATIVE_ENTRY);return out.value.data.entry;
}
static BirnamOutcome forbidden(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;(void)count;assert(!"nested pause entered its action");return (BirnamOutcome){0};
}
static BirnamOutcome paused(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;(void)count;
    assert(bootstrap.pause(bootstrap.context,(BirnamNativeEntry){forbidden,NULL}).status==BIRNAM_INVALID_REQUEST);
    assert(bootstrap.load(bootstrap.context,(const uint8_t *)"unused",6,NULL,0).status==BIRNAM_INVALID_REQUEST);
    assert(bootstrap.release_result(bootstrap.context,oop(held[1]))==0);
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static BirnamOutcome save(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;assert(count==0 && owner.collector->active(owner.collector->context)==1);
    allocation=select(id.receiver,name("allocate"));delegation=select(id.receiver,name("delegated"));
    interning=select(id.receiver,id.intern);cloning=select(held[0],name("clone"));
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static void *construct(BirnamNativeEntry entry,void *receiver,const BirnamOperand *args,size_t count) {
    owner.budget_factory->replace(owner.budget_context,16u<<20);
    size_t before=visits;
    BirnamOutcome out=entry.method(entry.context,receiver,args,count);
    assert(out.status==BIRNAM_LOADED && out.value.kind==BIRNAM_OBJECT && visits>before);
    assert(allocated(out.value.data.object));
    assert(owner.budget_factory->read(owner.budget_context)<(16u<<20));
    return out.value.data.object;
}
static BirnamOutcome exercise(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;assert(count==0);
    assert(owner.collector->active(owner.collector->context)==1);
    owner.settings->set_precise(owner.settings->context,1);
    BirnamOperand size=word(16);
    /* During this collection only the previously completed instance exists.
       A post-allocation collection would incorrectly trace zeroed new bytes. */
    trace_hooks_set_v1(lifetime.hooks,held[0],initialized_hook);
    if(held[2])((uintptr_t *)held[2])[1]=0x83;
    verify_initialized=1;
    held[2]=construct(allocation,held[0],&size,1);
    ((uintptr_t *)held[2])[1]=0x83;
    verify_initialized=0;
    lifetime.replace_hook(lifetime.context,held[0],(TraceHookSlotV1){0});
    held[3]=construct(delegation,held[0],NULL,0);
    static unsigned run;
    const char *text=run++ ? "activationInternAfterRuntime" : "activationInternBeforeRuntime";
    BirnamOperand bytes={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    held[4]=construct(interning,id.receiver,&bytes,1);
    owner.budget_factory->replace(owner.budget_context,16u<<20);
    size_t before=visits;
    BirnamOutcome existing=interning.method(interning.context,id.receiver,&bytes,1);
    assert(existing.status==BIRNAM_LOADED && existing.value.data.object==held[4] && visits==before);
    void *copy=construct(cloning,held[1],NULL,0);
    assert(copy!=held[1] && ((uintptr_t *)copy)[0]==((uintptr_t *)held[1])[0] && ((uintptr_t *)copy)[1]==0x83);
    assert(!lifetime.finalizer(lifetime.context,copy).callback);
    owner.budget_factory->replace(owner.budget_context,16u<<20);before=visits;
    held[5]=integer->make(integer->context,INT64_MAX);
    assert(visits>before && integer->read(integer->context,held[5])==INT64_MAX);
    held[6]=integer->make(integer->context,1);
    void *plus=name("+");BirnamOperand one=oop(held[6]);
    owner.budget_factory->replace(owner.budget_context,16u<<20);before=visits;
    held[7]=send(held[5],plus,&one,1).value.data.object;
    assert(visits>before && send(held[7],name("kind"),NULL,0).value.data.object==name("overflow"));
    assert(bootstrap.pause(bootstrap.context,(BirnamNativeEntry){paused,NULL}).status==BIRNAM_LOADED);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=1}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==6);id=*protocol;owner=*(CollectionStateV2 *)args[0].data.word;
    bootstrap=*(BootstrapOwnedV2 *)args[1].data.word;roots=*(RootRegistryOwnedV1 *)args[2].data.word;
    lifetime=*(LifetimeOwnedV1 *)args[3].data.word;integer=(IntegerApiV1 *)args[5].data.word;
    assert(!owner.collector->active(owner.collector->context));
    assert(bootstrap.pause(bootstrap.context,(BirnamNativeEntry){forbidden,NULL}).status==BIRNAM_INVALID_REQUEST);
    root_registry_register_v1(roots.api,provider); /* Before populating native ownership. */
    size_selector=name("allocationSizeOf:");
    BirnamOutcome prototype=send(id.receiver,name("objectPrototype"),NULL,0);
    void *base=send(id.receiver,name("behaviorOf:"),&prototype.value,1).value.data.object;
    held[0]=send(base,name("delegated"),NULL,0).value.data.object;
    BirnamOperand size=word(16);held[1]=send(held[0],name("allocate"),&size,1).value.data.object;
    ((uintptr_t *)held[1])[0]=(uintptr_t)prototype.value.data.object;((uintptr_t *)held[1])[1]=0x83;
    if(!args[4].data.word) { uint8_t fields[]={1,0};payload_layout_set_v1(lifetime.layouts,held[0],fields,2,0); }
    const char *names[]={"saveActivationConstructors","exerciseActivationConstructors"};
    BirnamNativeMethod methods[]={save,exercise};
    for(size_t i=0;i<2;i++) {
        BirnamOperand binding[]={oop(name(names[i])),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={methods[i],NULL}}};
        send(id.receiver,name("addMethod"),binding,2);
    }
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
