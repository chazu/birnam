/* Observe Word-owner allocations and collecting permission through Id messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum {AT,PUT,BEHAVIOR,ALLOCATIONS,GC,ADD,XOR,PERMIT_ALIAS,ALLOCATE_ALIAS,
      ROW,WORDS,PERMIT,ALLOCATE,BIND,SAME,SLOT_PUT,WRAP_ADD,MUTABLE_MATCH,ALLOCATOR_MATCH,COUNT};
static Oop names[COUNT];static size_t words;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
/* [WordVt, GC, allow, replace primitive, replacement, permission calls, allocations]. */
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=3)return id.invalid;
 Oop behavior=send(id.root,BEHAVIOR,a,1);CHECK(behavior);if(behavior==get(c,0))words++;return 0;
}
static Oop inventory(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)a;if(n)return id.invalid;words=0;CHECK(send(id.root,ALLOCATIONS,&r,1));return id_size(words);
}
static Oop permit(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;CHECK(put(c,5,id_size(id_unbox(get(c,5))+1)));
 Oop answer=send(r,PERMIT_ALIAS,a,n);CHECK(answer);CHECK(send(get(c,1),GC,0,0));
 if(id_unbox(get(c,3))){Oop change[]={names[XOR],get(c,4)};CHECK(send(get(c,0),ADD,change,2));}
 return id_unbox(get(c,2))?answer:id_size(0);
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=2)return id.invalid;
 if(a[0]==get(c,0))CHECK(put(c,6,id_size(id_unbox(get(c,6))+1)));
 return send(r,ALLOCATE_ALIAS,a,n);
}
/* [ObjectModel, mutable XOR Method, replacement context]. Accept arbitrary
   binding batches, including the allocator's own admission checks. */
static Oop matching(Oop c,const Oop*a,size_t n,int allocator){
 if(n%3)return id.invalid;
 for(size_t i=0;i<n;i+=3){
  if(a[i+1]==names[WRAP_ADD]){
   if(allocator){for(size_t j=0;j<3;j++){Oop edit[]={id_size(j),get(c,1),get(c,j+2)};CHECK(send(get(c,0),SLOT_PUT,edit,3));}}
   else {Oop edit[]={id_size(1),get(c,1),get(c,2)};CHECK(send(get(c,0),SLOT_PUT,edit,3));}
  }
  Oop request[]={a[i+1],a[i]},selected=send(id.root,BIND,request,2);CHECK(selected);
  Oop pair[]={selected,a[i+2]},matches=send(id.root,SAME,pair,2);CHECK(matches);
  if(matches!=id_size(1))return id_size(0);
 }
 return id_size(1);
}
static Oop mutable_match(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return matching(c,a,n,0);}
/* [ObjectModel, mutable allocator, new entry, new context, new code owner]. */
static Oop allocator_match(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return matching(c,a,n,1);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=COUNT||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("behaviorOf:"),LIT("allocationsDo:"),LIT("collectPrecisely"),LIT("addMethod"),LIT("bitXor:"),LIT("expressionOriginalPermission:"),LIT("expressionOriginalAllocation:bytes:"),LIT("allocation:bytes:identity:"),LIT("words"),LIT("permit"),LIT("allocation"),LIT("bind:to:"),LIT("sameNativeMethod:as:"),LIT("slotAt:in:put:"),LIT("wrappingAdd:"),LIT("mutableMatch"),LIT("allocatorMatch")};
static const IdMethodSpec methods[]={{ROW,row},{WORDS,inventory},{PERMIT,permit},{ALLOCATE,allocation},{MUTABLE_MATCH,mutable_match},{ALLOCATOR_MATCH,allocator_match}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,6,initialize};
