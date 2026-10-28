/* Cell access and construction remain ordinary current messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[6];
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop get(Oop c,size_t i){Oop k=id_size(i);return id.message(id.root,c,names[0],&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return id.message(id.root,c,names[1],a,2);}
/* [prior alias, GC, watched receiver, result, calls]. */
static Oop read(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=2)return id.invalid;if(a[1]!=get(c,2))return id.message(id.root,r,get(c,0),a,n);CHECK(put(c,4,id_size(id_unbox(get(c,4))+1)));CHECK(id.message(id.root,get(c,1),names[2],0,0));return get(c,3);}
/* [target, recorded value, clones, setters]. */
static Oop clone(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(put(c,2,id_size(id_unbox(get(c,2))+1)));return get(c,0);}
static Oop set(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=1)return id.invalid;CHECK(put(c,1,a[0]));CHECK(put(c,3,id_size(id_unbox(get(c,3))+1)));return r;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=6)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("read"),LIT("clone"),LIT("set")};
static const IdMethodSpec methods[]={{3,read},{4,clone},{5,set}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,6,methods,3,initialize};
