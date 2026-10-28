/* Root cleanup observations through Id messages; no runtime payload access. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT,PUT,SEQUENCE,REFERENCES,ADD,RELEASE,ALIAS,COLLECT,VISIT,SNAPSHOT,
       FIRST,SECOND,LOG,RESET,COUNT };
static Oop names[COUNT];
static Oop watched[4], order[512], calls[512];
static size_t order_count,call_count;
static int collecting;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
/* Snapshot context: [Roots, Sequence of four watched objects]. */
static Oop visit(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=1)return id.invalid;
 for(size_t i=0;i<4;i++)if(a[0]==watched[i]){if(order_count==512)return id.invalid;order[order_count++]=id_size(i);}
 return 0;
}
static Oop snapshot(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)a;if(n)return id.invalid;
 for(size_t i=0;i<4;i++)watched[i]=get(get(c,1),i);
 order_count=0;Oop roots=get(c,0), args[]={roots,r};CHECK(send(roots,REFERENCES,args,2));
 return send(id.root,SEQUENCE,order,order_count);
}
/* Release context: [behavior, next Method or null, GC, failure or null]. */
static Oop cleanup(Oop c,Oop r,const Oop*a,size_t n,size_t tag){
 if(n!=1||call_count+2>512)return id.invalid;
 if(!collecting){calls[call_count++]=id_size(tag);calls[call_count++]=a[0];}
 Oop next=get(c,1);if(next){CHECK(put(c,1,0));Oop change[]={names[RELEASE],next};CHECK(send(get(c,0),ADD,change,2));}
 Oop result=send(r,ALIAS,a,n);CHECK(result);
 if(!collecting){collecting=1;Oop answer=send(get(c,2),COLLECT,0,0);collecting=0;CHECK(answer);}
 Oop failure=get(c,3);return failure?id_failure(failure):result;
}
static Oop first(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;return cleanup(c,r,a,n,1);}
static Oop second(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;return cleanup(c,r,a,n,2);}
static Oop log_values(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;return send(id.root,SEQUENCE,calls,call_count);}
static Oop reset(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;call_count=0;return 0;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=COUNT||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("sequence"),LIT("referencesOf:do:"),LIT("addMethod"),LIT("release:"),LIT("batchOriginalRelease:"),LIT("collectPrecisely"),LIT("visit:"),LIT("snapshot"),LIT("first"),LIT("second"),LIT("log"),LIT("reset")};
static const IdMethodSpec methods[]={{VISIT,visit},{SNAPSHOT,snapshot},{FIRST,first},{SECOND,second},{LOG,log_values},{RESET,reset}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,6,initialize};
