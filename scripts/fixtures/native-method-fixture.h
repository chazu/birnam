#ifndef BIRNAM_NATIVE_METHOD_FIXTURE_H
#define BIRNAM_NATIVE_METHOD_FIXTURE_H
#include "native-fixture.h"
/* These fixtures retain only pinned native implementations, selected through
 * Id. Their calls run inside the caller's managed turn. No Method object is
 * interpreted as a C function address. */
static inline BirnamNativeEntry fixture_select_native(const BirnamProtocol *id,IdApi *api,Oop behavior,Oop selector) {
    BirnamOperand argument={.kind=BIRNAM_OBJECT,.data.object=selector};
    BirnamOutcome result=id->message(id->context,behavior,api->intern("lookup"),&argument,1);
    assert(result.status==BIRNAM_LOADED && result.value.kind==BIRNAM_NATIVE_ENTRY && result.value.data.entry.method);
    return result.value.data.entry;
}
static inline void fixture_install_native(const BirnamProtocol *id,IdApi *api,Oop behavior,Oop selector,BirnamNativeEntry entry) {
    BirnamOperand arguments[]={{.kind=BIRNAM_OBJECT,.data.object=selector},{.kind=BIRNAM_NATIVE_ENTRY,.data.entry=entry}};
    assert(id->message(id->context,behavior,api->intern("addMethod"),arguments,2).status==BIRNAM_LOADED);
}
static inline Oop fixture_invoke_native(IdApi *api,BirnamNativeEntry entry,Oop receiver,const Oop *args,size_t count) {
    assert(count<=2);BirnamOperand arguments[2];
    for(size_t i=0;i<count;i++)arguments[i]=(BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=args[i]};
    BirnamOutcome result=entry.method(entry.context,receiver,arguments,count);
    assert(result.status==BIRNAM_LOADED);
    if(result.value.kind==BIRNAM_UNIT)return api->nil_obj;
    assert(result.value.kind==BIRNAM_OBJECT);return result.value.data.object;
}
#endif
