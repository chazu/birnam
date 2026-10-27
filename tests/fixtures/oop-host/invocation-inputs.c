/* Declared checkpoints release the caller's lease and inspect allocation identities. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, RELEASE, GC, IDENTITY, ALLOCATIONS, CHECKPOINT, FAIL, ROW, COUNT };
static Oop names[COUNT];
static size_t wanted[2], seen;
#define CHECK(v) do { Oop checked=(v);if(id_fault(checked))return checked; }while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop put(Oop c,size_t i,Oop value){Oop a[]={id_size(i),value};return send(c,PUT,a,2);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
 for(size_t i=0;i<2;i++)if(wanted[i]==id_unbox(a[2]))seen|=(size_t)1<<i;
 return 0;
}
/* [GC, Roots, external token or null, calls, failure]. */
static Oop checkpoint(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=2)return id.invalid;seen=0;
 for(size_t i=0;i<2;i++){
  if((uintptr_t)a[i]&1){wanted[i]=0;seen|=(size_t)1<<i;}
  else {Oop identity=send(id.root,IDENTITY,a+i,1);CHECK(identity);wanted[i]=id_unbox(identity);}
 }
 Oop token=get(c,2);CHECK(put(c,2,0));
 if(token)CHECK(send(get(c,1),RELEASE,&token,1));
 CHECK(send(get(c,0),GC,0,0));CHECK(send(id.root,ALLOCATIONS,&r,1));
 if(seen!=3)return id.invalid;
 CHECK(put(c,3,id_size(id_unbox(get(c,3))+1)));
 return a[1];
}
static Oop fail(Oop root,Oop c,Oop r,const Oop*a,size_t n){CHECK(checkpoint(root,c,r,a,n));return id_failure(get(c,4));}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("release:"),LIT("collectPrecisely"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("check:with:"),LIT("fail:with:"),LIT("allocation:bytes:identity:")};
static const IdMethodSpec methods[]={{CHECKPOINT,checkpoint},{FAIL,fail},{ROW,row}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,3,initialize};
