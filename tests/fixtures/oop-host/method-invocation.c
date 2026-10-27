/* Method construction callbacks use declared Oop Methods and weak identities. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[11], identities[32];
static size_t watched, live;
enum {AT,PUT,COLLECT,DEFINE,IDENTITY,ALLOCATIONS,ALLOCATION,PROBE,REENTER,WATCH,REMAINING};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop key=id_size(i);return send(c,AT,&key,1);}
static Oop increment(Oop c,size_t i){Oop pair[]={id_size(i),id_size(id_unbox(get(c,i))+1)};return send(c,PUT,pair,2);}
/* [GC,answer,calls,failure-or-null] */
static Oop probe(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;(void)n;CHECK(increment(c,2));CHECK(send(get(c,0),COLLECT,0,0));
    Oop failure=get(c,3);return failure?id_failure(failure):get(c,1);
}
/* [original selector alias,Reflect,target,selector,body,GC,calls] */
static Oop reenter(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;Oop before=get(c,6);CHECK(increment(c,6));
    if(before==id_size(0)){
        Oop nested[]={get(c,3),get(c,2),get(c,4)};CHECK(send(get(c,1),DEFINE,nested,3));
        CHECK(send(get(c,5),COLLECT,0,0));
    }
    return id.message(id.root,r,get(c,0),a,n);
}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n>32)return id.invalid;watched=n;
    for(size_t i=0;i<n;i++){identities[i]=send(id.root,IDENTITY,&a[i],1);CHECK(identities[i]);}return 0;
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<watched;i++)if(a[2]==identities[i])live++;return 0;
}
static Oop remaining(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    live=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(live);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=11)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("defineMethod:on:as:"),
    LIT("identityOf:"),LIT("allocationsDo:"),LIT("allocation:bytes:identity:"),LIT("probe"),LIT("reenter"),LIT("watch"),LIT("remaining")};
static const IdMethodSpec methods[]={{ALLOCATION,allocation},{PROBE,probe},{REENTER,reenter},{WATCH,watch},{REMAINING,remaining}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,11,methods,5,initialize};
