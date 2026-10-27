/* Numeric loop observations and allocation identity checks through Oop messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[10],identity;static size_t alive;
enum{AT,PUT,ADD,COLLECT,STEP,WATCH,IDENTITY,ALLOCATIONS,ALLOCATION,ALIVE};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t name,const Oop*a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
/* [GC, observed Vector, answer] */
static Oop step(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=1)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));
    Oop values=send(get(c,1),ADD,a,1);CHECK(values);Oop update[]={id_size(1),values};CHECK(send(c,PUT,update,2));return get(c,2);
}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1)return id.invalid;identity=send(id.root,IDENTITY,a,1);CHECK(identity);return 0;
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(a[2]==identity)alive++;return 0;
}
static Oop remaining(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;alive=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(alive);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=10)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("add:"),LIT("collectPrecisely"),LIT("step"),LIT("watch"),
    LIT("identityOf:"),LIT("allocationsDo:"),LIT("allocation:bytes:identity:"),LIT("remaining")};
static const IdMethodSpec methods[]={{STEP,step},{WATCH,watch},{ALLOCATION,allocation},{ALIVE,remaining}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,10,methods,4,initialize};
