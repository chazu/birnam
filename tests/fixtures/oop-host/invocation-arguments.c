/* [GC, receiver, nested selector, inside, calls, byte object]. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[4];
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop get(Oop c,size_t i){Oop k=id_size(i);return id.message(id.root,c,names[0],&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return id.message(id.root,c,names[1],a,2);}
static Oop inspect(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=4)return id.invalid;Oop before[]={a[0],a[1],a[2],a[3]};
 CHECK(put(c,4,id_size(id_unbox(get(c,4))+1)));
 CHECK(id.message(id.root,get(c,0),names[2],0,0));
 if(get(c,3)==id_size(0)){
  CHECK(put(c,3,id_size(1)));Oop receiver=get(c,1),args[]={receiver,receiver,get(c,5),id_size(0)};
  Oop result=id.message(id.root,receiver,get(c,2),args,4);CHECK(result);if(result!=receiver)return id.invalid;
  CHECK(put(c,3,id_size(0)));
 }
 for(size_t i=0;i<4;i++)if(a[i]!=before[i])return id.invalid;
 return a[0];
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=4)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("inspect")};
static const IdMethodSpec methods[]={{3,inspect}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,4,methods,1,initialize};
