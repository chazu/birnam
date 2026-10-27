/* Owner callbacks and independent raw-byte oracle for test-created ObjectModel storage. */
#include "../oop-id-v2/id.h"
#include <stdio.h>
static Oop extent_query,charge_query;static IdProtocol id;static Oop names[11];static size_t watched[8],watch_count,mask,behaviors,finalized,expected_final,binding_count;static Oop final_behavior,last_binding;
enum{AT,PUT,COLLECT,PRECISE,IDENTITY,ALLOCATIONS,IS_BEHAVIOR,OWNER,ENSURE,ALLOCATE,SLOT_PUT};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked)){fprintf(stderr,"ObjectModel fixture send failed at %d\n",__LINE__);return checked;}}while(0)
#define EXPECT(v) do{if(!(v)){fprintf(stderr,"ObjectModel fixture assertion failed at %d\n",__LINE__);return id.invalid;}}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
/* [alias,calls]. Forward without allocating an argument container in retirement. */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;CHECK(put(c,1,id_size(id_unbox(get(c,1))+1)));return id.message(id.root,r,get(c,0),a,n);}
/* [GC,ObjectModel,source,alias,entered]. */
static Oop reenter(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(get(c,4)==id_size(0)){CHECK(put(c,4,id_size(1)));Oop change[]={id_size(0),get(c,2),0};CHECK(send(get(c,1),SLOT_PUT,change,3));CHECK(send(get(c,0),PRECISE,0,0));}return id.message(id.root,r,get(c,3),a,n);
}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;EXPECT(n<=8);watch_count=n;for(size_t i=0;i<n;i++){Oop identity=send(id.root,IDENTITY,a+i,1);CHECK(identity);watched[i]=id_unbox(identity);}return 0;}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;for(size_t i=0;i<watch_count;i++)if(id_unbox(a[2])==watched[i])mask|=(size_t)1<<i;Oop b=send(id.root,IS_BEHAVIOR,a,1);CHECK(b);if(b==id_size(1))behaviors++;return 0;}
/* [GC,ObjectModel,base,probe]. */
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;mask=behaviors=0;Oop probe=get(c,3);CHECK(send(id.root,ALLOCATIONS,&probe,1));return id_size(mask);}
static Oop behavior_count(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(behaviors);}
static Oop binding(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;binding_count++;last_binding=a[0];EXPECT(a[2]==id_size(0));return 0;}
static Oop bindings(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;EXPECT(binding_count==1);return last_binding;}
static Oop watch_final(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=2)return id.invalid;Oop identity=send(id.root,IDENTITY,a,1);CHECK(identity);expected_final=id_unbox(identity);final_behavior=a[1];return 0;}
static Oop finalize(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;Oop identity=send(id.root,IDENTITY,a,1);CHECK(identity);EXPECT(id_unbox(identity)==expected_final);EXPECT(send(id.root,OWNER,&final_behavior,1)==0);
 Oop base=get(c,2);EXPECT(id_fault(send(id.root,ENSURE,&base,1)));Oop args[]={base,id_size(0)};EXPECT(id_fault(send(get(c,1),ALLOCATE,args,2)));EXPECT(send(get(c,0),COLLECT,0,0)==id_size(0));finalized++;return 0;
}
static Oop final_count(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(finalized);}
/* This oracle defines the content of the opaque test allocation and its padding. */
static Oop fill_raw(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;EXPECT(id.message(id.root,id.root,extent_query,a,1)==id_size(9));Oop requested=id_size(9);EXPECT(id.message(id.root,id.root,charge_query,&requested,1)==id_size(24));for(size_t i=0;i<16;i++)((unsigned char*)a[0])[i]=(unsigned char)(i+7);return a[0];}
static Oop check_raw(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;EXPECT(id.message(id.root,id.root,extent_query,a,1)==id_size(16));for(size_t i=0;i<16;i++)EXPECT(((unsigned char*)a[0])[i]==i+7);return id_size(1);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=26||n)return p->invalid;id=*p;extent_query=s[24];charge_query=s[25];for(size_t i=0;i<11;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collect"),LIT("collectPrecisely"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("isBehavior:"),LIT("ownerOf:"),LIT("ensurePrivateVtableFor:"),LIT("allocate:bytes:"),LIT("slotAt:in:put:"),LIT("forward"),LIT("reenter"),LIT("watch"),LIT("allocation:bytes:identity:"),LIT("mask"),LIT("behaviorCount"),LIT("binding:value:weak:"),LIT("binding"),LIT("watchFinal"),LIT("finalize:"),LIT("finalCount"),LIT("fillRaw"),LIT("checkRaw"),LIT("allocationSizeOf:"),LIT("allocationChargeFor:")};
static const IdMethodSpec methods[]={{11,forward},{12,reenter},{13,watch},{14,allocation},{15,present},{16,behavior_count},{17,binding},{18,bindings},{19,watch_final},{20,finalize},{21,final_count},{22,fill_raw},{23,check_raw}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,26,methods,13,initialize};
