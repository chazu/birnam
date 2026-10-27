/* Declared Oop callbacks for generated dispatch and collecting owner reads. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[7];
enum{AT,PUT,COLLECT,SEQ,INVOKE,FORWARD,FIRST};
static Oop get(Oop c,size_t i){Oop k=id_size(i);return id.message(id.root,c,names[AT],&k,1);}
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
/* [original Method,GC,calls,collect flag]. */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop pair[]={id_size(2),id_size(id_unbox(get(c,2))+1)};CHECK(id.message(id.root,c,names[PUT],pair,2));
 if(get(c,3)==id_size(1))CHECK(id.message(id.root,get(c,1),names[COLLECT],0,0));
 Oop values=id.message(id.root,id.root,names[SEQ],a,n);CHECK(values);Oop args[]={r,values};return id.message(id.root,get(c,0),names[INVOKE],args,2);
}
static Oop first(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;return n==1?a[0]:id.invalid;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 (void)a;if(count!=7||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("sequence"),LIT("invokeOn:arguments:"),LIT("forward"),LIT("first")};
static const IdMethodSpec methods[]={{FORWARD,forward},{FIRST,first}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,7,methods,2,initialize};
