/* Explicit Store/publication callbacks; all state is an Oop Sequence. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum {AT,PUT,GC,ADD,VALUE,MANAGED,AUTHORIZE,NOTIFY,COMMIT,FAULT,ERROR_VALUE,
      CAUGHT,CLEANED,COLLECT,AUTHORIZER,OBSERVER,VTABLE,SELECTOR,METHOD,ANSWER,COUNT};
static Oop names[COUNT];
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
/* [vtable, selector, Method, Error value, authorizer, observer, failure,
    GC, nil, event Sequence, event count]. Events: authorize=0, notify=1,
    fault=2, caught=3, cleanup=4. */
static Oop event(Oop c,size_t e){size_t i=id_unbox(get(c,10));if(i==32)return id.invalid;CHECK(put(get(c,9),i,id_size(e)));return put(c,10,id_size(i+1));}
static Oop answer(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;return c;}
#define SIMPLE(fn,body) static Oop fn(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;body}
SIMPLE(fault,CHECK(event(c,2));CHECK(send(get(c,7),GC,0,0));return id_failure(get(c,6));)
SIMPLE(error_value,return get(c,3);)
SIMPLE(caught,CHECK(event(c,3));return get(c,8);)
SIMPLE(cleaned,CHECK(event(c,4));CHECK(send(get(c,7),GC,0,0));return get(c,8);)
SIMPLE(collect,CHECK(send(get(c,7),GC,0,0));return get(c,8);)
SIMPLE(authorizer,CHECK(event(c,0));return get(c,4);)
SIMPLE(observer,CHECK(event(c,1));return get(c,5);)
SIMPLE(vtable,return get(c,0);)
SIMPLE(selector,return get(c,1);)
SIMPLE(method,return get(c,2);)
SIMPLE(authorize,CHECK(event(c,0));return send(get(c,4),VALUE,0,0);)
static Oop notify(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;CHECK(event(c,1));Oop args[]={get(c,1),get(c,0)};return send(get(c,5),MANAGED,args,2);}
static Oop commit(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)a;(void)n;Oop args[]={get(c,2),get(c,1)};CHECK(send(get(c,0),ADD,args,2));return r;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=COUNT||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("addMethod:as:"),LIT("value"),LIT("managedMethodChanged:on:"),LIT("authorize"),LIT("notify"),LIT("commit"),LIT("fault"),LIT("errorValue"),LIT("caught"),LIT("cleaned"),LIT("collect"),LIT("authorizer"),LIT("observer"),LIT("vtable"),LIT("selector"),LIT("method"),LIT("answer")};
static const IdMethodSpec methods[]={{AUTHORIZE,authorize},{NOTIFY,notify},{COMMIT,commit},{FAULT,fault},{ERROR_VALUE,error_value},{CAUGHT,caught},{CLEANED,cleaned},{COLLECT,collect},{AUTHORIZER,authorizer},{OBSERVER,observer},{VTABLE,vtable},{SELECTOR,selector},{METHOD,method},{ANSWER,answer}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,14,initialize};
