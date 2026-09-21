/* Test-only proxy for the production Native Loader constructor. */
#include "native-fixture.h"
#include <stdio.h>
static BirnamOutcome load(const BirnamProtocol *id,const char *path,const BirnamOperand *args,size_t count) {
    assert(count<=8);BirnamOperand request[9]={{.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)path,strlen(path)}}};
    for(size_t i=0;i<count;i++)request[i+1]=args[i];
    return id->message(id->context,id->receiver,id->load,request,count+1);
}
static BirnamOperand word(BirnamOutcome outcome) {
    assert(outcome.status==BIRNAM_LOADED);(void)fixture_word(outcome.value);return outcome.value;
}
static BirnamOperand service(const BirnamProtocol *id,const char *runtime,const char *name,BirnamOperand api) {
    BirnamOperand args[]={{.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)name,strlen(name)}},api};
    return word(load(id,runtime,args,2));
}
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    const char *original=getenv("BIRNAM_SERVICE_TEST_ORIGINAL");assert(original && count==8);
    BirnamOutcome result=load(id,original,args,count);
    assert(result.status==BIRNAM_LOADED && result.value.kind==BIRNAM_OBJECT);
    char runtime[4096],factory[4096];const char *slash=strrchr(original,'/');assert(slash);
    int length=snprintf(runtime,sizeof(runtime),"%.*s/libbirnam_runtime.dylib",(int)(slash-original),original);
    assert(length>0 && (size_t)length<sizeof(runtime));
    const char *kind=getenv("BIRNAM_SERVICE_TEST_KIND");assert(kind);
    BirnamOperand inputs[6]={args[0],args[4],service(id,runtime,"rootRegistry",args[0])};
    size_t inputs_count=3;
    if(!strcmp(kind,"deferred-index") || !strcmp(kind,"deferred-identity")) {
        inputs[3]=service(id,runtime,"gcSettings",args[0]);
        inputs[4]=service(id,runtime,"finalizers",args[0]);
        const char *directory=getenv("BIRNAM_LIB_DIR");assert(directory);
        length=snprintf(factory,sizeof(factory),"%s/libbirnam_storage.dylib",directory);
        assert(length>0 && (size_t)length<sizeof(factory));
        inputs[5]=(BirnamOperand){.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)factory,(size_t)length}};
        inputs_count=6;
    } else if(strcmp(kind,"root-registry")) {
        int stack=!strcmp(kind,"native-stack");assert(stack || !strcmp(kind,"gc-settings"));
        inputs[3]=service(id,runtime,stack?"nativeStack":"gcSettings",args[0]);
        length=snprintf(factory,sizeof(factory),"%.*s/libbirnam_%s.dylib",(int)(slash-original),original,stack?"native_stack":"gc_settings");
        assert(length>0 && (size_t)length<sizeof(factory));
        inputs[4]=word(load(id,factory,NULL,0));inputs_count=5;
    }
    const char *fixture=getenv("BIRNAM_SERVICE_TEST_FIXTURE");assert(fixture);
    assert(load(id,fixture,inputs,inputs_count).status==BIRNAM_LOADED);
    return result;
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
