#include "id.h"
static IdProtocol id;static Oop at;
/* [receiver, selector, argument]. Unaware native C must get an answer now. */
static Oop callback(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;
 Oop index=id_size(0),receiver=id.message(id.root,c,at,&index,1);index=id_size(1);Oop selector=id.message(id.root,c,at,&index,1);index=id_size(2);Oop arg=id.message(id.root,c,at,&index,1);
 Oop result=id.message(id.root,receiver,selector,&arg,1);
 if(!id_fault(result))return id.invalid;return id_size(71);
}
static Oop reject(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;(void)n;return id.invalid;}
static const IdLiteral literals[]={LIT("at:"),LIT("callback"),LIT("rejectLookup")};
static const IdMethodSpec methods[]={{1,callback},{2,reject}};
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){(void)a;if(n!=3||count)return p->invalid;id=*p;at=s[0];return image;}
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,3,methods,2,initialize};
