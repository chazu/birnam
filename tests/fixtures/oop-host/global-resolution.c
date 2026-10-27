/* Resolver observations retain identities, never the temporary owners under test. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT,PUT,GC,IDENTITY,ALLOCATIONS,NS_GET,GLOBAL_GET,ROW,PRESENT,COUNT };
static Oop names[COUNT];static size_t wanted;static Oop found;
enum { Ns,Gc,Monitor,Key,NilName,GlobalsName,RealNil,RealGlobals,OldNilId,OldGlobalsId,Events,EventCount,Mutate,Active,FailKey,NsReply,GlobalReply,KeyFailure,NsFailure,GlobalFailure,Alias };
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop identity(Oop o){if(!o||((uintptr_t)o&1))return id_size(0);return send(id.root,IDENTITY,&o,1);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found=a[0];return 0;}
static Oop find(Oop c,Oop key){wanted=id_unbox(key);found=0;Oop monitor=get(c,Monitor);CHECK(send(id.root,ALLOCATIONS,&monitor,1));return found;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;Oop result=find(c,a[0]);CHECK(result);return id_size(result!=0);}
static Oop record(Oop c,Oop value){size_t index=id_unbox(get(c,EventCount));if(index>=256)return id.invalid;Oop key=identity(value);CHECK(key);CHECK(put(get(c,Events),index,key));return put(c,EventCount,id_size(index+1));}
static Oop namespace_get(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;CHECK(record(c,a[0]));
 if(a[0]==get(c,GlobalsName))switch(id_unbox(get(c,NsReply))){case 1:return 0;case 2:return id_size(7);case 3:return id_failure(get(c,NsFailure));}
 if(get(c,FailKey)==id_size(1)&&a[0]==get(c,Key))return id_failure(get(c,KeyFailure));
 return id.message(id.root,r,get(c,Alias),a,n);
}
static Oop globals_get(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1||a[0]!=get(c,Key))return id.invalid;CHECK(record(c,r));
 if(get(c,Mutate)==id_size(1)&&get(c,Active)==id_size(0)){
  CHECK(put(c,Active,id_size(1)));
  Oop nil[]={get(c,NilName),get(c,RealNil)},globals[]={get(c,GlobalsName),get(c,RealGlobals)};
  CHECK(send(get(c,Ns),PUT,nil,2));CHECK(send(get(c,Ns),PUT,globals,2));CHECK(send(get(c,Gc),GC,0,0));
  Oop old_nil=find(c,get(c,OldNilId));CHECK(old_nil);if(!old_nil)return id.invalid;
  Oop old_globals=find(c,get(c,OldGlobalsId));CHECK(old_globals);if(!old_globals)return id.invalid;
  CHECK(put(c,Active,id_size(0)));
 }
 switch(id_unbox(get(c,GlobalReply))){case 1:return id_failure(get(c,GlobalFailure));case 2:return id_size(9);}
 Oop nil=find(c,get(c,OldNilId));CHECK(nil);return nil?nil:id.invalid;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("namespaceGet"),LIT("globalsGet"),LIT("allocation:bytes:identity:"),LIT("present:")};
static const IdMethodSpec methods[]={{NS_GET,namespace_get},{GLOBAL_GET,globals_get},{ROW,row},{PRESENT,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,4,initialize};
