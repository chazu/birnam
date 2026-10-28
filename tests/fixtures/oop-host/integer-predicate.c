/* Explicit predicate visitor: commit through current Id, with collection. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[8];
enum {AT,PUT,GC,COMMIT,SEQUENCE,INVOKE,FORWARD,REPLACE};
static Oop get(Oop c,size_t i){Oop index=id_size(i);return id.message(id.root,c,names[AT],&index,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return id.message(id.root,c,names[PUT],a,2);}
/* [GC, calls, result, optional failure]. */
static Oop commit(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;
 Oop result=put(c,1,id_size(id_unbox(get(c,1))+1));if(id_fault(result))return result;
 result=put(c,2,a[0]);if(id_fault(result))return result;
 result=id.message(id.root,get(c,0),names[GC],0,0);if(id_fault(result))return result;
 Oop failure=get(c,3);return failure?id_failure(failure):0;
}
/* [captured Method, calls, selected receiver or null]. */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop selected=get(c,2);
 if(!selected||r==selected){Oop changed=put(c,1,id_size(id_unbox(get(c,1))+1));if(id_fault(changed))return changed;}
 Oop args=id.message(id.root,id.root,names[SEQUENCE],a,n);if(id_fault(args))return args;
 Oop invocation[]={r,args};return id.message(id.root,get(c,0),names[INVOKE],invocation,2);
}
/* [captured Method, selected calls, selected receiver, replacement result]. */
static Oop replace(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 if(r!=get(c,2))return forward(root,c,r,a,n);
 Oop changed=put(c,1,id_size(id_unbox(get(c,1))+1));return id_fault(changed)?changed:get(c,3);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=8||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("commitNumericPredicate:"),LIT("sequence"),LIT("invokeOn:arguments:"),LIT("forward"),LIT("replace")};
static const IdMethodSpec methods[]={{COMMIT,commit},{FORWARD,forward},{REPLACE,replace}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,8,methods,3,initialize};
