/* GC controls and weak graph observations. All runtime access uses Id messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[30], identities[2];
static size_t watched, live, events;
/* These graph edges are reported only by the configured parent's trace method. */
static Oop parent, carrier, trigger, behavior;
static int next_mode, change_layout;
enum {AT,PUT,COLLECT,PRECISE_COLLECT,PRECISE,SET_PRECISE,AUTOMATIC,ALLOCATE,
      IDENTITY,ALLOCATIONS,ALLOCATION,VISIT,REFERENCES,LAYOUT,ADD_METHOD,FROM_WORD,
      BUDGET,CHARGE,PREPARE,AUTO_GC,RETAIN,RELEASE,FORWARD,QUERY,FINALIZE,
      CONFIGURE,WALK,WATCH,MASK,EVENTS};
#define CHECK(v) do { Oop checked=(v);if(id_fault(checked))return checked; }while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
/* [alias,calls,failure-or-null]. No new argument containers during tracing. */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;CHECK(increment(c,1));Oop failure=get(c,2);
    return failure?id_failure(failure):id.message(id.root,r,get(c,0),a,n);
}
/* [Integer vtable,replacement constructor Method,full-width query answer,calls] */
static Oop query(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    CHECK(increment(c,3));Oop pair[]={names[FROM_WORD],get(c,1)};CHECK(send(get(c,0),ADD_METHOD,pair,2));return get(c,2);
}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n>2)return id.invalid;watched=n;
    for(size_t i=0;i<n;i++){identities[i]=send(id.root,IDENTITY,&a[i],1);CHECK(identities[i]);}return 0;
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<watched;i++)if(identities[i]==a[2])live|=1<<i;return 0;
}
/* Context is the allocation visitor. */
static Oop mask(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    live=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(live);
}
/* [GC,calls,violations] */
static Oop finalize(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n!=1)return id.invalid;CHECK(increment(c,1));
    Oop gc=get(c,0),zero=id_size(0),one=id_size(1);
    if(id_fault(send(gc,AUTOMATIC,&zero,1)))CHECK(increment(c,2));
    if(id_fault(send(gc,SET_PRECISE,&one,1)))CHECK(increment(c,2));
    if(send(gc,COLLECT,0,0)!=zero)CHECK(increment(c,2));return 0;
}
static Oop configure(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=6)return id.invalid;
    parent=a[0];carrier=a[1];trigger=a[2];behavior=a[3];
    next_mode=id_unbox(a[4])!=0;change_layout=id_unbox(a[5])!=0;events=0;return 0;
}
/* [GC,ObjectModel,empty byte provider]. Only the parent's edges keep the graph alive. */
static Oop walk(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=2)return id.invalid;
    if(a[0]==parent){CHECK(send(a[1],VISIT,&carrier,1));CHECK(send(a[1],VISIT,&trigger,1));}
    else if(a[0]==trigger){
        events=events*4+1;Oop gc=get(c,0),mode=id_size(next_mode),zero=id_size(0);
        CHECK(send(gc,SET_PRECISE,&mode,1));CHECK(send(gc,AUTOMATIC,&zero,1));
        if(change_layout){Oop args[]={behavior,get(c,2),id_size(1)};CHECK(send(get(c,1),LAYOUT,args,3));}
        if(send(gc,COLLECT,0,0)!=zero)return id.invalid;
        if(!id_fault(send(behavior,ALLOCATE,&zero,1)))return id.invalid;
    }else if(a[0]==carrier){events=events*4+2;CHECK(send(get(c,1),REFERENCES,a,n));}
    return 0;
}
static Oop event_count(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(events);
}
/* [GC,ObjectVt,spy context,failure,allocation visitor].
   Do not allocate Names or argument snapshots between the accounting queries. */
