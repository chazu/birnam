/* Test-only allocated-object identity observation; no object payload access. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop allocations;static size_t wanted,found;
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found++;return 0;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);found=0;Oop result=id.message(id.root,id.root,allocations,&r,1);return id_fault(result)?result:id_size(found);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=3||n)return p->invalid;id=*p;allocations=s[0];return image;}
static const IdLiteral literals[]={LIT("allocationsDo:"),LIT("allocation:bytes:identity:"),LIT("present:")};
static const IdMethodSpec methods[]={{1,row},{2,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,3,methods,2,initialize};
