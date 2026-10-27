/* Reentrant edits and failures use declared Methods and ordinary Oop state. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[13],identities[2];static size_t remaining;
enum { AT,PUT,SEQ,INVOKE,SLOT_PUT,COLLECT,CAS,REMOVE,WATCH,IDENTITY,ALLOCATIONS,ALLOCATION,COUNT };
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked))return checked; } while(0)
static Oop send(Oop r,size_t name,const Oop *a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop get(Oop c,size_t i){Oop a=id_size(i);return send(c,AT,&a,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop invoke(Oop method,Oop receiver,const Oop *a,size_t n){
    Oop values=send(id.root,SEQ,a,n);CHECK(values);Oop args[]={receiver,values};return send(method,INVOKE,args,2);
}
/* [original, GC, ObjectModel, target, foreign, armed] */
static Oop cas(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;
    if(get(c,5)==id_size(1)){
        CHECK(put(c,5,id_size(0)));Oop values[]={id_size(0),get(c,3),get(c,4)};
        CHECK(send(get(c,2),SLOT_PUT,values,3));CHECK(send(get(c,1),COLLECT,0,0));
    }
    return invoke(get(c,0),r,a,n);
}
/* [original, Globals, key, value, watched selector, armed, fault] */
static Oop removal(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;if(n!=1)return id.invalid;
    Oop answer=invoke(get(c,0),r,a,n);CHECK(answer);
    if(a[0]==get(c,4)&&get(c,5)==id_size(1)){
        CHECK(put(c,5,id_size(0)));Oop values[]={get(c,2),get(c,3)};
        CHECK(send(get(c,1),PUT,values,2));return id_failure(get(c,6));
    }
    return answer;
}
/* Identity observations do not keep either object alive. */
static Oop watch(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=2)return id.invalid;
    for(size_t i=0;i<n;i++){identities[i]=send(id.root,IDENTITY,a+i,1);CHECK(identities[i]);}return 0;
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<2;i++)if(identities[i]==a[2])remaining++;return 0;
}
static Oop count(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;remaining=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(remaining);
}
static Oop initialize(const IdProtocol *p,Oop image,const Oop *symbols,size_t n,const Oop *a,size_t count){
    (void)a;if(count||n!=13)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("sequence"),LIT("invokeOn:arguments:"),
    LIT("slotAt:in:put:"),LIT("collectPrecisely"),LIT("cas"),LIT("removal"),LIT("watch"),LIT("identityOf:"),
    LIT("allocationsDo:"),LIT("allocation:bytes:identity:"),LIT("count")};
static const IdMethodSpec methods[]={{CAS,cas},{REMOVE,removal},{WATCH,watch},{ALLOCATION,allocation},{COUNT,count}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,13,methods,5,initialize};
