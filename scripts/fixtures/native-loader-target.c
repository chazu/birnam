#include "../../rust/birnam-abi/tests/fixtures/id-protocol-v1/id_protocol.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef NATIVE_LOADER_NO_INIT
int not_a_module(void) { return 1; }
#else
static BirnamProtocol carrier;
static unsigned calls;
static BirnamOutcome intern(const char *name) {
    BirnamOperand argument={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)name,strlen(name)}};
    return carrier.message(carrier.context,carrier.receiver,carrier.intern,&argument,1);
}
static BirnamOutcome send(BirnamObject receiver,const char *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome symbol=intern(selector);assert(symbol.status==BIRNAM_LOADED && symbol.value.kind==BIRNAM_OBJECT);
    return carrier.message(carrier.context,receiver,symbol.value.data.object,args,count);
}
static BirnamOutcome count_loads(void *context,BirnamObject self,const BirnamOperand *args,size_t count) {
    (void)context;(void)self;(void)args;assert(!count);
    char value[64];snprintf(value,sizeof(value),"native-load-%u",calls);return intern(value);
}
/* Neither dependency discovery nor the retired initializer may be invoked. */
int birnam_native_loader_dependency_v12(const void *provider) { (void)provider;abort(); }
void *module_init(void) { abort(); }
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    assert(id && count==1 && args[0].kind==BIRNAM_OBJECT);
    if(calls) assert(id->context==carrier.context);
    carrier=*id;
    BirnamObject system=args[0].data.object;
    BirnamOutcome behavior=send(carrier.receiver,"behaviorOf:",args,1);
    assert(behavior.status==BIRNAM_LOADED && behavior.value.kind==BIRNAM_OBJECT);
    assert(send(system,"gc",NULL,0).status==BIRNAM_LOADED);
    calls++;
    BirnamOutcome selector=intern("nativeLoaderCount");assert(selector.status==BIRNAM_LOADED);
    BirnamOperand install[]={selector.value,{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={count_loads,NULL}}};
    assert(send(behavior.value.data.object,"addMethod",install,2).status==BIRNAM_LOADED);
    return (BirnamOutcome){0};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
#endif
