/* Source-loader callbacks use owner byte views and ordinary Oop sends. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[10];
enum{AT,PUT,COLLECT,SIZE,INDEX,RECORD,FORWARD,BYTES,EQUAL,SUPPLY};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop pair[]={id_size(i),v};return send(c,PUT,pair,2);}
/* [GC,alias,calls,failure-or-null] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;CHECK(put(c,2,id_size(id_unbox(get(c,2))+1)));CHECK(send(get(c,0),COLLECT,0,0));
 Oop failure=get(c,3);return failure?id_failure(failure):id.message(id.root,r,get(c,1),a,n);
}
/* [GC,bytes alias,trigger,Codegen,nested source,name,once,path,path bytes,equality alias] */
static Oop reenter(Oop c,Oop r,int mutate){
 if(r==get(c,2)&&get(c,6)==id_size(0)){
  CHECK(put(c,6,id_size(1)));
  if(mutate){Oop bytes=get(c,8),length=send(bytes,SIZE,0,0);CHECK(length);
   for(size_t i=0;i<id_unbox(length);i++)CHECK(put(bytes,i,id_size('x')));
  }
  Oop args[]={get(c,4),get(c,5)},recorded=send(get(c,3),RECORD,args,2);CHECK(recorded);
  if(recorded!=args[0])return id.invalid;
 }
 return 0;
}
static Oop bytes(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;CHECK(send(get(c,0),COLLECT,0,0));if(r==get(c,7))return get(c,8);
 CHECK(reenter(c,r,1));return id.message(id.root,r,get(c,1),a,n);
}
static Oop equal(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;CHECK(send(get(c,0),COLLECT,0,0));CHECK(reenter(c,r,0));
 return id.message(id.root,r,get(c,9),a,n);
}
/* [alias,selected source,expected byte provider] */
static Oop supply(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;Oop expected=get(c,2),length=send(a[0],SIZE,0,0);CHECK(length);
 Oop wanted=send(expected,SIZE,0,0);CHECK(wanted);int same=length==wanted;
 for(size_t i=0;same&&i<id_unbox(length);i++){
  Oop k=id_size(i),v=send(a[0],INDEX,&k,1);CHECK(v);Oop w=send(expected,INDEX,&k,1);CHECK(w);same=v==w;
 }
 return same?get(c,1):id.message(id.root,r,get(c,0),a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){
 (void)a;if(n!=10||count)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("sizeWord"),LIT("atWord:"),LIT("recordModuleSource:named:"),LIT("forward"),LIT("bytes"),LIT("equal"),LIT("supply")};
static const IdMethodSpec methods[]={{FORWARD,forward},{BYTES,bytes},{EQUAL,equal},{SUPPLY,supply}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,10,methods,4,initialize};
