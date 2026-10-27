/* Image acceptance callbacks: [alias, GC, calls, Codegen, failure, Roots]. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, GC, CANCEL, RETAIN, RELEASE, BEFORE, RESTORE, MAP, PUBLISH, DENY, COUNT };
static Oop names[COUNT];
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked))return checked; } while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static size_t increment(Oop c){size_t count=id_unbox(get(c,2))+1;Oop a[]={id_size(2),id_size(count)};send(c,PUT,a,2);return count;}
static Oop forward(Oop c,Oop r,const Oop*a,size_t n){return id.message(id.root,r,get(c,0),a,n);}
static Oop before(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(increment(c)==1)CHECK(send(get(c,1),GC,0,0));return forward(c,r,a,n);}
static Oop restore(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop result=forward(c,r,a,n);CHECK(result);
 Oop roots=get(c,5),pin=send(roots,RETAIN,&result,1);CHECK(pin);
 size_t count=increment(c);Oop collected=0;
 if(count<=8 || !(count&(count-1)))collected=send(get(c,1),GC,0,0);
 Oop keep[]={result,pin};Oop released=send(roots,RELEASE,keep,2);CHECK(collected);CHECK(released);return result;
}
static Oop map(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;size_t count=increment(c);CHECK(send(get(c,1),GC,0,0));
 if(!id_fault(send(get(c,3),CANCEL,0,0)))return id.invalid;
 return count==2?id_failure(get(c,4)):forward(c,r,a,n);
}
static Oop publish(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;CHECK(forward(c,r,a,n));increment(c);CHECK(send(get(c,1),GC,0,0));return id_failure(get(c,4));}
static Oop deny(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;return id_failure(c);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("cancelPortableImageUnit"),LIT("retain:"),LIT("releaseTokensProtecting"),LIT("before"),LIT("restore"),LIT("map"),LIT("publish"),LIT("deny")};
static const IdMethodSpec methods[]={{BEFORE,before},{RESTORE,restore},{MAP,map},{PUBLISH,publish},{DENY,deny}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,5,initialize};
