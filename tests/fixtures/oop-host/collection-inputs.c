/* Collection callbacks observe weak identities; new Vector nodes are explicit roots. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT,PUT,GC,RELEASE,IDENTITY,ALLOCATIONS,REHASH,SLOT_PUT,RETAIN_ALIAS,VECTOR_ALIAS,HASH,QUERY,RETAIN,VECTOR,ROW,PRESENT,COUNT };
static Oop names[COUNT];static size_t wanted;static Oop found;
enum { Gc,Roots,Model,Monitor,Inputs,InputCount,Retains,MapId,KeyId,MapToken,KeyToken,Active,Failed,Calls,Mutate,OldNode,VectorMode,OldRoot,OldTail,NewRoot,NewTail,OnlyVector,InsideVector,QueryAlias,HashFailure,ReadFailure };
#define CHECK(v) do {Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
static Oop identity(Oop value){if(!value||((uintptr_t)value&1))return id_size(0);return send(id.root,IDENTITY,&value,1);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found=a[0];return 0;}
static Oop find(Oop c,Oop key){wanted=id_unbox(key);found=0;Oop monitor=get(c,Monitor);CHECK(send(id.root,ALLOCATIONS,&monitor,1));return found;}
static Oop require_live(Oop c,Oop key){Oop v=find(c,key);CHECK(v);return v?0:id.invalid;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;Oop v=find(c,a[0]);CHECK(v);return id_size(v!=0);}
static int input(Oop c,Oop key){size_t n=id_unbox(get(c,InputCount));for(size_t i=0;i<n;i++)if(key==get(get(c,Inputs),i))return 1;return 0;}
static Oop live_inputs(Oop c){size_t n=id_unbox(get(c,InputCount));for(size_t i=0;i<n;i++){Oop v=find(c,get(get(c,Inputs),i));CHECK(v);if(!v)return id.invalid;}return 0;}
static Oop release_inputs(Oop c){for(size_t i=MapToken;i<=KeyToken;i++){Oop token=get(c,i);CHECK(put(c,i,0));if(token)CHECK(send(get(c,Roots),RELEASE,&token,1));}return 0;}
static Oop retain(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;Oop key=identity(a[0]);CHECK(key);
 if((get(c,OnlyVector)==id_size(0)||get(c,InsideVector)==id_size(1))&&key!=id_size(0)&&input(c,key))CHECK(increment(c,Retains));
 return send(r,RETAIN_ALIAS,a,n);
}
static Oop vector(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop previous=get(c,InsideVector),key=identity(r);CHECK(key);
 CHECK(put(c,InsideVector,id_size(key==get(c,MapId))));
 Oop result=send(r,VECTOR_ALIAS,a,n);CHECK(put(c,InsideVector,previous));return result;
}
static Oop hash(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;
 if(get(c,Active)==id_size(0)){
  CHECK(put(c,Active,id_size(1)));CHECK(release_inputs(c));
  if(get(c,Mutate)==id_size(1)){Oop map=find(c,get(c,MapId));CHECK(map);if(!map)return id.invalid;CHECK(send(map,REHASH,&map,1));}
  CHECK(send(get(c,Gc),GC,0,0));CHECK(live_inputs(c));
  if(get(c,Mutate)==id_size(1))CHECK(require_live(c,get(c,OldNode)));
  CHECK(increment(c,Calls));CHECK(put(c,Active,id_size(0)));
  if(get(c,Failed)==id_size(1))return id_failure(get(c,HashFailure));
 }
 return id_size(42);
}
static Oop query(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;
 if(get(c,Active)==id_size(0)){
  CHECK(put(c,Active,id_size(1)));
  if(get(c,VectorMode)==id_size(1)&&n&&identity(a[0])==get(c,KeyId)){
   CHECK(release_inputs(c));Oop map=find(c,get(c,MapId));CHECK(map);if(!map)return id.invalid;
   for(size_t i=0;i<2;i++){Oop values[]={id_size(2+i),map,get(c,NewRoot+i)};CHECK(send(get(c,Model),SLOT_PUT,values,3));}
   CHECK(send(get(c,Gc),GC,0,0));
   CHECK(require_live(c,get(c,OldRoot)));CHECK(require_live(c,get(c,OldTail)));
  }
  CHECK(send(get(c,Gc),GC,0,0));CHECK(live_inputs(c));
  CHECK(increment(c,Calls));CHECK(put(c,Active,id_size(0)));
  if(get(c,Failed)==id_size(1))return id_failure(get(c,ReadFailure));
 }
 return id.message(id.root,r,get(c,QueryAlias),a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("release:"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("rehashImage:"),LIT("slotAt:in:put:"),LIT("collectionOriginalRetain:"),LIT("collectionOriginalVectorAt:"),LIT("hash"),LIT("query"),LIT("retain"),LIT("vector"),LIT("allocation:bytes:identity:"),LIT("present:")};
static const IdMethodSpec methods[]={{HASH,hash},{QUERY,query},{RETAIN,retain},{VECTOR,vector},{ROW,row},{PRESENT,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,6,initialize};
