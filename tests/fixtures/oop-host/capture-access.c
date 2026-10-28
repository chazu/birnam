/* Test callbacks use only Id messages; identities are weak observations. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, COLLECT, BITS, IDENTITY, ALLOCATIONS, ADD, VALUE,
       FRESH, FACTORY, NESTED, ROW, PRESENT, RETAIN, RELEASE, COUNT };
static Oop names[COUNT];
static size_t wanted, seen;
#define CHECK(v) do { Oop checked=(v);if(id_fault(checked))return checked; }while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=3)return id.invalid;
 if(wanted==id_unbox(a[2]))seen++;return 0;
}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);seen=0;
 CHECK(send(id.root,ALLOCATIONS,&r,1));return id_size(seen);
}
/* [GC, Word, calls, first identity, optional next reader, vtable, selector, observer, Roots]. */
static Oop fresh(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1||!((uintptr_t)a[0]&1))return id.invalid;
 size_t index=id_unbox(a[0]);if(index>1)return id.invalid;
 CHECK(put(c,2,id_size(id_unbox(get(c,2))+1)));
 if(index==1){
  CHECK(send(get(c,0),COLLECT,0,0));
  Oop identity=get(c,3), found=send(get(c,7),PRESENT,&identity,1);CHECK(found);
  if(id_unbox(found)!=1)return id.invalid;
 }
 if(index==0&&get(c,4)){
  Oop args[]={get(c,6),get(c,4)};CHECK(send(get(c,5),ADD,args,2));
 }
 Oop bits=id_size(101+index), result=send(get(c,1),BITS,&bits,1);CHECK(result);
 Oop token=send(get(c,8),RETAIN,&result,1);CHECK(token);
 if(index==0){Oop identity=send(id.root,IDENTITY,&result,1);CHECK(identity);CHECK(put(c,3,identity));}
 Oop release[]={result,token};CHECK(send(get(c,8),RELEASE,release,2));return result;
}
static Oop factory(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 if(n!=2)return id.invalid;return fresh(root,c,r,a,1);
}
/* [GC, other Block, expected later-capture identity, calls, observer]. */
static Oop nested(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)a;if(n)return id.invalid;
 CHECK(send(get(c,1),VALUE,0,0));CHECK(send(get(c,0),COLLECT,0,0));
 Oop identity=get(c,2), found=send(get(c,4),PRESENT,&identity,1);CHECK(found);
 if(id_unbox(found)!=1)return id.invalid;
 CHECK(put(c,3,id_size(id_unbox(get(c,3))+1)));return r;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 (void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("fromBits:"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("addMethod"),LIT("value"),LIT("fresh:"),LIT("factory:of:"),LIT("nested"),LIT("allocation:bytes:identity:"),LIT("present:"),LIT("retain:"),LIT("releaseTokensProtecting")};
static const IdMethodSpec methods[]={{FRESH,fresh},{FACTORY,factory},{NESTED,nested},{ROW,row},{PRESENT,present}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,5,initialize};
