/* Replaceable materialization dependencies, including collection and native failure. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[4];
/* [GC,answer,calls,failure-or-null] */
static Oop result(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;(void)n;
    Oop index=id_size(2),count=id.message(id.root,c,names[0],&index,1);
    if(id_fault(count))return count;
    Oop update[]={index,id_size(id_unbox(count)+1)};
    Oop changed=id.message(id.root,c,names[1],update,2);if(id_fault(changed))return changed;
    index=id_size(0);Oop gc=id.message(id.root,c,names[0],&index,1);if(id_fault(gc))return gc;
    Oop collected=id.message(id.root,gc,names[2],0,0);if(id_fault(collected))return collected;
    index=id_size(3);Oop failure=id.message(id.root,c,names[0],&index,1);if(id_fault(failure))return failure;
    if(failure)return id_failure(failure);
    index=id_size(1);return id.message(id.root,c,names[0],&index,1);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){
    (void)a;if(n!=4||count)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("result")};
static const IdMethodSpec methods[]={{3,result}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,4,methods,1,initialize};
