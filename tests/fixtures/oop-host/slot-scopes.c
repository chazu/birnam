/* Private observations are weak: only the runtime may retain the tested inputs. */
#include "../oop-id-v2/id.h"
#include <stdlib.h>
#include <string.h>
static IdProtocol id;
static Oop names[13], state[7], identities[3];
static Oop *seen, *baseline;
static size_t count, capacity, baseline_count, calls, nested, fail, alive;
enum { SEQ, ROOTS, VISIT, COLLECT, GET, IDENTITY, ALLOCATIONS,
       CONFIGURE, CONVERT, MODE, CHECKPOINT, STATS, ALLOCATION };
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked)) return checked; } while(0)
static Oop send(Oop r,size_t name,const Oop *a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop snapshot(void){count=0;return send(state[0],ROOTS,&state[2],1);}
static Oop census(void){alive=0;return send(id.root,ALLOCATIONS,&state[2],1);}
static int contains(Oop value){for(size_t i=0;i<count;i++)if(seen[i]==value)return 1;return 0;}
static Oop visit(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1)return id.invalid;
    if(count==capacity){size_t next=capacity?capacity*2:64;Oop *p=realloc(seen,next*sizeof(Oop));if(!p)return id.invalid;seen=p;capacity=next;}
    seen[count++]=a[0];return 0;
}
/* [ObjectModel, GC, visitor, target, edge, index, fault] */
static Oop configure(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=7)return id.invalid;
    for(size_t i=0;i<n;i++)state[i]=a[i];
    for(size_t i=0;i<3;i++){identities[i]=send(id.root,IDENTITY,&state[i+3],1);CHECK(identities[i]);}
    calls=nested=fail=0;return 0;
}
static Oop convert(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1||a[0]!=state[5])return id.invalid;
    calls++;CHECK(snapshot());
    if(!contains(state[3])||!contains(state[5]))return id.invalid;
    size_t before_count=count;
    Oop *before=malloc(count*sizeof(Oop));if(!before)return id.invalid;
    memcpy(before,seen,count*sizeof(Oop));
    Oop error=send(state[1],COLLECT,0,0);
    if(!id_fault(error))error=census();
    if(!id_fault(error)&&alive!=7)error=id.invalid;
    if(!id_fault(error)&&!nested){
        nested=1;Oop args[]={state[5],state[3]};Oop value=send(state[0],GET,args,2);nested=0;
        if(id_fault(value))error=value;
        else if(value!=state[4])error=id.invalid;
        else {
            error=snapshot();
            if(!id_fault(error)&&(count!=before_count||memcmp(seen,before,count*sizeof(Oop))))error=id.invalid;
            if(!id_fault(error))error=send(state[1],COLLECT,0,0);
            if(!id_fault(error)&&fail)error=id_failure(state[6]);
        }
    }
    free(before);return id_fault(error)?error:id_size(0);
}
static Oop mode(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1)return id.invalid;fail=id_unbox(a[0]);return 0;
}
static Oop checkpoint(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1)return id.invalid;CHECK(snapshot());
    if(!id_unbox(a[0])){
        Oop *p=realloc(baseline,count*sizeof(Oop));if(!p)return id.invalid;baseline=p;baseline_count=count;
        memcpy(baseline,seen,count*sizeof(Oop));
    }else if(count!=baseline_count||memcmp(seen,baseline,count*sizeof(Oop)))return id.invalid;
    return id_size(1);
}
static Oop stats(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;CHECK(census());
    Oop values[]={id_size(calls),id_size(alive)};return send(id.root,SEQ,values,2);
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop *a,size_t n){
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    for(size_t i=0;i<3;i++)if(a[2]==identities[i])alive|=(size_t)1<<i;return 0;
}
static Oop initialize(const IdProtocol *p,Oop image,const Oop *symbols,size_t n,const Oop *a,size_t count){
    (void)a;if(count||n!=13)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("sequence"),LIT("rootsDo:"),LIT("visit:"),LIT("collectPrecisely"),
    LIT("slot:in:"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("configure"),LIT("convert"),LIT("mode"),
    LIT("checkpoint"),LIT("stats"),LIT("allocation:bytes:identity:")};
static const IdMethodSpec methods[]={{VISIT,visit},{CONFIGURE,configure},{CONVERT,convert},{MODE,mode},
    {CHECKPOINT,checkpoint},{STATS,stats},{ALLOCATION,allocation}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,13,methods,7,initialize};
