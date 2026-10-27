/* [query alias, observed inputs sequence, count, failure or null]. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[3];
static Oop get(Oop c,size_t i){Oop index=id_size(i);return id.message(id.root,c,names[0],&index,1);}
static Oop put(Oop c,size_t i,Oop value){Oop a[]={id_size(i),value};return id.message(id.root,c,names[1],a,2);}
static Oop observe(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=3)return id.invalid;
 size_t index=id_unbox(get(c,2));if(index>=16)return id.invalid;
 Oop result=put(get(c,1),index,a[0]);if(id_fault(result))return result;
 result=put(c,2,id_size(index+1));if(id_fault(result))return result;
 Oop failure=get(c,3);if(failure)return id_failure(failure);
 return id.message(id.root,r,get(c,0),a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=3)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("observe")};
static const IdMethodSpec methods[]={{2,observe}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,3,methods,1,initialize};
