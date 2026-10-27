/* Relational snapshots, nested queries and ephemeral emitted Oops. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[14];static size_t wanted,alive;
enum{AT,PUT,COLLECT,FIND,SIZE,WORD,ALLOCATE,RETAIN,RELEASE,FROM,VALUE,IDENTITY,ALLOCATIONS,NEW};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
/* [GC,alias,calls,nest,Store,projection,clauses,Integer]. */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;CHECK(increment(c,2));CHECK(send(get(c,0),COLLECT,0,0));
 if(get(c,3)==id_size(1)){CHECK(put(c,3,id_size(0)));Oop args[]={get(c,5),get(c,6)},result=send(get(c,4),FIND,args,2);CHECK(result);Oop length=send(result,SIZE,0,0);CHECK(length);Oop count=send(get(c,7),WORD,&length,1);CHECK(count);if(count==id_size(0))return id.invalid;}
 return id.message(id.root,r,get(c,1),a,n);
}
/* [GC,EntityVt,Roots,Integer,attribute,escaped collector,entity identity,nil]. */
static Oop emit(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;CHECK(put(c,5,a[0]));Oop gc=get(c,0),vt=get(c,1),roots=get(c,2),integer=get(c,3),attr=get(c,4),zero=id_size(0);
 Oop entity=send(vt,ALLOCATE,&zero,1);CHECK(entity);Oop pin=send(roots,RETAIN,&entity,1);CHECK(pin);Oop identity=send(id.root,IDENTITY,&entity,1);CHECK(identity);CHECK(put(c,6,identity));
 Oop bits=id_size(13),value=send(integer,FROM,&bits,1);CHECK(value);Oop args[]={entity,attr,value};Oop result=send(a[0],VALUE,args,3);
 Oop released=send(roots,RELEASE,&pin,1);CHECK(released);CHECK(result);CHECK(send(gc,COLLECT,0,0));Oop still=send(id.root,IDENTITY,&entity,1);CHECK(still);if(still!=identity)return id.invalid;return get(c,7);
}
/* [GC,original Vector factory,calls]. */
static Oop factory(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(increment(c,2));CHECK(send(get(c,0),COLLECT,0,0));return send(get(c,1),NEW,0,0);}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)alive++;return 0;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);alive=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(alive);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=19)return p->invalid;id=*p;for(size_t i=0;i<14;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("findAll:where:"),LIT("size"),LIT("wordOf:"),LIT("allocate"),LIT("retain:"),LIT("release:"),LIT("fromWord:"),LIT("value:value:value:"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("new"),LIT("forward"),LIT("emit"),LIT("factory"),LIT("allocation:bytes:identity:"),LIT("present")};
static const IdMethodSpec methods[]={{14,forward},{15,emit},{16,factory},{17,allocation},{18,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,19,methods,5,initialize};
