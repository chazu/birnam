/* A production-prefix module whose object is held only by the early root owner. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include <assert.h>
#include <string.h>

static BirnamProtocol id;
static RootRegistryOwnedV1 owner;
static BirnamObject object, identity;
static size_t token;
static BirnamOutcome send(BirnamObject receiver, BirnamObject selector, const BirnamOperand *args, size_t count) {
    BirnamOutcome answer=id.message(id.context,receiver,selector,args,count);
    assert(answer.status==BIRNAM_LOADED);
    return answer;
}
static BirnamObject name(const char *text) {
    BirnamOperand bytes={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    BirnamOutcome answer=send(id.receiver,id.intern,&bytes,1);
    assert(answer.value.kind==BIRNAM_OBJECT);
    return answer.value.data.object;
}
static BirnamOutcome query(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args;
    assert(count==0);
    BirnamOperand candidate={.kind=BIRNAM_OBJECT,.data.object=object};
    return send(id.receiver,identity,&candidate,1);
}
static BirnamOutcome release_root(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args;
    assert(count==0);
    int removed=owner.release(owner.context,token);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=(size_t)removed}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol, const BirnamOperand *args, size_t count) {
    assert(count==1 && args[0].kind==BIRNAM_WORD);
    id=*protocol;
    owner=*(const RootRegistryOwnedV1 *)args[0].data.word;
    assert(owner.version==1 && owner.byte_size==sizeof(owner));
    BirnamObject prototype=name("objectPrototype"), behavior_of=name("behaviorOf:");
    BirnamObject allocate=name("allocate"), add=name("addMethod");
    identity=name("identityOf:");
    BirnamOutcome answer=send(id.receiver,prototype,NULL,0);
    answer=send(id.receiver,behavior_of,&answer.value,1);
    BirnamOperand size={.kind=BIRNAM_WORD,.data.word=0};
    answer=send(answer.value.data.object,allocate,&size,1);
    assert(answer.value.kind==BIRNAM_OBJECT);
    /* Register ownership before publishing this object in native module state. */
    token=owner.retain(owner.context,answer.value.data.object);
    assert(token!=0);
    object=answer.value.data.object;
    size_t baseline=owner.roots(owner.context).count, previous=token;
    for (size_t i=0;i<1000;i++) {
        size_t temporary=owner.retain(owner.context,object);
        assert(temporary>previous);
        assert(owner.release(owner.context,temporary)==1);
        assert(owner.release(owner.context,temporary)==0);
        assert(owner.roots(owner.context).count==baseline);
        previous=temporary;
    }
    const char *names[]={"earlyRootIdentity","releaseEarlyRoot"};
    BirnamNativeMethod methods[]={query,release_root};
    for (size_t i=0;i<2;i++) {
        BirnamOperand install[]={ {.kind=BIRNAM_OBJECT,.data.object=name(names[i])},
            {.kind=BIRNAM_NATIVE_ENTRY,.data.entry={methods[i],NULL}} };
        send(id.receiver,add,install,2);
    }
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=token}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
