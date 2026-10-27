/* External Bytes capacity and collecting policy calls, with Oop byte providers. */
#include "../oop-id-v2/id.h"
#include <stdio.h>
static IdProtocol id;static Oop names[16];static size_t calls,total;
enum{AT,FROM,APPEND,BYTES,SIZE,INDEX,COLLECT,PREPARE,CHARGED,CHARGE,AUTO,RETAIN,RELEASE,IDENTITY,ADD,ORIGINAL};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked)){fprintf(stderr,"external allocation send failed at %d\n",__LINE__);return checked;}}while(0)
#define EXPECT(v) do{if(!(v)){fprintf(stderr,"external allocation assertion failed at %d\n",__LINE__);return id.invalid;}}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
/* Context: [GC,Roots,Bytes,large provider,GC vtable,original charge,hook,
             one-byte input,suffix,empty input,restored input]. */
static Oop collecting_charge(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1||!((uintptr_t)a[0]&1))return id.invalid;calls++;total+=id_unbox(a[0]);CHECK(send(get(c,0),COLLECT,0,0));return send(r,ORIGINAL,a,n);
}
static Oop large_size(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(16u<<20);}
static Oop large_byte(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;return n==1&&((uintptr_t)a[0]&1)&&id_unbox(a[0])<(16u<<20)?id_size(7):id.invalid;}
static Oop run(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;Oop gc=get(c,0),roots=get(c,1),factory=get(c,2);
 for(size_t i=0;i<2;i++){
  CHECK(send(gc,COLLECT,0,0));Oop provider=get(c,3),value=send(factory,FROM,&provider,1);CHECK(value);Oop pin=send(roots,RETAIN,&value,1);CHECK(pin);
  Oop charge=send(gc,CHARGED,0,0);CHECK(charge);EXPECT(id_unbox(charge)>=(16u<<20));CHECK(send(gc,PREPARE,0,0));EXPECT(send(gc,CHARGED,0,0)==id_size(0));CHECK(send(id.root,IDENTITY,&value,1));CHECK(send(roots,RELEASE,&pin,1));
 }
 Oop zero=id_size(0),one=id_size(1),amount=id_size(16u<<20);CHECK(send(gc,AUTO,&zero,1));CHECK(send(gc,COLLECT,0,0));CHECK(send(gc,CHARGE,&amount,1));CHECK(send(gc,PREPARE,0,0));EXPECT(send(gc,CHARGED,0,0)==amount);
 CHECK(send(gc,AUTO,&one,1));CHECK(send(gc,PREPARE,0,0));EXPECT(send(gc,CHARGED,0,0)==zero);Oop absent=0;EXPECT(id_fault(send(gc,CHARGE,&absent,1)));CHECK(send(gc,CHARGE,&zero,1));EXPECT(send(gc,CHARGED,0,0)==zero);
 Oop replacement[]={names[CHARGE],get(c,6)};CHECK(send(get(c,4),ADD,replacement,2));
 Oop input=get(c,7),value=send(factory,FROM,&input,1);CHECK(value);Oop pin=send(roots,RETAIN,&value,1);CHECK(pin);EXPECT(total==1);
 Oop suffix=get(c,8);CHECK(send(value,APPEND,&suffix,1));Oop view=send(value,BYTES,0,0);CHECK(view);Oop view_pin=send(roots,RETAIN,&view,1);CHECK(view_pin);
 EXPECT(send(view,SIZE,0,0)==id_size(4097));
 for(size_t i=0;i<4097;i++){Oop at=id_size(i);EXPECT(send(view,INDEX,&at,1)==id_size(i?42:'a'));}
 EXPECT(total>=4097);size_t before=total;Oop empty=get(c,9);CHECK(send(value,APPEND,&empty,1));EXPECT(total==before);EXPECT(calls>=2);
 replacement[1]=get(c,5);CHECK(send(get(c,4),ADD,replacement,2));before=calls;input=get(c,10);CHECK(send(factory,FROM,&input,1));EXPECT(calls==before);
 CHECK(send(roots,RELEASE,&view_pin,1));CHECK(send(roots,RELEASE,&pin,1));CHECK(send(gc,COLLECT,0,0));return one;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 (void)a;if(count!=20||n)return p->invalid;id=*p;for(size_t i=0;i<16;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("fromBytes:"),LIT("appendBytes:"),LIT("bytes"),LIT("sizeWord"),LIT("atWord:"),LIT("collectPrecisely"),LIT("prepareAllocation"),LIT("allocationBytes"),LIT("chargeExternalAllocation:"),LIT("automaticCollection:"),LIT("retain:"),LIT("release:"),LIT("identityOf:"),LIT("addMethod"),LIT("externalOriginalCharge:"),LIT("collectingCharge"),LIT("largeSize"),LIT("largeByte"),LIT("run")};
static const IdMethodSpec methods[]={{16,collecting_charge},{17,large_size},{18,large_byte},{19,run}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,20,methods,4,initialize};