static Oop failed_collection(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    Oop zero=id_size(0),dead=send(get(c,1),ALLOCATE,&zero,1);CHECK(dead);
    CHECK(watch(id.root,0,0,&dead,1));CHECK(put(get(c,2),2,get(c,3)));
    Oop gc=get(c,0),charged=send(gc,CHARGE,0,0);CHECK(charged);
    Oop failure=send(gc,PRECISE_COLLECT,0,0);
    if(failure!=id_failure(get(c,3))||send(gc,CHARGE,0,0)!=charged)return id.invalid;
    if(mask(id.root,get(c,4),0,0,0)!=id_size(1))return id.invalid;
    return failure;
}
/* [GC,ObjectVt,allocation visitor]. Keep a native stack slot, not a Roots entry. */
static Oop stack_root(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    Oop zero=id_size(0);volatile Oop value=send(get(c,1),ALLOCATE,&zero,1);CHECK(value);
    Oop observed=value;CHECK(watch(id.root,0,0,&observed,1));
    CHECK(send(get(c,0),COLLECT,0,0));
    if(mask(id.root,get(c,2),0,0,0)!=id_size(1))return id.invalid;
    return value;
}
/* [GC,System,true,false,ObjectVt,Roots]. Exact accounting through existing selectors. */
static Oop accounting(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;
    Oop gc=get(c,0),system=get(c,1),truth=get(c,2),falsity=get(c,3),one=id_size(1);
    CHECK(send(gc,BUDGET,&one,1));CHECK(send(system,AUTO_GC,&falsity,1));CHECK(send(gc,PRECISE_COLLECT,0,0));
    size_t previous=0;Oop tokens[3];
    for(size_t i=0;i<3;i++){
        Oop zero=id_size(0),value=send(get(c,4),ALLOCATE,&zero,1);CHECK(value);
        tokens[i]=send(get(c,5),RETAIN,&value,1);CHECK(tokens[i]);
        Oop charged=send(gc,CHARGE,0,0);CHECK(charged);
        if(id_unbox(charged)<=previous)return id.invalid;previous=id_unbox(charged);
    }
    Oop charge=send(gc,CHARGE,0,0);CHECK(charge);
    if(!id_unbox(charge))return id.invalid;
    CHECK(send(gc,PREPARE,0,0));if(send(gc,CHARGE,0,0)!=charge)return id.invalid;
    CHECK(send(system,AUTO_GC,&truth,1));
    Oop enabled=send(gc,CHARGE,0,0);CHECK(enabled);
    if(id_unbox(enabled)<id_unbox(charge))return id.invalid;
    CHECK(send(gc,PREPARE,0,0));if(send(gc,CHARGE,0,0)!=id_size(0))return id.invalid;
    Oop budget=id_size(1<<20);CHECK(send(gc,BUDGET,&budget,1));CHECK(send(system,AUTO_GC,&falsity,1));
    for(size_t i=0;i<3;i++)CHECK(send(get(c,5),RELEASE,&tokens[i],1));
    return one;
}
/* Descriptor names for the last three callbacks follow the shared message literals. */
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collect"),LIT("collectPrecisely"),LIT("precisePayloads"),
    LIT("precisePayloads:"),LIT("automaticCollection:"),LIT("allocate"),LIT("identityOf:"),LIT("allocationsDo:"),
    LIT("allocation:bytes:identity:"),LIT("visit:"),LIT("referencesOf:do:"),LIT("setLayoutOf:to:rest:"),LIT("addMethod"),
    LIT("fromWord:"),LIT("budget:"),LIT("allocationBytes"),LIT("prepareAllocation"),LIT("autoGc:"),LIT("retain:"),LIT("release:"),
    LIT("forward"),LIT("query"),LIT("finalize"),LIT("configureWalk"),LIT("walk"),LIT("watch"),LIT("mask"),LIT("events"),
    LIT("failedCollection"),LIT("stackRoot"),LIT("accounting")};
static const IdMethodSpec methods[]={{FORWARD,forward},{QUERY,query},{FINALIZE,finalize},{CONFIGURE,configure},{WALK,walk},
    {WATCH,watch},{MASK,mask},{EVENTS,event_count},{ALLOCATION,allocation},{30,failed_collection},{31,stack_root},{32,accounting}};
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=33)return p->invalid;id=*p;for(size_t i=0;i<30;i++)names[i]=symbols[i];return image;
}
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,33,methods,12,initialize};
