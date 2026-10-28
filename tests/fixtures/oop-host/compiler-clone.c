/* Allocation observations use identities and current vtable lookup, not payloads. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[4];static size_t count,wanted,found;
enum {BEHAVIOR,ALLOCATIONS,ROW,COUNT};
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=3)return id.invalid;
 if(id_unbox(a[2])==wanted)found++;
 Oop behavior=id.message(id.root,id.root,names[BEHAVIOR],a,1);
 if(id_fault(behavior))return behavior;if(behavior==c)count++;return 0;
}
static Oop inventory(Oop r){count=found=0;Oop result=id.message(id.root,id.root,names[ALLOCATIONS],&r,1);return id_fault(result)?result:id_size(count);}
static Oop instances(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)a;return n?id.invalid:inventory(r);}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);Oop result=inventory(r);return id_fault(result)?result:id_size(found);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=5||n)return p->invalid;id=*p;for(size_t i=0;i<4;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("behaviorOf:"),LIT("allocationsDo:"),LIT("allocation:bytes:identity:"),LIT("instances"),LIT("present:")};
static const IdMethodSpec methods[]={{ROW,row},{COUNT,instances},{4,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,5,methods,3,initialize};
