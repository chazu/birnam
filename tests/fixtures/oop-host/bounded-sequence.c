#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop guard(Oop root,Oop alias,Oop receiver,const Oop*a,size_t n){
 (void)root;
 if(n>8192)return id.invalid;
 return id.message(id.root,receiver,alias,a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*l,size_t n,const Oop*a,size_t count){
 (void)l;(void)a;if(n!=1||count)return p->invalid;id=*p;return image;
}
static const IdLiteral literals[]={LIT("guard")};
static const IdMethodSpec methods[]={{0,guard}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,1,methods,1,initialize};
