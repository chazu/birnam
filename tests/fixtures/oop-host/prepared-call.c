/* Select the authored operation for one Machine; helpers use the saved Method. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[8];
enum{AT,PUT,SEQ,INVOKE,FIELD,WORD,REPLACE,CAPTURE};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop get(Oop c,size_t i){Oop k=id_size(i);return id.message(id.root,c,names[AT],&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return id.message(id.root,c,names[PUT],a,2);}
/* [target,authored,original,calls,active,helper calls,Integer,max copies] */
static Oop replace(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=2)return id.invalid;int target=r==get(c,0)&&get(c,4)==id_size(0);size_t counter=target?3:5;
 CHECK(put(c,counter,id_size(id_unbox(get(c,counter))+1)));if(target)CHECK(put(c,4,id_size(1)));
 if(target){Oop query[]={a[0],id_size(5)},copies=id.message(id.root,r,names[FIELD],query,2);CHECK(copies);
  Oop bits=id.message(id.root,get(c,6),names[WORD],&copies,1);CHECK(bits);if(id_unbox(bits)>id_unbox(get(c,7)))CHECK(put(c,7,bits));}
 Oop values=id.message(id.root,id.root,names[SEQ],a,n);CHECK(values);Oop args[]={r,values};
 Oop result=id.message(id.root,get(c,target?1:2),names[INVOKE],args,2);
 if(target){Oop cleared=put(c,4,id_size(0));CHECK(cleared);}return result;
}
/* [original registration Method,captured Oop arguments]. */
static Oop capture(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=5)return id.invalid;Oop values=id.message(id.root,id.root,names[SEQ],a,n);CHECK(values);CHECK(put(c,1,values));
 Oop args[]={r,values};return id.message(id.root,get(c,0),names[INVOKE],args,2);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 (void)a;if(count!=8||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("sequence"),LIT("invokeOn:arguments:"),LIT("callPlan:field:"),LIT("wordOf:"),LIT("replace"),LIT("capture")};
static const IdMethodSpec methods[]={{REPLACE,replace},{CAPTURE,capture}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,8,methods,2,initialize};
