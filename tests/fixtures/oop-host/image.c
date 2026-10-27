/* Image boundary callbacks use only Oop contexts and current owner messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum{AT,PUT,GC,RETAIN,RELEASE,ADD,BEHAVIOR,ASSERT,RECORD,RETENTION,PROVIDER,SLOT,IDENTITY,ALLOCS,PRESENT,CHANGE,NOTE,SIZE,PROJECTION,CENSUS,FIXED,CONFLICT,COUNT};static Oop names[COUNT];
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
/* [alias,Roots,GC,mode,calls,IntegerVt,selector,replacement,Store,one,
    Module,imageProvider,store,false]. */
static Oop change_body(Oop c){
 CHECK(increment(c,4));Oop result=0;
 if(get(c,3)==id_size(2)){Oop a[]={get(c,11),get(c,12),get(c,13)};result=send(get(c,10),PROVIDER,a,3);}
 else if(get(c,3)==id_size(1)){Oop one=get(c,9);result=send(get(c,8),RETENTION,&one,1);}
 else{Oop a[]={get(c,6),get(c,7)};result=send(get(c,5),ADD,a,2);}
 CHECK(result);return send(get(c,2),GC,0,0);
}
static Oop change(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;Oop roots=get(c,1),answer=id.message(id.root,r,get(c,0),a,n);CHECK(answer);
 Oop pin=send(roots,RETAIN,&answer,1);CHECK(pin);Oop result=change_body(c);
 Oop args[]={answer,pin},released=send(roots,RELEASE,args,2);CHECK(result);CHECK(released);return answer;
}
static size_t projection_id,child_id,wanted,found;
/* [original size alias,GC,ObjectModel,monitor,collections]. */
static Oop note(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;
 Oop identity=send(id.root,IDENTITY,a,1);CHECK(identity);projection_id=id_unbox(identity);
 Oop query[]={id_size(0),a[0]},child=send(get(c,2),SLOT,query,2);CHECK(child);
 identity=send(id.root,IDENTITY,&child,1);CHECK(identity);child_id=id_unbox(identity);return a[0];
}
static Oop size_projection(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;Oop identity=send(id.root,IDENTITY,a,1);CHECK(identity);
 if(id_unbox(identity)==projection_id){
  CHECK(send(get(c,1),GC,0,0));Oop p=id_size(projection_id),child=id_size(child_id);
  if(send(get(c,3),PRESENT,&p,1)!=id_size(1)||send(get(c,3),PRESENT,&child,1)!=id_size(1))return id.invalid;
  CHECK(increment(c,4));
 }
 return id.message(id.root,r,get(c,0),a,n);
}
static Oop projection(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;return a[0]==id_size(0)?id_size(projection_id):a[0]==id_size(1)?id_size(child_id):id.invalid;}
static Oop census(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found++;return 0;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);found=0;CHECK(send(id.root,ALLOCS,&r,1));return id_size(found);}
static Oop fixed(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;return c;}
/* [alias,selector,marker,marker Method,Globals,Store,side key,GC,source-or-null,
    source name,Codegen,published target,calls,error-or-null,native fault]. */
static Oop conflict(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=4)return id.invalid;Oop result=id.message(id.root,r,get(c,0),a,n);CHECK(result);
 if(a[2]!=get(c,1))return result;
 CHECK(increment(c,12));CHECK(put(c,11,a[1]));Oop behavior=send(id.root,BEHAVIOR,a+1,1);CHECK(behavior);
 Oop binding[]={a[2],get(c,3)};CHECK(send(behavior,ADD,binding,2));
 Oop global[]={get(c,6),get(c,2)};CHECK(send(get(c,4),PUT,global,2));
 Oop fact[]={get(c,6),get(c,6),get(c,2)};CHECK(send(get(c,5),ASSERT,fact,3));
 Oop source=get(c,8);if(source){Oop record[]={source,get(c,9)};CHECK(send(get(c,10),RECORD,record,2));}
 CHECK(send(get(c,7),GC,0,0));Oop error=get(c,13);return error?error:id_failure(get(c,14));
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("retain:"),LIT("releaseTokensProtecting"),LIT("addMethod"),LIT("behaviorOf:"),LIT("assert:attr:value:"),LIT("recordModuleSource:named:"),LIT("retainLast:"),LIT("field:of:put:"),LIT("imageSlotAt:in:"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("present:"),LIT("change"),LIT("note"),LIT("size"),LIT("projection:"),LIT("census"),LIT("fixed"),LIT("conflict")};
static const IdMethodSpec methods[]={{CHANGE,change},{NOTE,note},{SIZE,size_projection},{PROJECTION,projection},{CENSUS,census},{PRESENT,present},{FIXED,fixed},{CONFLICT,conflict}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,8,initialize};
