/* Replacement during entry observes only weak Method/body identities. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum{AT,PUT,GC,ADD,ALLOCS,SWITCH,FIXED,ROW,PRESENT,COUNT};static Oop names[COUNT];static size_t wanted,found;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found++;return 0;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);found=0;CHECK(send(id.root,ALLOCS,&r,1));return id_size(found);}
/* [GC, target vtable, selector, replacement, Method identity, body identity, calls, monitor]. */
static Oop change(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;
 Oop counter[]={id_size(6),id_size(id_unbox(get(c,6))+1)};CHECK(send(c,PUT,counter,2));
 Oop args[]={get(c,3),get(c,2)};CHECK(send(get(c,1),ADD,args,2));CHECK(send(get(c,0),GC,0,0));
 for(size_t i=4;i<=5;i++){Oop key=get(c,i);if(send(get(c,7),PRESENT,&key,1)!=id_size(1))return id.invalid;}
 return 0;
}
static Oop fixed(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;return c;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("addMethod:as:"),LIT("allocationsDo:"),LIT("switch"),LIT("fixed"),LIT("allocation:bytes:identity:"),LIT("present:")};
static const IdMethodSpec methods[]={{SWITCH,change},{FIXED,fixed},{ROW,row},{PRESENT,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,4,initialize};
