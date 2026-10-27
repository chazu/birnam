/* Weak result identities observe the actual collecting Roots cleanup path. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT,GC,IDENTITY,ALLOCATIONS,FACTORY_ALIAS,ADD_ALIAS,RELEASE_ALIAS,FACTORY,ADD,CLEANUP,WATCH,LAST,LOST,CALLS,ROW,PRESENT,COUNT };
static Oop names[COUNT];static size_t last_id,wanted,found,calls,fail_mode;static int lost,inside;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found++;return 0;}
/* [GC, monitor, generated/batch failure, inventory failure]. */
static Oop present(Oop c,size_t identity){wanted=identity;found=0;Oop monitor=get(c,1);CHECK(send(id.root,ALLOCATIONS,&monitor,1));return id_size(found);}
static Oop remember(Oop result){CHECK(result);if(!result||((uintptr_t)result&1))return id.invalid;Oop identity=send(id.root,IDENTITY,&result,1);CHECK(identity);last_id=id_unbox(identity);return result;}
static Oop factory(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;return remember(send(r,FACTORY_ALIAS,a,n));}
static Oop add(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;return remember(send(r,ADD_ALIAS,a,n));}
static Oop cleanup(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop result=send(r,RELEASE_ALIAS,a,n);CHECK(result);
 if(last_id&&!inside){
  inside=1;Oop collected=send(get(c,0),GC,0,0);inside=0;CHECK(collected);
  Oop alive=present(c,last_id);CHECK(alive);calls++;if(alive!=id_size(1))lost=1;
  if(fail_mode){size_t mode=fail_mode;fail_mode=0;return id_failure(get(c,mode+1));}
 }
 return result;
}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=2||id_unbox(a[1])>2)return id.invalid;last_id=id_unbox(a[0]);fail_mode=id_unbox(a[1]);lost=0;calls=0;return 0;}
static Oop last(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(last_id);}
static Oop was_lost(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(lost);}
static Oop call_count(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(calls);}
static Oop present_method(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;return present(c,id_unbox(a[0]));}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("collectPrecisely"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("cleanupOriginalFactory:"),LIT("cleanupOriginalAdd:"),LIT("cleanupOriginalRelease:"),LIT("factory"),LIT("add"),LIT("cleanup"),LIT("watch:failure:"),LIT("lastIdentity"),LIT("lost"),LIT("calls"),LIT("allocation:bytes:identity:"),LIT("present:")};
static const IdMethodSpec methods[]={{FACTORY,factory},{ADD,add},{CLEANUP,cleanup},{WATCH,watch},{LAST,last},{LOST,was_lost},{CALLS,call_count},{ROW,row},{PRESENT,present_method}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,9,initialize};
