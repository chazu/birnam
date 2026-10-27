/* Artifact policy probes use only Oop contexts and current Id messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[16], identities[2];
static size_t live;
enum {AT,PUT,COLLECT,CHECKPOINT,CANCEL,DEFINE,PROBE,SELECT,REENTER,CONSTRUCTOR,DEMAND,IDENTITY,ALLOCATIONS,WATCH,REMAINING,ALLOCATION};
#define CHECK(v) do { Oop checked=(v);if(id_fault(checked))return checked; }while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
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
/* [GC,first template,second template,distinct,calls] */
static Oop select_template(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n!=2)return id.invalid;
    Oop before=get(c,4);CHECK(increment(c,4));CHECK(send(get(c,0),COLLECT,0,0));
    return get(c,(get(c,3)==id_size(1)&&before!=id_size(0))?2:1);
}
/* [alias,GC,calls] */
static Oop constructor(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;CHECK(increment(c,2));CHECK(send(get(c,1),COLLECT,0,0));
    return id.message(id.root,r,get(c,0),a,n);
}
/* [alias,GC,TCC,Codegen,calls,answer-or-null]. Demand callbacks cannot cancel the active request. */
static Oop demand(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;CHECK(increment(c,4));CHECK(send(get(c,1),COLLECT,0,0));CHECK(send(get(c,2),CHECKPOINT,0,0));
    if(!id_fault(send(get(c,3),CANCEL,0,0)))return id.invalid;
    Oop answer=get(c,5);return answer?answer:id.message(id.root,r,get(c,0),a,n);
}
/* Weak allocation identities, not roots for the selected templates. */
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=2)return id.invalid;
    for(size_t i=0;i<2;i++){identities[i]=send(id.root,IDENTITY,&a[i],1);CHECK(identities[i]);}return 0;
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<2;i++)if(identities[i]==a[2])live++;return 0;
}
static Oop remaining(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    live=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(live);
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("checkpoint"),LIT("cancelPortableImageUnit"),LIT("defineMethod:on:as:"),LIT("probe"),LIT("selectTemplate"),LIT("reenter"),LIT("constructor"),LIT("demand"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("watch"),LIT("remaining"),LIT("allocation:bytes:identity:")};
static const IdMethodSpec methods[]={{PROBE,probe},{SELECT,select_template},{REENTER,reenter},{CONSTRUCTOR,constructor},{DEMAND,demand},{WATCH,watch},{REMAINING,remaining},{ALLOCATION,allocation}};
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=16)return p->invalid;id=*p;for(size_t i=0;i<16;i++)names[i]=symbols[i];return image;
}
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,16,methods,8,initialize};
