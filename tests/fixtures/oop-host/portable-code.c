/* Portable owner callbacks and weak allocation observations through Oop messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[8],identities[16];static size_t watched,live;
enum{AT,PUT,COLLECT,IDENTITY,ALLOCATIONS,FORWARD,WATCH,REMAINING};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
/* [GC,original selector alias,calls,failure-or-null] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;Oop pair[]={id_size(2),id_size(id_unbox(get(c,2))+1)};CHECK(send(c,PUT,pair,2));
    Oop failure=get(c,3);if(failure)return id_failure(failure);
    CHECK(send(get(c,0),COLLECT,0,0));return id.message(id.root,r,get(c,1),a,n);
}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n>16)return id.invalid;watched=n;
    for(size_t i=0;i<n;i++){identities[i]=send(id.root,IDENTITY,&a[i],1);CHECK(identities[i]);}return 0;
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<watched;i++)if(identities[i]==a[2])live++;return 0;
}
static Oop remaining(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;live=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(live);
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("forward"),LIT("watch"),LIT("remaining"),LIT("allocation:bytes:identity:")};
static const IdMethodSpec methods[]={{FORWARD,forward},{WATCH,watch},{REMAINING,remaining},{8,allocation}};
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){
    (void)a;if(n!=9||count)return p->invalid;id=*p;for(size_t i=0;i<8;i++)names[i]=s[i];return image;
}
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,9,methods,4,initialize};
