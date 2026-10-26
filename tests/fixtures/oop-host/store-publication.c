/* Explicit Store policy callbacks; state is an Oop Sequence. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, COLLECT, DIRECT, READ, ASSERT, AUTHORIZE, MANAGED };
static Oop names[8];
enum { STORE, EXECUTION, GC, NIL, TRUE_VALUE, FALSE_VALUE, FOREIGN, ERROR_VALUE,
       AUTH_FAULT, MANAGED_FAULT, MODE, RECORDING, EVENT_COUNT, FIRST_EVENT, SECOND_EVENT };
#define CHECK(v) do { Oop result = (v); if (id_fault(result)) return result; } while (0)
static Oop send(Oop r, size_t name, const Oop *a, size_t n) {
    return id.message(id.root, r, names[name], a, n);
}
static Oop get(Oop c, size_t index) { Oop i=id_size(index); return send(c,AT,&i,1); }
static Oop put(Oop c, size_t index, Oop v) { Oop a[]={id_size(index),v}; return send(c,PUT,a,2); }
static Oop event(Oop c, Oop name) {
    Oop count=get(c,EVENT_COUNT); CHECK(count);
    size_t n=id_unbox(count);
    if(n>=2) return id.invalid;
    CHECK(put(c,FIRST_EVENT+n,name));
    return put(c,EVENT_COUNT,id_size(n+1));
}
static Oop authorize(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)r;(void)a;
    if(n) return id.invalid;
    if(get(c,RECORDING)!=id_size(1)) return get(c,TRUE_VALUE);
    CHECK(event(c,names[AUTHORIZE]));
    size_t mode=id_unbox(get(c,MODE));
    if(mode==1) return get(c,FALSE_VALUE);
    if(mode==2) return id_failure(get(c,AUTH_FAULT));
    return get(c,TRUE_VALUE);
}
static Oop managed(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)r;
    if(n!=2) return id.invalid;
    if(get(c,RECORDING)!=id_size(1)) return get(c,NIL);
    CHECK(event(c,names[MANAGED]));
    CHECK(send(get(c,GC),COLLECT,0,0));
    Oop direct=send(get(c,EXECUTION),DIRECT,a,2); CHECK(direct);
    Oop query[]={a[1],a[0]};
    Oop fact=send(get(c,STORE),READ,query,2); CHECK(fact);
    if((direct==get(c,NIL)) != (fact==0)) return id.invalid;
    if(fact && direct!=fact) return id.invalid;
    size_t mode=id_unbox(get(c,MODE));
    if(mode==3) {
        Oop values[]={get(c,FOREIGN),a[0],get(c,TRUE_VALUE)};
        CHECK(send(get(c,STORE),ASSERT,values,3));
        return id_failure(get(c,MANAGED_FAULT));
    }
    return get(c,mode==4?ERROR_VALUE:NIL);
}
static Oop initialize(const IdProtocol *p,Oop owner,const Oop *symbols,size_t count,const Oop *a,size_t n) {
    (void)a;
    if(n || count!=8) return p->invalid;
    id=*p;for(size_t i=0;i<count;i++)names[i]=symbols[i];return owner;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),
    LIT("directMethodFor:in:"),LIT("rawCurrentValue:attr:"),LIT("assert:attr:value:"),
    LIT("authorize"),LIT("managed")};
static const IdMethodSpec methods[]={{AUTHORIZE,authorize},{MANAGED,managed}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,8,methods,2,initialize};
