/* Compiled by the real TCC owner; independent native ABI declaration. */
#include "id_protocol.h"
#include <string.h>
static BirnamProtocol id;
static BirnamObject ns, baseline;
static BirnamOperand obj(BirnamObject o){BirnamOperand a={0};a.kind=BIRNAM_OBJECT;a.data.object=o;return a;}
static BirnamOperand word(size_t w){BirnamOperand a={0};a.kind=BIRNAM_WORD;a.data.word=w;return a;}
static BirnamOutcome send(BirnamObject r,const char*n,const BirnamOperand*a,size_t count){
 BirnamOperand b={0};b.kind=BIRNAM_BYTES;b.data.bytes.pointer=(const uint8_t*)n;b.data.bytes.length=strlen(n);
 BirnamOutcome selector=id.message(id.context,id.receiver,id.intern,&b,1);
 if(selector.status)return selector;
 return id.message(id.context,r,selector.value.data.object,a,count);
}
static BirnamObject named(const char*n){BirnamOperand b={0};b.kind=BIRNAM_BYTES;b.data.bytes.pointer=(const uint8_t*)n;b.data.bytes.length=strlen(n);BirnamOutcome s=id.message(id.context,id.receiver,id.intern,&b,1);BirnamOperand a=s.value;return send(ns,"at:",&a,1).value.data.object;}
static BirnamObject symbol(const char*n){BirnamOperand b={0};b.kind=BIRNAM_BYTES;b.data.bytes.pointer=(const uint8_t*)n;b.data.bytes.length=strlen(n);return id.message(id.context,id.receiver,id.intern,&b,1).value.data.object;}
static BirnamOutcome recover(void*ctx,BirnamObject machine,const BirnamOperand*a,size_t n){
 (void)ctx;BirnamOutcome bad={0};bad.status=BIRNAM_INVALID_REQUEST;if(n!=1)return bad;
 BirnamObject roots=named("Roots"),vector=send(named("Vector"),"new",0,0).value.data.object;
 BirnamOperand v=obj(vector);BirnamOperand pins[4];pins[0]=send(roots,"retain:",&v,1).value;
 vector=send(vector,"add:",a,1).value.data.object;v=obj(vector);pins[1]=send(roots,"retain:",&v,1).value;
 BirnamOperand nil=obj(named("nil"));vector=send(vector,"add:",&nil,1).value.data.object;v=obj(vector);pins[2]=send(roots,"retain:",&v,1).value;
 BirnamOperand z=word(0);BirnamObject zero=send(named("Integer"),"fromWord:",&z,1).value.data.object;v=obj(zero);pins[3]=send(roots,"retain:",&v,1).value;
 BirnamOperand recipe[2]={obj(zero),obj(vector)};BirnamOutcome result=send(machine,"recoverAt:with:",recipe,2);if(result.status){result.detail=101;return result;}
 /* Remove the last binding to the entered candidate, then collect and try the
    retirement checkpoint before executing more instructions in this mapping. */
 BirnamOutcome behavior=send(id.receiver,"behaviorOf:",a,1);BirnamOperand replacement[2]={obj(baseline),obj(symbol("value"))};
 result=send(behavior.value.data.object,"addMethod:as:",replacement,2);if(result.status){result.detail=102;return result;}
 result=send(named("GC"),"collectPrecisely",0,0);if(result.status){result.detail=103;return result;}
 result=send(named("TCC"),"checkpoint",0,0);if(result.status){result.detail=104;return result;}
 BirnamOperand marker[2]={obj(symbol("Recovered")),obj(named("true"))};result=send(ns,"at:put:",marker,2);if(result.status){result.detail=105;return result;}
 for(size_t i=4;i>0;i--)send(roots,"release:",&pins[i-1],1);
 BirnamOutcome declined={0};return declined;
}
static BirnamOutcome initialize(const BirnamProtocol*p,const BirnamOperand*a,size_t n){
 BirnamOutcome bad={0};bad.status=BIRNAM_INVALID_REQUEST;if(!p||n!=3)return bad;id=*p;ns=a[0].data.object;baseline=a[2].data.object;
 BirnamOutcome retained=send(a[1].data.object,"retain:",&a[2],1);if(retained.status)return retained;
 BirnamOperand z=word(0);BirnamOutcome zero=send(named("Integer"),"fromWord:",&z,1);if(zero.status)return zero;
 BirnamOperand e={0};e.kind=BIRNAM_NATIVE_ENTRY;e.data.entry.method=recover;
 BirnamOperand args[3]={e,zero.value,a[1]};return send(baseline,"withInline:at:owner:",args,3);
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
