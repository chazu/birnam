/* These weak test observations are deliberately not reported as GC roots. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[16], state[10], identities[2], saved;
static size_t calls, entered, nested, alive, fail;
enum { SEQ, INVOKE, COLLECT, LAYOUT, INVENTORY, REASSOCIATE, RETAIN, IDENTITY,
       ALLOCATIONS, CONFIGURE, PARENT, CONVERT, MODE, STATS, ALLOCATION, PARENT_ORIGINAL };
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked)) return checked; } while(0)
static Oop send(Oop r,size_t name,const Oop *a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop invoke(Oop method,Oop receiver,const Oop *a,size_t n){
    Oop values=send(id.root,SEQ,a,n);CHECK(values);
    Oop args[]={receiver,values};return send(method,INVOKE,args,2);
}
static Oop census(void){alive=0;return send(id.root,ALLOCATIONS,&state[9],1);}
/* [ObjectModel, GC, Roots, target, private vtable, replacement vtable,
    captured Integer factory, zero layout, fault, visitor] */
static Oop configure(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=10)return id.invalid;
    for(size_t i=0;i<n;i++)state[i]=a[i];
    for(size_t i=0;i<2;i++){identities[i]=send(id.root,IDENTITY,&state[i+3],1);CHECK(identities[i]);}
    calls=entered=nested=alive=fail=0;saved=0;return 0;
}
static Oop parent(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;if(n!=1)return id.invalid;
    Oop answer=send(r,PARENT_ORIGINAL,a,n);CHECK(answer);
    if(a[0]==state[4]&&!entered){
        entered=1;
        Oop values[]={state[3],state[5]};CHECK(send(id.root,REASSOCIATE,values,2));
        CHECK(send(state[1],COLLECT,0,0));CHECK(census());
        if(!(alive&2))return id.invalid;
        saved=send(state[2],RETAIN,&state[4],1);CHECK(saved);
    }
    return answer;
}
static Oop convert(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;if(n!=1)return id.invalid;
    calls++;
    if(calls==1){
        Oop values[]={state[4],state[7],id_size(0)};
        CHECK(send(state[0],LAYOUT,values,3));
        CHECK(send(state[0],INVENTORY,&state[3],1));nested++;
    }
    CHECK(send(state[1],COLLECT,0,0));CHECK(census());
    if(!(alive&1))return id.invalid;
    if(fail&&calls==2)return id_failure(state[8]);
    return invoke(state[6],r,a,n);
}
static Oop mode(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1)return id.invalid;
    fail=id_unbox(a[0]);calls=nested=0;return 0;
}
static Oop stats(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;
    CHECK(census());
    Oop values[]={id_size(entered),saved,id_size(calls),id_size(alive),id_size(nested)};
    return send(id.root,SEQ,values,5);
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<2;i++)if(a[2]==identities[i])alive|=(size_t)1<<i;
    return 0;
}
static Oop initialize(const IdProtocol *p,Oop image,const Oop *symbols,size_t count,const Oop *a,size_t n){
    (void)a;if(n||count!=16)return p->invalid;id=*p;
    for(size_t i=0;i<count;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("sequence"),LIT("invokeOn:arguments:"),LIT("collectPrecisely"),
    LIT("setLayoutOf:to:rest:"),LIT("referenceSlotsOf:"),LIT("reassociateBehaviorOf:with:"),
    LIT("retain:"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("configure"),LIT("parent"),
    LIT("convert"),LIT("mode"),LIT("stats"),LIT("allocation:bytes:identity:"),LIT("referenceParentOriginal:")};
static const IdMethodSpec methods[]={{CONFIGURE,configure},{PARENT,parent},{CONVERT,convert},
    {MODE,mode},{STATS,stats},{ALLOCATION,allocation}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,16,methods,6,initialize};
