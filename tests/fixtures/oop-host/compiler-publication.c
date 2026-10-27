/* Collecting explicit Store publication hooks, declared independently of Rust. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[7];
enum{AT,PUT,COLLECT,RAW,WORD,PUBLISH,AUTHORIZE};
/* [GC,Store,target,Integer,alias,decision,authorizations,notifications,
    publications,expected word,submitted Method,authorization fault,notification fault] */
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop pair[]={id_size(i),v};return send(c,PUT,pair,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
static Oop publish(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=3)return id.invalid;CHECK(put(c,10,a[2]));CHECK(send(get(c,0),COLLECT,0,0));CHECK(increment(c,8));return id.message(id.root,r,get(c,4),a,n);
}
static Oop authorize(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));CHECK(increment(c,6));Oop fault=get(c,11);return fault?id_failure(fault):get(c,5);
}
static Oop observe(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=2)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));
 Oop query[]={a[1],a[0]},mirror=send(get(c,1),RAW,query,2);CHECK(mirror);if(mirror!=get(c,10))return id.invalid;
 Oop value=id.message(id.root,get(c,2),a[0],0,0);CHECK(value);Oop bits=send(get(c,3),WORD,&value,1);CHECK(bits);if(bits!=get(c,9))return id.invalid;
 CHECK(increment(c,7));Oop fault=get(c,12);return fault?id_failure(fault):get(c,5);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 (void)a;if(count!=8||n)return p->invalid;id=*p;for(size_t i=0;i<7;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("rawCurrentValue:attr:"),LIT("wordOf:"),LIT("publish"),LIT("authorize"),LIT("observe")};
static const IdMethodSpec methods[]={{5,publish},{6,authorize},{7,observe}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,8,methods,3,initialize};
