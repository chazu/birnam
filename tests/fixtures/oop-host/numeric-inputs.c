/* Numeric owner scope checks use messages and weak allocation identities. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, GC, IDENTITY, ALLOCATIONS, FROM_BITS, QUERY_ALIAS, RETAIN_ALIAS, QUERY, RETAIN, ROW, PRESENT, COUNT };
static Oop names[COUNT];static size_t wanted,found;
#define CHECK(v) do {Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
static Oop identity(Oop value){if(!value||((uintptr_t)value&1))return id_size(0);return send(id.root,IDENTITY,&value,1);}
/* [GC, Word, monitor, all retains, input retains, collections, active,
    collecting, nested left, nested identity, fail after, failed identity,
    failure, input identity sequence]. */
static int input(Oop c,Oop key){for(size_t i=0;i<5;i++)if(key==get(get(c,13),i))return 1;return 0;}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found++;return 0;}
static Oop present(Oop c,Oop key){wanted=id_unbox(key);found=0;Oop monitor=get(c,2);CHECK(send(id.root,ALLOCATIONS,&monitor,1));return id_size(found);}
static Oop present_method(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;return present(c,a[0]);}
static Oop retain(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;CHECK(increment(c,3));Oop key=identity(a[0]);CHECK(key);
 if(key!=id_size(0)&&input(c,key))CHECK(increment(c,4));
 return send(r,RETAIN_ALIAS,a,n);
}
static Oop query(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;
 int top=get(c,6)==id_size(0);CHECK(put(c,6,id_size(1)));
 Oop key=identity(a[0]);CHECK(key);
 if(get(c,7)==id_size(0)){
  CHECK(put(c,7,id_size(1)));CHECK(send(get(c,0),GC,0,0));
  for(size_t i=0;i<5;i++)if(present(c,get(get(c,13),i))!=id_size(1))return id.invalid;
  if(key!=id_size(0)&&present(c,key)!=id_size(1))return id.invalid;
  CHECK(increment(c,5));CHECK(put(c,7,id_size(0)));
 }
 if(top){
  size_t left=id_unbox(get(c,8));
  if(left&&key!=id_size(0)&&!input(c,key)){
   CHECK(put(c,8,id_size(left-1)));Oop value=id_size(777),child=send(get(c,1),FROM_BITS,&value,1);CHECK(child);
   Oop child_id=identity(child);CHECK(child_id);CHECK(put(c,9,child_id));
   CHECK(put(c,7,id_size(1)));CHECK(send(get(c,0),GC,0,0));CHECK(put(c,7,id_size(0)));
   if(present(c,key)!=id_size(1)||present(c,child_id)!=id_size(0))return id.invalid;
  }
  CHECK(put(c,6,id_size(0)));size_t until=id_unbox(get(c,10));
  if(until){CHECK(put(c,10,id_size(until-1)));if(until==1){CHECK(put(c,11,key));return id_failure(get(c,12));}}
 }
 return send(r,QUERY_ALIAS,a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("fromBits:"),LIT("numericOriginalBehavior:"),LIT("numericOriginalRetain:"),LIT("query"),LIT("retain"),LIT("allocation:bytes:identity:"),LIT("present:")};
static const IdMethodSpec methods[]={{QUERY,query},{RETAIN,retain},{ROW,row},{PRESENT,present_method}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,4,initialize};
