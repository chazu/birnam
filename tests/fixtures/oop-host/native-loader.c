/* Native admission and collecting argument providers, independently declared. */
#include "../oop-id-v2/id.h"
#ifdef LOADER_MISSING
int not_a_module(void){return 1;}
#elif defined(LOADER_TARGET) || defined(LOADER_INCOMPATIBLE)
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t names,const Oop*a,size_t n){
 (void)image;if(names!=2)return p->invalid;
 if(n==0)return 0;
 if(n==1)return a[0];
 if(n==2){Oop result=p->message(p->root,a[0],s[0],0,0);if(id_fault(result))return result;
  result=p->message(p->root,p->root,s[1],a+1,1);if(id_fault(result))return result;return a[1];}
 if(n==3)return id_size(17);
 return p->invalid;
}
static const IdLiteral literals[]={LIT("collectPrecisely"),LIT("identityOf:")};
#ifdef LOADER_INCOMPATIBLE
const IdModule birnam_native_module_v2={99,sizeof(IdModule),literals,2,0,0,initialize};
#else
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,2,0,0,initialize};
#endif
#else
static IdProtocol id;static Oop names[12],made[64];static size_t made_count,observed_count,live;
enum {AT,PUT,COLLECT,FROM,WORD,ALLOCATE,IDENTITY,ALLOCATIONS,SIZE_WORD,ERROR,SIZE,INDEX};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop key=id_size(i);return send(c,AT,&key,1);}
static Oop put(Oop c,size_t i,Oop v){Oop args[]={id_size(i),v};return send(c,PUT,args,2);}
/* [GC,Integer,value vtable,length bits,wrong size,failure,path bytes,mutate,seen,seen count] */
static Oop size_value(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));
 if(get(c,7)==id_size(1)){Oop path=get(c,6),length=send(path,SIZE_WORD,0,0);CHECK(length);for(size_t i=0;i<id_unbox(length);i++)CHECK(put(path,i,id_size('x')));}
 if(get(c,4)==id_size(1))return get(c,0);Oop bits=get(c,3);return send(get(c,1),FROM,&bits,1);
}
static Oop at_value(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));
 Oop failure=get(c,5);if(failure)return id_failure(failure);
 Oop index=send(get(c,1),WORD,a,1);CHECK(index);size_t seen=id_unbox(get(c,9));CHECK(put(get(c,8),seen,index));CHECK(put(c,9,id_size(seen+1)));
 if(index==id_size(0)&&get(c,3)==id_size(2))return get(c,0);
 Oop zero=id_size(0),value=send(get(c,2),ALLOCATE,&zero,1);CHECK(value);
 if(made_count==64)return id.invalid;made[made_count]=send(id.root,IDENTITY,&value,1);CHECK(made[made_count]);made_count++;return value;
}
/* [GC,alias,calls,failure,path,path bytes,Namespace,published Error] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;CHECK(send(get(c,0),COLLECT,0,0));CHECK(put(c,2,id_size(id_unbox(get(c,2))+1)));
 Oop failure=get(c,3);if(failure)return id_failure(failure);
 Oop publish=get(c,7);if(publish){Oop args[]={names[ERROR],publish};CHECK(send(get(c,6),PUT,args,2));}
 if(r==get(c,4))return get(c,5);
 return id.message(id.root,r,get(c,1),a,n);
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
 for(size_t i=made_count-observed_count;i<made_count;i++)if(a[2]==made[i])live++;return 0;
}
static Oop remaining(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;observed_count=id_unbox(a[0]);if(observed_count>made_count)return id.invalid;
 live=0;CHECK(send(id.root,ALLOCATIONS,&c,1));return id_size(live);
}
static Oop last_identity(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;(void)a;return n||!made_count?id.invalid:made[made_count-1];
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("fromWord:"),LIT("wordOf:"),LIT("allocate"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("sizeWord"),LIT("Error"),LIT("size"),LIT("index"),LIT("forward"),LIT("allocation:bytes:identity:"),LIT("remaining"),LIT("lastCreatedIdentity")};
static const IdMethodSpec methods[]={{SIZE,size_value},{INDEX,at_value},{12,forward},{13,allocation},{14,remaining},{15,last_identity}};
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){
 (void)a;if(n!=16||count)return p->invalid;id=*p;for(size_t i=0;i<12;i++)names[i]=s[i];return image;
}
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,16,methods,6,initialize};
#endif
