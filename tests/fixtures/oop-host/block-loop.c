/* Declared callbacks for live loop policy, collection and replacement. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum {AT,PUT,GC,VALUE,ADD,CLONE,IDENTITY,ALLOCS,RETAIN,RELEASE,CONDITION,BODY,REPLY,MUTATE,FRESH,NAMESPACE,CENSUS,PRESENT,COUNT};
static Oop names[COUNT];
/* Counter: GC,true,false,nil,calls,conditions,limit,inverted,condition override,
   condition value,body override,body value,failure,collecting,condition fault,
   body fault,Object,previous identity,probe,Roots. */
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
static Oop condition(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(increment(c,5));
 if(get(c,13)==id_size(1))CHECK(send(get(c,0),GC,0,0));
 if(get(c,12)==id_size(1))return id_failure(get(c,14));
 if(get(c,8)==id_size(1))return get(c,9);
 int yes=(id_unbox(get(c,4))<id_unbox(get(c,6)))!=(get(c,7)==id_size(1));return get(c,yes?1:2);
}
static Oop body(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(increment(c,4));
 if(get(c,13)==id_size(1))CHECK(send(get(c,0),GC,0,0));
 if(get(c,12)==id_size(2))return id_failure(get(c,15));
 return get(c,get(c,10)==id_size(1)?11:3);
}
/* [GC,value,calls]. */
static Oop reply(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;(void)n;CHECK(increment(c,2));CHECK(send(get(c,0),GC,0,0));return get(c,1);
}
/* [GC,target vtable,new Method,result,calls]. */
static Oop mutate(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;(void)n;CHECK(increment(c,4));Oop binding[]={names[VALUE],get(c,2)};
 CHECK(send(get(c,1),ADD,binding,2));CHECK(send(get(c,0),GC,0,0));return get(c,3);
}
static Oop fresh(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(send(get(c,0),GC,0,0));
 Oop previous=get(c,17);
 if(previous!=id_size(0)&&send(get(c,18),PRESENT,&previous,1)!=id_size(0))return id.invalid;
 CHECK(increment(c,4));Oop prototype=get(c,16),roots=get(c,19);
 Oop value=send(prototype,CLONE,0,0);CHECK(value);
 Oop pin=send(roots,RETAIN,&value,1);CHECK(pin);
 Oop identity=send(id.root,IDENTITY,&value,1),result=identity;
 if(!id_fault(identity))result=put(c,17,identity);
 Oop release[]={value,pin},released=send(roots,RELEASE,release,2);CHECK(result);CHECK(released);return value;
}
/* [GC,alias,ErrorVt key,calls]. */
static Oop namespace_at(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;
 if(a[0]==get(c,2)){CHECK(increment(c,3));CHECK(send(get(c,0),GC,0,0));}
 return id.message(id.root,r,get(c,1),a,n);
}
static size_t wanted,found;
static Oop census(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found++;return 0;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);found=0;CHECK(send(id.root,ALLOCS,&r,1));return id_size(found);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("value"),LIT("addMethod"),LIT("clone"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("retain:"),LIT("releaseTokensProtecting"),LIT("condition"),LIT("body"),LIT("reply"),LIT("mutate"),LIT("fresh"),LIT("namespaceAt"),LIT("census"),LIT("present:")};
static const IdMethodSpec methods[]={{CONDITION,condition},{BODY,body},{REPLY,reply},{MUTATE,mutate},{FRESH,fresh},{NAMESPACE,namespace_at},{CENSUS,census},{PRESENT,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,8,initialize};
