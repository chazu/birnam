/* Current method binding/capture/primitive replacements, expressed only through Id. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum {AT,PUT,GC,ADD,VALUE,CAPTURE_ALIAS,RECEIVER,ARGUMENT,CAPTURE,COUNT};
static Oop names[COUNT];
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c){return put(c,0,id_size(id_unbox(get(c,0))+1));}
static Oop receiver(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)a;if(n!=1)return id.invalid;CHECK(increment(c));return r;}
static Oop argument(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=2&&n!=3)return id.invalid;CHECK(increment(c));return a[n-2];}
/* [calls, GC, target behavior, replacement Method]. Original capture is owned by
   the currently entered Block, so it remains live across collection. */
static Oop capture(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop result=send(r,CAPTURE_ALIAS,a,n);CHECK(result);CHECK(increment(c));
 CHECK(send(get(c,1),GC,0,0));Oop change[]={names[VALUE],get(c,3)};CHECK(send(get(c,2),ADD,change,2));return result;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=COUNT||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("addMethod"),LIT("compute:with:"),LIT("helperOriginalCapture:"),LIT("receiver"),LIT("argument"),LIT("capture")};
static const IdMethodSpec methods[]={{RECEIVER,receiver},{ARGUMENT,argument},{CAPTURE,capture}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,3,initialize};
