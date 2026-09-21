/* Malform one result of the real native entry; all other requests stay real. */
#include "../../abi/fixtures/runtime-support-v1/runtime_support_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define BEGIN_TABLE(Type) \
    static Type copy; copy=*(const Type *)answer.value.data.word; \
    answer.value.data.word=(size_t)&copy; \
    if(!strcmp(fault,"version")) { copy.version++; return answer; } \
    if(!strcmp(fault,"size")) { copy.byte_size--; return answer; }
#define NULL_OPERATION(field) \
    if(!strcmp(fault,#field)) { copy.field=NULL; return answer; }

static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *args, size_t count) {
    void *image=dlopen(getenv("BIRNAM_SUPPORT_ORIGINAL"),RTLD_NOW|RTLD_LOCAL);
    assert(image);
    const BirnamModule *module=dlsym(image,"birnam_native_module_v1");
    assert(module && module->initialize);
    BirnamOutcome answer=module->initialize(id,args,count);
    const char *request=getenv("BIRNAM_SUPPORT_REQUEST"), *fault=getenv("BIRNAM_SUPPORT_FAULT");
    if(count!=1 || args[0].kind!=BIRNAM_BYTES || args[0].data.bytes.length!=strlen(request) ||
       memcmp(args[0].data.bytes.pointer,request,strlen(request))) return answer;
    assert(answer.status==BIRNAM_LOADED && answer.value.kind==BIRNAM_WORD && answer.value.data.word);
    FILE *receipt=fopen(getenv("BIRNAM_SUPPORT_REACHED"),"w");
    assert(receipt); assert(fputs(request,receipt)>=0); assert(!fclose(receipt));
    if(!strcmp(fault,"null")) { answer.value.data.word=0; return answer; }
    if(!strcmp(fault,"kind")) { answer.value.kind=BIRNAM_UNIT; return answer; }
    if(!strcmp(fault,"failure")) return (BirnamOutcome){.status=BIRNAM_INITIALIZER_FAILED};
    if(!strcmp(request,"methodObservation")) {
        BEGIN_TABLE(MethodObservationFactoryV1)
        NULL_OPERATION(create) NULL_OPERATION(destroy) NULL_OPERATION(notify)
    } else if(!strcmp(request,"observation")) {
        BEGIN_TABLE(StorageObservationFactoryV1)
        NULL_OPERATION(create) NULL_OPERATION(destroy) NULL_OPERATION(read)
        NULL_OPERATION(set_active) NULL_OPERATION(set_permit) NULL_OPERATION(advance_epoch)
    } else if(!strcmp(request,"deferredIdentity")) {
        BEGIN_TABLE(DeferredIdentityFactoryV1)
        NULL_OPERATION(create) NULL_OPERATION(destroy) NULL_OPERATION(read) NULL_OPERATION(write)
    } else if(!strcmp(request,"deferredIndex")) {
        BEGIN_TABLE(DeferredIndexFactoryV1)
        NULL_OPERATION(destroy) NULL_OPERATION(len) NULL_OPERATION(capacity) NULL_OPERATION(push)
        NULL_OPERATION(read) NULL_OPERATION(write) NULL_OPERATION(swap_remove) NULL_OPERATION(clear)
    } else if(!strcmp(request,"dispatchCache")) {
        BEGIN_TABLE(DispatchCacheFactoryV2)
        NULL_OPERATION(create) NULL_OPERATION(destroy) NULL_OPERATION(lookup)
        NULL_OPERATION(insert) NULL_OPERATION(clear)
    } else if(!strcmp(request,"guardVerdict")) {
        BEGIN_TABLE(GuardVerdictFactoryV1)
        NULL_OPERATION(create) NULL_OPERATION(destroy) NULL_OPERATION(read) NULL_OPERATION(write)
    }
    abort();
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
