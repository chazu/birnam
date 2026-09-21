/* Real nested native loading; only Bootstrap retains the returned Object. */
#include "../../abi/fixtures/bootstrap-v1/bootstrap_v1.h"
#include <assert.h>
#include <string.h>
#ifndef MODE
#define MODE 0
#endif
static BirnamProtocol id;
static BirnamOutcome send(void *receiver,void *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome answer=id.message(id.context,receiver,selector,args,count);
    assert(answer.status==BIRNAM_LOADED); return answer;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
#if MODE == 1
static BootstrapOwnedV1 bootstrap;
static BirnamOperand result;
static BirnamOutcome query(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    return send(id.receiver,name("identityOf:"),&result,1);
}
static BirnamOutcome release(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=bootstrap.release_result(bootstrap.context,result)}};
}
#endif
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    id=*protocol;
#if MODE == 0
    (void)args; assert(count==0);
    BirnamOutcome prototype=send(id.receiver,name("objectPrototype"),NULL,0);
    void *behavior=send(id.receiver,name("behaviorOf:"),&prototype.value,1).value.data.object;
    BirnamOperand size={.kind=BIRNAM_WORD,.data.word=0};
    return send(behavior,name("allocate"),&size,1);
#else
    assert(count==3 && args[0].kind==BIRNAM_WORD && args[1].kind==BIRNAM_BYTES && args[2].kind==BIRNAM_BYTES);
    bootstrap=*(const BootstrapOwnedV1 *)args[0].data.word;
    assert(bootstrap.version==1 && bootstrap.byte_size==sizeof(bootstrap));
    /* Exercise Protocol retention/release using the real Id, not a test Id. */
    BirnamOutcome same_id=bootstrap.load(bootstrap.context,args[2].data.bytes.pointer,args[2].data.bytes.length,NULL,0);
    assert(same_id.status==BIRNAM_LOADED && same_id.value.kind==BIRNAM_PROTOCOL);
    assert(same_id.value.data.protocol->context==id.context);
    assert(bootstrap.release_result(bootstrap.context,same_id.value)==1);
    assert(bootstrap.release_result(bootstrap.context,same_id.value)==0);
    BirnamOutcome answer=bootstrap.load(bootstrap.context,args[1].data.bytes.pointer,args[1].data.bytes.length,NULL,0);
    assert(answer.status==BIRNAM_LOADED && answer.value.kind==BIRNAM_OBJECT);
    /* Bootstrap already retained this result before native state uses it. */
    result=answer.value;
    const char *names[]={"bootstrapResultIdentity","releaseBootstrapResult"};
    BirnamNativeMethod methods[]={query,release};
    for (size_t i=0;i<2;i++) {
        BirnamOperand install[]={ {.kind=BIRNAM_OBJECT,.data.object=name(names[i])},
            {.kind=BIRNAM_NATIVE_ENTRY,.data.entry={methods[i],NULL}} };
        send(id.receiver,name("addMethod"),install,2);
    }
    return query(NULL,NULL,NULL,0);
#endif
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
