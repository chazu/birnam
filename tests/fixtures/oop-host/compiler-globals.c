/* Current Globals reads/writes with collection and exact failure identity. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[4];
static Oop get(Oop c,size_t i){Oop k=id_size(i);return id.message(id.root,c,names[0],&k,1);}
/* [GC,alias,target,value,calls,failure-or-null]. */
static Oop replacing(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n<1||n>2)return id.invalid;
 Oop result=id.message(id.root,get(c,0),names[2],0,0);if(id_fault(result))return result;
 if(a[0]==get(c,2)){
  Oop pair[]={id_size(4),id_size(id_unbox(get(c,4))+1)};
  result=id.message(id.root,c,names[1],pair,2);if(id_fault(result))return result;
  Oop failure=get(c,5);return failure?id_failure(failure):get(c,3);
 }
 return id.message(id.root,r,get(c,1),a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 (void)a;if(count!=4||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("replacing")};
static const IdMethodSpec methods[]={{3,replacing}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,4,methods,1,initialize};
