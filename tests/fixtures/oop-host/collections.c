/* Collection callbacks and weak allocation observations use only Oop messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[18], identities[63];
static size_t watched;
static uint64_t live;
enum {AT,PUT,COLLECT,WORD,FROM,IDENTITY,ALLOCATIONS,ALLOCATION,CALLBACK,
      REPLACEMENT,EQUAL,WATCH,MASK,OBSERVED,RESET,ADD_METHOD,VALUE,SIZE};
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked)) return checked; } while(0)
static Oop send(Oop r,size_t name,const Oop*a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop increment(Oop c,size_t i,size_t amount){
    Oop pair[]={id_size(i),id_size(id_unbox(get(c,i))+amount)};return send(c,PUT,pair,2);
}
static Oop observe(Oop value){
    if(watched==63)return id.invalid;
    Oop identity=send(id.root,IDENTITY,&value,1);CHECK(identity);
    identities[watched++]=identity;return value;
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<watched;i++)if(a[2]==identities[i])live|=UINT64_C(1)<<i;
    return 0;
}
static Oop mask(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    live=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(live);
}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;watched=0;
    for(size_t i=0;i<n;i++)CHECK(observe(a[i]));return 0;
}
static Oop observed(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(watched);
}
static Oop reset(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;watched=0;return 0;
}
/* [GC,Integer,mode,calls,callback vtable,replacement Method,fault,allocation visitor] */
static Oop callback(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;
    CHECK(send(get(c,0),COLLECT,0,0));
    size_t mode=id_unbox(get(c,2));
    if(mode==1||mode==5){
        Oop actual=mask(id.root,get(c,7),0,0,0);CHECK(actual);
        if(id_unbox(actual)!=((UINT64_C(1)<<watched)-1))return id.invalid;
    }
    CHECK(increment(c,3,1));
    if(mode==5&&get(c,3)==id_size(3))return id_failure(get(c,6));
    if(mode==4){
        if(n!=1)return id.invalid;
        Oop pair[]={names[VALUE],get(c,5)};CHECK(send(get(c,4),ADD_METHOD,pair,2));return a[0];
    }
    if(mode==0)return n==1?a[0]:id.invalid;
    size_t value=789;
    if(mode==1||mode==5||mode==2){
        if(n!=(mode==2?2:1))return id.invalid;
        Oop word=send(a[0],WORD,0,0);CHECK(word);
        if(!((uintptr_t)word&1))return id.invalid;
        value=id_unbox(word);
        if(mode==2){
            word=send(a[1],WORD,0,0);CHECK(word);
            if(!((uintptr_t)word&1))return id.invalid;
            value+=id_unbox(word);
        }else value%=3;
    }else if(mode!=3||n)return id.invalid;
    Oop bits=id_size(value);
    Oop result=send(get(c,1),FROM,&bits,1);CHECK(result);
    return observe(result);
}
static Oop replacement(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=1)return id.invalid;
    CHECK(send(get(c,0),COLLECT,0,0));CHECK(increment(c,3,100));return a[0];
}
/* [GC,empty Map,99,98,true,calls] */
static Oop nested_equal(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n!=1)return id.invalid;
    CHECK(increment(c,5,1));CHECK(send(get(c,0),COLLECT,0,0));
    Oop pair[]={get(c,2),get(c,3)};
    Oop nested=send(get(c,1),PUT,pair,2);CHECK(nested);
    Oop length=send(nested,SIZE,0,0);CHECK(length);
    Oop bits=send(length,WORD,0,0);CHECK(bits);
    if(bits!=id_size(1))return id.invalid;return get(c,4);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=18)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("word"),LIT("fromWord:"),
    LIT("identityOf:"),LIT("allocationsDo:"),LIT("allocation:bytes:identity:"),LIT("callback"),LIT("replacement"),
    LIT("nestedEqual"),LIT("watch"),LIT("mask"),LIT("observed"),LIT("reset"),LIT("addMethod"),LIT("value:"),LIT("size")};
static const IdMethodSpec methods[]={{ALLOCATION,allocation},{CALLBACK,callback},{REPLACEMENT,replacement},
    {EQUAL,nested_equal},{WATCH,watch},{MASK,mask},{OBSERVED,observed},{RESET,reset}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,18,methods,8,initialize};
