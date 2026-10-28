/* Constructor/admission collection through public Id messages only. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum {AT,PUT,RELEASE,GC,ALLOCATE_ALIAS,PARAMETER_ALIAS,ALLOCATIONS,ALLOCATE,PARAMETER,ROW,PRESENT,CONSTANT,COUNT};
static Oop names[COUNT];static size_t wanted,found;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
/* [GC, Roots, caller token or null, weak identity, target receiver, calls]. */
static Oop collect(Oop c,Oop r){
 Oop token=get(c,2);if(r!=get(c,4)||!token)return 0;
 CHECK(put(c,2,0));CHECK(put(c,5,id_size(id_unbox(get(c,5))+1)));
 CHECK(send(get(c,1),RELEASE,&token,1));return send(get(c,0),GC,0,0);
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;CHECK(collect(c,r));return send(r,ALLOCATE_ALIAS,a,n);}
static Oop parameter(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;CHECK(collect(c,r));return send(r,PARAMETER_ALIAS,a,n);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found++;return 0;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);found=0;CHECK(send(id.root,ALLOCATIONS,&r,1));return id_size(found);}
static Oop constant(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;return n?id.invalid:c;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=COUNT||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("release:"),LIT("collectPrecisely"),LIT("closureOriginalAllocate"),LIT("closureOriginalParameterCount:"),LIT("allocationsDo:"),LIT("allocation"),LIT("parameter"),LIT("allocation:bytes:identity:"),LIT("present:"),LIT("constant")};
static const IdMethodSpec methods[]={{ALLOCATE,allocation},{PARAMETER,parameter},{ROW,row},{PRESENT,present},{CONSTANT,constant}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,5,initialize};
