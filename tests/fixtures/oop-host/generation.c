/* Generation probes enter saved Methods through explicit selector aliases. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum{AT,PUT,GC,RECORD,CONSTRUCT,FAULT,REJECT,EDIT,REMOVE,ADD,SAVE,RETAIN,RELEASE,REMOVE_STORE,COUNT};static Oop names[COUNT];
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
/* [GC,alias,Codegen,added source,name,mutate,calls]. */
static Oop constructing(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;CHECK(increment(c,6));
 if(get(c,5)==id_size(1)){CHECK(put(c,5,id_size(0)));Oop args[]={get(c,3),get(c,4)};CHECK(send(get(c,2),RECORD,args,2));}
 CHECK(send(get(c,0),GC,0,0));return id.message(id.root,r,get(c,1),a,n);
}
/* [GC,alias,key-or-absence,Error,armed,calls,after,extra,extra selector]. */
static Oop fault(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop key=get(c,2);
 if((key&&(!n||a[n-1]!=key))||get(c,4)!=id_size(1))return id.message(id.root,r,get(c,1),a,n);
 CHECK(put(c,4,id_size(0)));CHECK(increment(c,5));
 if(get(c,6)==id_size(1)){
  Oop result=id.message(id.root,r,get(c,1),a,n);CHECK(result);
  if(get(c,7)==id_size(1)){if(n!=2)return id.invalid;Oop extra[]={a[0],get(c,8)};CHECK(id.message(id.root,r,get(c,1),extra,2));}
 }
 CHECK(send(get(c,0),GC,0,0));return get(c,3);
}
/* [fault,calls]. */
static Oop reject(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;CHECK(increment(c,1));return id_failure(get(c,0));}
/* [setter alias,armed,denied]. A reentrant remove/reinstall is an ordinary edit. */
static Oop edit(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=2)return id.invalid;Oop answer=id.message(id.root,r,get(c,0),a,n);CHECK(answer);
 if(get(c,1)==id_size(1)){
  CHECK(put(c,1,id_size(0)));CHECK(send(r,REMOVE,a+1,1));Oop restored=send(r,ADD,a,n);CHECK(restored);
  if(restored!=a[0])CHECK(increment(c,2));
 }
 return answer;
}
/* [compiler alias,Roots,Store,selector,vtable,calls]. */
static Oop during_save(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop roots=get(c,1),answer=id.message(id.root,r,get(c,0),a,n);CHECK(answer);
 Oop pin=send(roots,RETAIN,&answer,1);CHECK(pin);Oop result=increment(c,5);
 if(!id_fault(result)){Oop args[]={get(c,3),get(c,4)};result=send(get(c,2),REMOVE_STORE,args,2);}
 Oop release[]={answer,pin},released=send(roots,RELEASE,release,2);CHECK(result);CHECK(released);return answer;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("recordModuleSource:named:"),LIT("constructing"),LIT("fault"),LIT("reject"),LIT("edit"),LIT("removeMethod:"),LIT("addMethod:as:"),LIT("duringSave"),LIT("retain:"),LIT("releaseTokensProtecting"),LIT("removeMethod:on:")};
static const IdMethodSpec methods[]={{CONSTRUCT,constructing},{FAULT,fault},{REJECT,reject},{EDIT,edit},{SAVE,during_save}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,5,initialize};
