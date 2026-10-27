/* Embedded-file callbacks and weak identity observations use only Id messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[7];
enum{AT,PUT,COLLECT,FORWARD,EMBEDDED,ALLOCATION,RETAIN};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop pair[]={id_size(i),v};return send(c,PUT,pair,2);}
/* [GC,alias,calls,observed receiver or null] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop observed=get(c,3);if(!observed||observed==r)CHECK(put(c,2,id_size(id_unbox(get(c,2))+1)));
 CHECK(send(get(c,0),COLLECT,0,0));return id.message(id.root,r,get(c,1),a,n);
}
/* [GC,value,calls,last path,failure or null] */
static Oop embedded(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;
 CHECK(put(c,3,a[0]));CHECK(put(c,2,id_size(id_unbox(get(c,2))+1)));CHECK(send(get(c,0),COLLECT,0,0));
 Oop failure=get(c,4);return failure?id_failure(failure):get(c,1);
}
/* [allocation identity,seen] -- does not retain the observed allocation. */
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=3)return id.invalid;
 if(a[2]==get(c,0))CHECK(put(c,1,id_size(1)));return 0;
}
/* Context is Roots. Like Id's native image, this test owner lives until exit. */
static Oop retain(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;CHECK(send(c,RETAIN,a,1));return r;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){
 (void)a;if(n!=7||count)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("forward"),LIT("embedded"),LIT("allocation:bytes:identity:"),LIT("retain:")};
static const IdMethodSpec methods[]={{FORWARD,forward},{EMBEDDED,embedded},{ALLOCATION,allocation},{RETAIN,retain}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,7,methods,4,initialize};
