/* Independent descriptor steps and collecting callbacks; contexts are Oops. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, AT_WORD, COLLECT, WORD_OF, INTEGER, RETURN, CONTINUE, READ, WRITE,
       SUSPEND, CAPTURE, CALL, TAIL, PARK, ELIGIBLE, NEW, IDENTITY, ALLOCATIONS,
       STEP, CALLBACK, BOUNDARY, TRACE, SELF, ALLOCATION, PRESENT,
       CANDIDATE, WITHDRAW, OPTIMIZE, FROM, PROOF, ADD, COUNT };
static Oop names[COUNT];
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked))return checked; } while(0)
static Oop send(Oop r,size_t s,const Oop *a,size_t n) { return id.message(id.root,r,names[s],a,n); }
static Oop get(Oop c,size_t i) { Oop k=id_size(i);return send(c,AT,&k,1); }
static Oop put(Oop c,size_t i,Oop v) { Oop a[]={id_size(i),v};return send(c,PUT,a,2); }
/* [Namespace,mode]. The descriptor owns all frame shapes; this step reads them
   only through Execution's current messages. */
static Oop step(Oop root,Oop c,Oop machine,const Oop *a,size_t n) {
    (void)root;if(n>1)return id.invalid;
    size_t pc=0,mode=id_unbox(get(c,1));
    if(n) {
        Oop integer=send(get(c,0),AT,&names[INTEGER],1);CHECK(integer);
        Oop bits=send(integer,WORD_OF,a,1);CHECK(bits);pc=id_unbox(bits);
    }
    if(mode==1 || mode==6) { Oop slot=id_size(1);return send(machine,RETURN,&slot,1); }
    if(mode==5) {
        Oop slot=id_size(pc==0?1:3);
        if(pc!=3)return send(machine,CONTINUE,&slot,1);
        slot=id_size(1);return send(machine,RETURN,&slot,1);
    }
    if(pc==1) { Oop slot=id_size(5);return send(machine,RETURN,&slot,1); }
    if(mode==2 || mode==3) {
        Oop args[]={id_size(5),id_size(1)};
        return send(machine,mode==2?SUSPEND:CAPTURE,args,2);
    }
    Oop slot=id_size(1),owner=send(machine,READ,&slot,1);CHECK(owner);
    for(size_t i=0;i<3;i++) {
        Oop index=id_size(i),value=send(owner,AT_WORD,&index,1);CHECK(value);
        Oop args[]={id_size(i+2),value};CHECK(send(machine,WRITE,args,2));
    }
    Oop site=id_size(0);return send(machine,mode==7?TAIL:CALL,&site,1);
}
/* [GC,calls]. */
static Oop callback(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)r;if(n!=1)return id.invalid;
    CHECK(send(get(c,0),COLLECT,0,0));
    CHECK(put(c,1,id_size(id_unbox(get(c,1))+1)));return a[0];
}
/* [GC,park,rejected]. */
static Oop boundary(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)a;if(n)return id.invalid;
    CHECK(send(get(c,0),COLLECT,0,0));
    if(get(c,1)==id_size(1)) {
        Oop result=send(r,PARK,0,0);CHECK(put(c,2,id_size(id_fault(result))));return result;
    }
    return send(r,ELIGIBLE,0,0);
}
/* [Machine,rejected]. */
static Oop trace(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)r;(void)a;if(n!=1)return id.invalid;
    Oop result=send(get(c,0),NEW,0,0);
    return put(c,1,id_size(id_fault(result)));
}
static Oop self(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)c;(void)a;(void)n;return r;
}
/* [answer,Integer]. Both single-PC and branch/join descriptor candidates. */
static Oop candidate(Oop root,Oop c,Oop machine,const Oop*a,size_t n) {
    (void)root;if(n>1)return id.invalid;
    if(n) {
        Oop bits=send(get(c,1),WORD_OF,a,1);CHECK(bits);
        size_t pc=id_unbox(bits);
        if(pc!=3) {Oop next=id_size(pc==0?1:3);return send(machine,CONTINUE,&next,1);}
    }
    Oop word=id_size(1),slot=send(get(c,1),FROM,&word,1);CHECK(slot);
    Oop args[]={slot,get(c,0)};CHECK(send(machine,WRITE,args,2));
    return send(machine,RETURN,&slot,1);
}
/* [baseline,empty,GC,monitor,answer identity,constructor alias,calls]. */
static Oop withdraw(Oop root,Oop c,Oop r,const Oop*a,size_t n) {
    (void)root;if(n!=1)return id.invalid;
    if(a[0]==id_size(1) && get(c,6)==id_size(0)) {
        CHECK(put(c,6,id_size(1)));
        Oop empty=get(c,1);CHECK(send(get(c,0),OPTIMIZE,&empty,1));
        CHECK(send(get(c,2),COLLECT,0,0));
        Oop identity=get(c,4);
        if(send(get(c,3),PRESENT,&identity,1)!=id_size(1))return id.invalid;
    }
    return id.message(id.root,r,get(c,5),a,n);
}
/* [baseline,new steps,Id vtable,proof selector,original proof,alias,calls]. */
static Oop proof(Oop root,Oop c,Oop r,const Oop*a,size_t n) {
    (void)root;
    CHECK(put(c,6,id_size(id_unbox(get(c,6))+1)));
    Oop steps=get(c,1);CHECK(send(get(c,0),OPTIMIZE,&steps,1));
    Oop binding[]={get(c,3),get(c,4)};CHECK(send(get(c,2),ADD,binding,2));
    return id.message(id.root,r,get(c,5),a,n);
}
static size_t wanted,found;
static Oop allocation(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
    if(id_unbox(a[2])==wanted)found++;return 0;
}
static Oop present(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)c;if(n!=1)return id.invalid;
    wanted=id_unbox(a[0]);found=0;CHECK(send(id.root,ALLOCATIONS,&r,1));return id_size(found);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n) {
    (void)a;if(n||count!=COUNT)return p->invalid;id=*p;
    for(size_t i=0;i<count;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={
    LIT("at:"),LIT("at:put:"),LIT("atWord:"),LIT("collectPrecisely"),
    LIT("wordOf:"),LIT("Integer"),LIT("returnAt:"),LIT("continueAt:"),
    LIT("oopAt:"),LIT("oopAt:put:"),LIT("suspendAt:continueAt:"),
    LIT("captureAt:continueAt:"),LIT("callSite:"),LIT("tailCallSite:"),
    LIT("park"),LIT("controlEligible"),LIT("new"),LIT("identityOf:"),
    LIT("allocationsDo:"),LIT("step"),LIT("callback"),LIT("boundary"),
    LIT("rootsDo:"),LIT("self"),LIT("allocation:bytes:identity:"),LIT("present"),
    LIT("candidate"),LIT("withdraw"),LIT("optimizeWith:"),LIT("fromWord:"),
    LIT("proof"),LIT("addMethod")
};
static const IdMethodSpec methods[]={
    {STEP,step},{CALLBACK,callback},{BOUNDARY,boundary},{TRACE,trace},
    {SELF,self},{ALLOCATION,allocation},{PRESENT,present},{CANDIDATE,candidate},{WITHDRAW,withdraw},{PROOF,proof}
};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,10,initialize};
