/* Independent synchronous callbacks for the Oop recovery interface. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, COLLECT, RETURN, RECOVER, GRAPH, VALUE, PARK, STEP, CANDIDATE, FIXED,
    FROM, LOW, HIGH, RETAIN, RELEASE, IDENTITY, LAYOUT, SLOT, WORD, REFS,
    FACTORY, SIZE, INDEX, TRACE, ALLOCS, ALLOCATION, PRESENT, COUNT };
static Oop names[COUNT];
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
/* [constructor alias,Roots,GC,ObjectModel,layout,empty layout,mode,calls,identity]. */
static Oop inspect_fresh(Oop c,Oop value) {
    Oop identity=send(id.root,IDENTITY,&value,1);CHECK(identity);CHECK(put(c,8,identity));
    CHECK(increment(c,7));CHECK(send(get(c,2),COLLECT,0,0));
    if(get(c,6)==id_size(1)) {
        Oop args[]={get(c,4),get(c,5),id_size(2)};CHECK(send(get(c,3),LAYOUT,args,3));
    }
    return value;
}
static Oop factory(Oop root,Oop c,Oop r,const Oop*a,size_t n) {
    (void)root;if(n!=1)return id.invalid;
    Oop low=send(a[0],LOW,0,0);CHECK(low);Oop high=send(a[0],HIGH,0,0);CHECK(high);
    Oop value=id.message(id.root,r,get(c,0),a,n);CHECK(value);
    if(low!=id_size(0)||high!=id_size(0x80000000))return value;
    Oop pin=send(get(c,1),RETAIN,&value,1);CHECK(pin);
    Oop result=inspect_fresh(c,value),released=send(get(c,1),RELEASE,&pin,1);
    CHECK(result);CHECK(released);return result;
}
/* [GC,Integer,ObjectModel,inner recipe,Machine,mode,calls]. */
static Oop recipe_size(Oop root,Oop c,Oop r,const Oop*a,size_t n) {
    (void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));
    if(get(c,5)==id_size(1)) {
        CHECK(put(c,5,id_size(0)));Oop values[]={id_size(0),get(c,3)};
        CHECK(send(get(c,4),RECOVER,values,2));CHECK(increment(c,6));
    }
    Oop two=id_size(2);return send(get(c,1),FROM,&two,1);
}
static Oop recipe_at(Oop root,Oop c,Oop r,const Oop*a,size_t n) {
    (void)root;if(n!=1)return id.invalid;
    Oop index=send(get(c,1),WORD,a,1);CHECK(index);CHECK(send(get(c,0),COLLECT,0,0));
    Oop values[]={index,r};return send(get(c,2),SLOT,values,2);
}
static Oop trace_forward(Oop root,Oop c,Oop r,const Oop*a,size_t n) {
    (void)root;(void)r;return send(c,REFS,a,n);
}
static size_t wanted,alive;
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n) {
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    if(id_unbox(a[2])==wanted)alive++;return 0;
}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n) {
    (void)root;(void)c;if(n!=1)return id.invalid;
    wanted=id_unbox(a[0]);alive=0;CHECK(send(id.root,ALLOCS,&r,1));return id_size(alive);
}
static Oop initialize(const IdProtocol *p,Oop image,const Oop *symbols,size_t count,const Oop *a,size_t n){
    (void)a;if(n||count!=COUNT)return p->invalid;id=*p;
    for(size_t i=0;i<count;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("returnAt:"),
    LIT("recoverAt:with:"),LIT("recoverAt:withGraph:"),LIT("value"),LIT("park"),LIT("step"),LIT("candidate"),LIT("fixed"),
    LIT("fromWord:"),LIT("low32"),LIT("high32"),LIT("retain:"),LIT("release:"),LIT("identityOf:"),
    LIT("setLayoutOf:to:rest:"),LIT("slotAt:in:"),LIT("wordOf:"),LIT("referencesOf:do:"),
    LIT("factory"),LIT("recipeSize"),LIT("recipeAt"),LIT("traceForward"),LIT("allocationsDo:"),
    LIT("allocation:bytes:identity:"),LIT("present")};
static const IdMethodSpec methods[]={{STEP,step},{CANDIDATE,candidate},{FIXED,fixed},{FACTORY,factory},
    {SIZE,recipe_size},{INDEX,recipe_at},{TRACE,trace_forward},{ALLOCATION,allocation},{PRESENT,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,9,initialize};
