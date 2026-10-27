/* Store callbacks erase original roots, collect, and use public owner messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[9];static size_t watched[32],watch_count,mask,seen[64][3],seen_count;
enum{AT,PUT,COLLECT,BEGIN,IDENTITY,IMAGE_ADD,ASSERT,RAW};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
/* [GC,alias,calls,inside] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(get(c,3)==id_size(0)){CHECK(put(c,3,id_size(1)));CHECK(increment(c,2));CHECK(send(get(c,0),COLLECT,0,0));CHECK(put(c,3,id_size(0)));}return id.message(id.root,r,get(c,1),a,n);
}
/* [GC,Store,calls,fault-or-null]. Observations contain identities, not roots. */
static Oop snapshot(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=3||seen_count==64)return id.invalid;if(get(c,2)==id_size(0)){seen_count=0;CHECK(send(get(c,1),BEGIN,0,0));}CHECK(send(get(c,0),COLLECT,0,0));
 for(size_t i=0;i<3;i++){Oop identity=send(id.root,IDENTITY,a+i,1);CHECK(identity);seen[seen_count][i]=id_unbox(identity);}seen_count++;CHECK(increment(c,2));Oop fault=get(c,3);return fault?id_failure(fault):get(c,1);
}
/* [GC,target,calls] */
static Oop image_copy(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=6)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));CHECK(increment(c,2));return send(get(c,1),IMAGE_ADD,a,n);}
/* [GC,Store,decision,authorizations,notifications,expected Method or null] */
static Oop authorize(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));CHECK(increment(c,3));return get(c,2);}
static Oop observe(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=2)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));Oop args[]={a[1],a[0]},value=send(get(c,1),RAW,args,2);CHECK(value);if(value!=get(c,5))return id.invalid;CHECK(increment(c,4));return 0;}
/* [Store,entity,attribute,value,rejected] */
static Oop phase(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n!=2)return id.invalid;Oop args[]={get(c,1),get(c,2),get(c,3)},result=send(get(c,0),ASSERT,args,3);CHECK(put(c,4,id_size(id_fault(result))));return 0;}
static Oop watch(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n>32)return id.invalid;watch_count=n;for(size_t i=0;i<n;i++){Oop identity=send(id.root,IDENTITY,a+i,1);CHECK(identity);watched[i]=id_unbox(identity);}return 0;}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;for(size_t i=0;i<watch_count;i++)if(watched[i]==id_unbox(a[2]))mask|=(size_t)1<<i;return 0;}
static Oop mask_value(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;mask=0;Oop result=id.message(id.root,id.root,names[8],&c,1);CHECK(result);return id_size(mask);}
static Oop seen_row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;for(size_t i=0;i<seen_count;i++)if(seen[i][0]==id_unbox(a[0])&&seen[i][1]==id_unbox(a[1])&&seen[i][2]==id_unbox(a[2]))return id_size(1);return id_size(0);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=19)return p->invalid;id=*p;for(size_t i=0;i<9;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("imageLoadBegin"),LIT("identityOf:"),LIT("imageAddFact:attr:value:tx:op:historied:"),LIT("assert:attr:value:"),LIT("rawCurrentValue:attr:"),LIT("allocationsDo:"),LIT("forward"),LIT("snapshot"),LIT("imageCopy"),LIT("authorize"),LIT("observe"),LIT("phase"),LIT("watch"),LIT("allocation:bytes:identity:"),LIT("mask"),LIT("seen")};
static const IdMethodSpec methods[]={{9,forward},{10,snapshot},{11,image_copy},{12,authorize},{13,observe},{14,phase},{15,watch},{16,allocation},{17,mask_value},{18,seen_row}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,19,methods,10,initialize};
