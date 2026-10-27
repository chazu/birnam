/* Literal geometry/threshold witnesses; cached selectors avoid measurement noise. */
#include "../oop-id-v2/id.h"
#include <stdio.h>
static IdProtocol id;static Oop names[20];
enum{AT,ALLOCATE,CHARGED,COLLECT,BUDGET,PREPARE,CHARGE,RETAIN,RELEASE,DETACH,FREE,COUNT,IDENTITY,TRACE,ADD,ORIGINAL,REFERENCES,GEOMETRY,RUN,OVERFLOW};
/* Context: [GC,Roots,behavior,root vtable,trace owner,geometry Method,fault,original Method]. */
static size_t visits,observed_charge,geometry_calls,nesting;static int fail_trace,collecting,huge;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked)){fprintf(stderr,"allocation accounting send failed at %d\n",__LINE__);return checked;}}while(0)
#define EXPECT(v) do{if(!(v)){fprintf(stderr,"allocation accounting assertion failed at %d\n",__LINE__);return id.invalid;}}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop allocate(Oop c,size_t bytes){Oop n=id_size(bytes);return send(get(c,2),ALLOCATE,&n,1);}
static Oop install(Oop c,int custom){Oop a[]={names[CHARGE],get(c,custom?5:7)};return send(get(c,3),ADD,a,2);}
static Oop references(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n!=2)return id.invalid;visits++;Oop charge=send(get(c,0),CHARGED,0,0);CHECK(charge);observed_charge=id_unbox(charge);return fail_trace?id_failure(get(c,6)):0;
}
static Oop geometry(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;geometry_calls++;
 if(collecting){collecting=0;CHECK(send(get(c,0),COLLECT,0,0));}
 if(nesting){nesting--;CHECK(allocate(c,0));}
 return huge?id_size(SIZE_MAX>>1):send(r,ORIGINAL,a,n);
}
static Oop overflow(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;
 CHECK(install(c,1));huge=1;nesting=2;
 /* Three maximum immediate charges cannot fit in the native accumulator.
    Nested construction prevents an automatic collection from resetting it. */
 return allocate(c,0);
}
static Oop run(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;Oop gc=get(c,0),roots=get(c,1);
 for(size_t round=0;round<2;round++){
  CHECK(send(gc,COLLECT,0,0));size_t previous=0;
  for(size_t i=0;i<16;i++){CHECK(allocate(c,1<<20));Oop charge=send(gc,CHARGED,0,0);CHECK(charge);EXPECT(id_unbox(charge)>previous);previous=id_unbox(charge);}
  EXPECT(previous>=16u<<20);CHECK(send(gc,PREPARE,0,0));EXPECT(send(gc,CHARGED,0,0)==id_size(0));
 }
 Oop one=id_size(1),maximum=id_size(SIZE_MAX>>1);CHECK(send(gc,BUDGET,&one,1));CHECK(allocate(c,0));EXPECT(send(gc,CHARGED,0,0)!=id_size(0));
 CHECK(send(gc,PREPARE,0,0));EXPECT(send(gc,CHARGED,0,0)==id_size(0));CHECK(send(gc,BUDGET,&maximum,1));CHECK(send(gc,COLLECT,0,0));
 const size_t payloads[]={0,1,7,8,9,15,16},charges[]={8,16,16,16,24,24,24};Oop values[7],pins[7];size_t expected=0;
 for(size_t i=0;i<7;i++){
  Oop bytes=id_size(payloads[i]);EXPECT(send(id.root,CHARGE,&bytes,1)==id_size(charges[i]));
  values[i]=allocate(c,payloads[i]);CHECK(values[i]);pins[i]=send(roots,RETAIN,values+i,1);CHECK(pins[i]);expected+=charges[i];EXPECT(send(gc,CHARGED,0,0)==id_size(expected));
 }
 EXPECT(expected==128);EXPECT(send(id.root,CHARGE,&maximum,1)==0);
 Oop absent=0;EXPECT(id_fault(send(id.root,CHARGE,&absent,1)));EXPECT(id_fault(send(gc,CHARGED,&one,1)));EXPECT(send(gc,CHARGED,0,0)==id_size(expected));
 CHECK(send(roots,RELEASE,pins+6,1));Oop handle=send(id.root,DETACH,values+6,1);CHECK(handle);values[6]=0;
 CHECK(send(id.root,FREE,&handle,1));EXPECT(id_fault(send(id.root,FREE,&handle,1)));EXPECT(send(gc,CHARGED,0,0)==id_size(expected));
 fail_trace=1;Oop count=send(id.root,COUNT,0,0);CHECK(count);EXPECT(send(gc,COLLECT,0,0)==id_failure(get(c,6)));
 EXPECT(send(gc,CHARGED,0,0)==id_size(expected));EXPECT(send(id.root,COUNT,0,0)==count);
 fail_trace=0;CHECK(send(gc,COLLECT,0,0));EXPECT(visits>0&&observed_charge==expected);EXPECT(send(gc,CHARGED,0,0)==id_size(0));
 CHECK(allocate(c,0));Oop eight=id_size(8);CHECK(send(gc,BUDGET,&eight,1));EXPECT(send(gc,CHARGED,0,0)==eight);CHECK(send(gc,PREPARE,0,0));EXPECT(send(gc,CHARGED,0,0)==id_size(0));CHECK(send(gc,BUDGET,&maximum,1));
 CHECK(install(c,1));collecting=1;Oop fresh=allocate(c,0);CHECK(fresh);CHECK(send(id.root,IDENTITY,&fresh,1));EXPECT(send(gc,CHARGED,0,0)==eight);EXPECT(geometry_calls==1);
 nesting=1;CHECK(allocate(c,0));EXPECT(send(gc,CHARGED,0,0)==id_size(24));EXPECT(geometry_calls==3);
 CHECK(install(c,0));CHECK(allocate(c,0));EXPECT(geometry_calls==3);EXPECT(send(gc,CHARGED,0,0)==id_size(32));
 for(size_t i=0;i<6;i++)CHECK(send(roots,RELEASE,pins+i,1));CHECK(send(gc,COLLECT,0,0));
 Oop unregister[]={get(c,2),0};CHECK(send(gc,TRACE,unregister,2));Oop budget=id_size(1<<20);CHECK(send(gc,BUDGET,&budget,1));return one;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 (void)a;if(count!=20||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("allocate"),LIT("allocationBytes"),LIT("collectPrecisely"),LIT("budget:"),LIT("prepareAllocation"),LIT("allocationChargeFor:"),LIT("retain:"),LIT("release:"),LIT("detachAllocation:"),LIT("releaseDetachedAllocation:"),LIT("allocationCount"),LIT("identityOf:"),LIT("trace:with:"),LIT("addMethod"),LIT("accountingOriginalGeometry:"),LIT("referencesOf:do:"),LIT("geometry"),LIT("run"),LIT("overflow")};
static const IdMethodSpec methods[]={{REFERENCES,references},{GEOMETRY,geometry},{RUN,run},{OVERFLOW,overflow}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,20,methods,4,initialize};
