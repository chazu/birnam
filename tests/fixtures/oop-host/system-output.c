/* Output callbacks preserve failures and observe call lifetimes through Id. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[12], identities[2];
static size_t live;
enum {AT,PUT,COLLECT,PRINT,FROM,IDENTITY,ALLOCATIONS,ALLOCATION,WATCH,FORWARD,RENDER,MASK};
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked))return checked; } while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
/* [GC, original selector alias, calls, failure-or-null] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;
    Oop pair[]={id_size(2),id_size(id_unbox(get(c,2))+1)};CHECK(send(c,PUT,pair,2));
    CHECK(send(get(c,0),COLLECT,0,0));
    Oop failure=get(c,3);if(failure)return id_failure(failure);
    return id.message(id.root,r,get(c,1),a,n);
}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=2)return id.invalid;
    for(size_t i=0;i<2;i++){identities[i]=send(id.root,IDENTITY,&a[i],1);CHECK(identities[i]);}return 0;
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<2;i++)if(identities[i]==a[2])live|=1<<i;return 0;
}
static Oop mask(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    live=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(live);
}
/* [GC,System,String,prefix,outer byte provider,allocation visitor].
   The fixture does not retain either transient output receiver or printable value. */
static Oop render(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    CHECK(send(get(c,0),COLLECT,0,0));
    Oop alive=mask(id.root,get(c,5),0,0,0);CHECK(alive);
    if(alive!=id_size(3))return id.invalid;
    Oop prefix=get(c,3);CHECK(send(get(c,1),PRINT,&prefix,1));
    /* No fixture root for the returned String: output must protect it before bytes. */
    Oop provider=get(c,4);return send(get(c,2),FROM,&provider,1);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=12)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("print:"),LIT("fromBytes:"),
    LIT("identityOf:"),LIT("allocationsDo:"),LIT("allocation:bytes:identity:"),LIT("watch"),LIT("forward"),LIT("render"),LIT("mask")};
static const IdMethodSpec methods[]={{ALLOCATION,allocation},{WATCH,watch},{FORWARD,forward},{RENDER,render},{MASK,mask}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,12,methods,5,initialize};
