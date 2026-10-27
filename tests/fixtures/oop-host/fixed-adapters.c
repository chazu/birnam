/* Fixed adapter callbacks use only owner messages and weak input identities. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum {AT,PUT,GC,RELEASE,IDENTITY,ALLOCS,ADD,FROM_BITS,FACTORY_ALIAS,OPTIONAL,OBJECT,BITS,DENY,FACTORY,ROW,COUNT};static Oop names[COUNT];static size_t wanted,found;
/* [GC, Roots, Word, WordVt, targetVt, expected identity, external token,
    target calls, factory calls, mode, replace factory, replace target,
    factory Method, deny Method, full Word, nil, invalid Word,
    optional failure, object failure, replacement failure, monitor, getter calls]. */
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
static Oop identity(Oop o){if(!o||((uintptr_t)o&1))return id_size(0);return send(id.root,IDENTITY,&o,1);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found++;return 0;}
static Oop collect(Oop c){
 Oop token=get(c,6);CHECK(put(c,6,0));if(token)CHECK(send(get(c,1),RELEASE,&token,1));
 CHECK(send(get(c,0),GC,0,0));wanted=id_unbox(get(c,5));if(!wanted)return 0;
 found=0;Oop monitor=get(c,20);CHECK(send(id.root,ALLOCS,&monitor,1));return found==1?0:id.invalid;
}
static Oop factory(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;CHECK(increment(c,8));CHECK(collect(c));return send(r,FACTORY_ALIAS,a,n);}
static Oop optional(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;Oop key=identity(a[0]);CHECK(key);if(key!=get(c,5))return id.invalid;
 CHECK(collect(c));CHECK(increment(c,7));
 if(get(c,10)==id_size(1)){Oop binding[]={names[FROM_BITS],get(c,12)};CHECK(send(get(c,3),ADD,binding,2));}
 switch(id_unbox(get(c,9))){case 1:return 0;case 2:return get(c,2);case 3:return id_failure(get(c,17));case 4:return get(c,14);case 5:return id_size(0);case 6:return get(c,15);case 7:return get(c,16);default:return id_size(37);}
}
static Oop object(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1||a[0]!=id_size(37))return id.invalid;CHECK(collect(c));CHECK(increment(c,7));
 switch(id_unbox(get(c,9))){case 1:return 0;case 2:return id_size(37);case 3:return id_failure(get(c,18));default:{Oop value=id_size(71);return send(get(c,2),FROM_BITS,&value,1);}}
}
static Oop bits(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(increment(c,21));CHECK(collect(c));
 if(get(c,11)==id_size(1)){Oop binding[]={names[OBJECT],get(c,13)};CHECK(send(get(c,4),ADD,binding,2));}
 return id_size(37);
}
static Oop deny(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;return id_failure(get(c,19));}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("release:"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("addMethod"),LIT("fromBits:"),LIT("fixedOriginalFactory:"),LIT("optional:"),LIT("object:"),LIT("bits"),LIT("deny"),LIT("factory"),LIT("allocation:bytes:identity:")};
static const IdMethodSpec methods[]={{OPTIONAL,optional},{OBJECT,object},{BITS,bits},{DENY,deny},{FACTORY,factory},{ROW,row}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,6,initialize};
