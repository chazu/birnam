/* Independent continuation forwarding; no Execution alias registration. */
#include "id.h"
static IdContinuationProtocol api;
static IdProtocol id;
enum {SEND,ANSWER,WITH,METHOD,VALUE,DIRECT,WRAPPED,FORWARD,TWICE,COUNT};
static Oop names[COUNT];
static Oop enter(Oop root,Oop c,Oop r,const Oop*a,size_t n,int wrapped){
 (void)root;(void)r;if(n!=2)return id.invalid;Oop k=a[0];
 for(int i=0;i<wrapped;i++){
  Oop m=id.message(id.root,c,names[METHOD],&names[FORWARD],1);if(id_fault(m))return m;
  k=id.message(id.root,m,names[WITH],&k,1);if(id_fault(k))return k;
 }
 Oop request[]={a[1],names[VALUE],k};return api.control(id.root,names[SEND],request,3);
}
static Oop direct(Oop root,Oop c,Oop r,const Oop*a,size_t n){return enter(root,c,r,a,n,0);}
static Oop wrapped(Oop root,Oop c,Oop r,const Oop*a,size_t n){return enter(root,c,r,a,n,1);}
static Oop twice(Oop root,Oop c,Oop r,const Oop*a,size_t n){return enter(root,c,r,a,n,2);}
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=2)return id.invalid;Oop answer[]={c,a[1]};return api.control(id.root,names[ANSWER],answer,2);}
static Oop initialize(const IdContinuationProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=COUNT||n)return p->common.invalid;api=*p;id=p->common;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("prepareSend"),LIT("answerTo:with:"),LIT("withContext:"),LIT("methodAt:"),LIT("value"),LIT("direct:"),LIT("wrapped:"),LIT("forward"),LIT("twice:")};
static const IdContinuationMethodSpec methods[]={{DIRECT,direct,1},{WRAPPED,wrapped,1},{FORWARD,forward,1},{TWICE,twice,1}};
const IdContinuationModule birnam_native_module_v3={3,sizeof(IdContinuationModule),literals,COUNT,methods,4,initialize};
