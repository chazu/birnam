/* Independent allocation records. The measured window uses cached Id selectors. */
#include "../oop-id-v2/id.h"
#include <stdlib.h>
static int is_size(Oop value){return ((uintptr_t)value&1)!=0;}
static Oop prepare;static IdProtocol id;static Oop names[9];
enum{AT,COLLECT,ALLOCATIONS,EVAL,CHARGED,ORIGINAL,RUN,ALLOCATION,EXTERNAL};
typedef struct{size_t identity,bytes;}Record;
static Record *records[2];static size_t counts[2],capacities[2],active,external,measured,measured_external;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop key=id_size(i);return send(c,AT,&key,1);}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=3||!is_size(a[1])||!is_size(a[2]))return id.invalid;
 size_t count=counts[active];if(count==capacities[active]){
  size_t capacity=count?count*2:1024;if(capacity>SIZE_MAX/sizeof(Record))return id.invalid;
  Record*p=realloc(records[active],capacity*sizeof(Record));if(!p)return id.invalid;records[active]=p;capacities[active]=capacity;
 }
 records[active][count]=(Record){id_unbox(a[2]),id_unbox(a[1])};counts[active]++;return 0;
}
static Oop external_charge(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;if(n!=1||!is_size(a[0]))return id.invalid;
 Oop result=send(r,ORIGINAL,a,n);CHECK(result);size_t bytes=id_unbox(a[0]);if(bytes>SIZE_MAX-external)return id.invalid;external+=bytes;return result;
}
/* [GC,Codegen,source String]. Inventory and counter reads allocate no Oops. */
static Oop run(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)a;if(n)return id.invalid;
 Oop gc=get(c,0),compiler=get(c,1),source=get(c,2);CHECK(send(gc,COLLECT,0,0));
 external=0;counts[0]=counts[1]=0;active=0;CHECK(send(id.root,ALLOCATIONS,&r,1));
 if(send(gc,CHARGED,0,0)!=id_size(0))return id.invalid;
 Oop result=send(compiler,EVAL,&source,1);CHECK(result);
 active=1;CHECK(send(id.root,ALLOCATIONS,&r,1));
 /* Raw Id transfer allocations join pressure at the existing policy checkpoint. */
 CHECK(id.message(id.root,gc,prepare,0,0));Oop charge=send(gc,CHARGED,0,0);CHECK(charge);if(!is_size(charge))return id.invalid;
 measured=id_unbox(charge);measured_external=external;return result;
}
static Oop record_count(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=1||!is_size(a[0])||id_unbox(a[0])>1)return id.invalid;return id_size(counts[id_unbox(a[0])]);
}
static Oop record(Oop a,Oop b,int bytes){
 if(!is_size(a)||!is_size(b))return id.invalid;size_t i=id_unbox(a),which=id_unbox(b);
 if(which>1||i>=counts[which])return id.invalid;Record value=records[which][i];return id_size(bytes?value.bytes:value.identity);
}
static Oop identity_at(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;return n==2?record(a[0],a[1],0):id.invalid;}
static Oop bytes_at(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;return n==2?record(a[0],a[1],1):id.invalid;}
static Oop charge(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(measured);}
static Oop external_bytes(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(measured_external);}
static Oop clear(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;for(size_t i=0;i<2;i++){free(records[i]);records[i]=0;counts[i]=capacities[i]=0;}return 0;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 (void)a;if(count!=16||n)return p->invalid;id=*p;prepare=s[15];for(size_t i=0;i<9;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("collectPrecisely"),LIT("allocationsDo:"),LIT("eval:"),LIT("allocationBytes"),LIT("compilerOriginalCharge:"),LIT("run"),LIT("allocation:bytes:identity:"),LIT("chargeExternalAllocation:"),LIT("recordCount:"),LIT("identityAt:in:"),LIT("bytesAt:in:"),LIT("measuredCharge"),LIT("measuredExternal"),LIT("clear"),LIT("prepareAllocation")};
static const IdMethodSpec methods[]={{RUN,run},{ALLOCATION,allocation},{EXTERNAL,external_charge},{9,record_count},{10,identity_at},{11,bytes_at},{12,charge},{13,external_bytes},{14,clear}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,16,methods,9,initialize};
