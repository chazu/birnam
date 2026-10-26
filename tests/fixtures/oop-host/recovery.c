/* Independent synchronous callbacks for the Oop recovery interface. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[11];
enum { AT, PUT, COLLECT, RETURN, RECOVER, GRAPH, VALUE, PARK, STEP, CANDIDATE, FIXED };
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked)) return checked; } while(0)
static Oop send(Oop r,size_t name,const Oop *a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop get(Oop c,size_t i){Oop a=id_size(i);return send(c,AT,&a,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
/* [calls, GC]. The ordinary frame returns slot 1 without replaying candidate work. */
static Oop step(Oop root,Oop c,Oop machine,const Oop *a,size_t n){
    (void)root;(void)a;if(n)return id.invalid;
    CHECK(increment(c,0));CHECK(send(get(c,1),COLLECT,0,0));
    Oop slot=id_size(1);return send(machine,RETURN,&slot,1);
}
/* [mode, answer-or-recipe, GC, calls, nested Block, observed result]. */
static Oop candidate(Oop root,Oop c,Oop machine,const Oop *a,size_t n){
    (void)root;(void)a;if(n!=1)return id.invalid;
    size_t mode=id_unbox(get(c,0));
    CHECK(increment(c,3));CHECK(send(get(c,2),COLLECT,0,0));
    if(mode==0)return get(c,1);
    if(mode==1)return 0;
    if(mode==5){Oop answer=send(get(c,4),VALUE,0,0);CHECK(answer);CHECK(put(c,5,answer));}
    if(mode==7){Oop answer=send(machine,PARK,0,0);CHECK(put(c,5,id_fault(answer)?(Oop)((uintptr_t)answer&~(uintptr_t)2):answer));}
    Oop values[]={id_size(0),get(c,1)};
    Oop answer=send(machine,mode==3?GRAPH:RECOVER,values,2);CHECK(answer);
    if(mode==6){Oop again=send(machine,RECOVER,values,2);if(!id_fault(again))return id.invalid;}
    return mode==4?machine:0;
}
static Oop fixed(Oop root,Oop c,Oop r,const Oop *a,size_t n){(void)root;(void)r;(void)a;(void)n;return c;}
static Oop initialize(const IdProtocol *p,Oop image,const Oop *symbols,size_t count,const Oop *a,size_t n){
    (void)a;if(n||count!=11)return p->invalid;id=*p;
    for(size_t i=0;i<count;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("returnAt:"),
    LIT("recoverAt:with:"),LIT("recoverAt:withGraph:"),LIT("value"),LIT("park"),LIT("step"),LIT("candidate"),LIT("fixed")};
static const IdMethodSpec methods[]={{STEP,step},{CANDIDATE,candidate},{FIXED,fixed}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,11,methods,3,initialize};
