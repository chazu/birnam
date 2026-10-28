/* Observe Word-owner allocations and collecting permission through Id messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum {AT,PUT,BEHAVIOR,ALLOCATIONS,GC,ADD,XOR,PERMIT_ALIAS,ALLOCATE_ALIAS,
      ROW,WORDS,PERMIT,ALLOCATE,COUNT};
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
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=COUNT||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("behaviorOf:"),LIT("allocationsDo:"),LIT("collectPrecisely"),LIT("addMethod"),LIT("bitXor:"),LIT("expressionOriginalPermission:"),LIT("expressionOriginalAllocation:bytes:"),LIT("allocation:bytes:identity:"),LIT("words"),LIT("permit"),LIT("allocation")};
static const IdMethodSpec methods[]={{ROW,row},{WORDS,inventory},{PERMIT,permit},{ALLOCATE,allocation}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,4,initialize};
