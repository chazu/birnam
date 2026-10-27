/* Real TCC candidate: recover, remove its binding, collect, then keep executing. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop ns,baseline,names[23];
enum{AT,PUT,ROOTS,VECTOR,NEW,ADD,NIL,INTEGER,FROM,RETAIN,RELEASE,RECOVER,BEHAVIOR,REPLACE,VALUE,GC,COLLECT,TCC,CHECKPOINT};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop named(size_t s){return send(ns,AT,names+s,1);}
static Oop recover(Oop root,Oop c,Oop machine,const Oop*a,size_t n){
 (void)root;(void)c;if(n!=1)return id.invalid;
 Oop roots=named(ROOTS),pins[4],v=send(named(VECTOR),NEW,0,0);CHECK(v);pins[0]=send(roots,RETAIN,&v,1);CHECK(pins[0]);
 v=send(v,ADD,a,1);CHECK(v);pins[1]=send(roots,RETAIN,&v,1);CHECK(pins[1]);
 Oop nil=named(NIL);v=send(v,ADD,&nil,1);CHECK(v);pins[2]=send(roots,RETAIN,&v,1);CHECK(pins[2]);
 Oop z=id_size(0),zero=send(named(INTEGER),FROM,&z,1);CHECK(zero);pins[3]=send(roots,RETAIN,&zero,1);CHECK(pins[3]);
 Oop recipe[]={zero,v};CHECK(send(machine,RECOVER,recipe,2));
 Oop behavior=send(id.root,BEHAVIOR,a,1);CHECK(behavior);Oop replacement[]={baseline,names[VALUE]};CHECK(send(behavior,REPLACE,replacement,2));
 CHECK(send(named(GC),COLLECT,0,0));CHECK(send(named(TCC),CHECKPOINT,0,0));
 Oop marker[]={names[21],named(22)};CHECK(send(ns,PUT,marker,2));
 for(size_t i=4;i>0;i--)CHECK(send(roots,RELEASE,pins+i-1,1));return 0;
}
static Oop initialize(const IdProtocol*p,Oop unit,const Oop*s,size_t count,const Oop*a,size_t n){
 if(count!=23||n!=2)return p->invalid;id=*p;ns=a[0];baseline=a[1];for(size_t i=0;i<23;i++)names[i]=s[i];
 CHECK(send(unit,RETAIN,&baseline,1));Oop z=id_size(0),method=id.message(id.root,unit,s[19],&z,1);CHECK(method);
 Oop zero=send(named(INTEGER),FROM,&z,1);CHECK(zero);Oop args[]={method,zero,unit};return id.message(id.root,baseline,s[20],args,3);
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("Roots"),LIT("Vector"),LIT("new"),LIT("add:"),LIT("nil"),LIT("Integer"),LIT("fromWord:"),LIT("retain:"),LIT("release:"),LIT("recoverAt:with:"),LIT("behaviorOf:"),LIT("addMethod:as:"),LIT("value"),LIT("GC"),LIT("collectPrecisely"),LIT("TCC"),LIT("checkpoint"),LIT("declaredMethodAt:"),LIT("withInline:at:owner:"),LIT("Recovered"),LIT("true")};
static const IdMethodSpec methods[]={{11,recover}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,23,methods,1,initialize};
