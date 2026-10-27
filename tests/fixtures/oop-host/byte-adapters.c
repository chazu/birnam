/* Byte callbacks mutate the original owner after the adapter snapshots it. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum {AT,PUT,GC,RELEASE,IDENTITY,ALLOCS,APPEND,FROM_BYTES,SIZE,BYTE_AT,CONSUME,DISCARD,RECORD,ROW,COUNT};static Oop names[COUNT];static size_t wanted;static Oop found;
/* [GC, Roots, Bytes, monitor, input identity, external token, calls, mode,
    recorded word, mutation suffix, target failure, factory failure, nil, ignored result]. */
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)found=a[0];return 0;}
static Oop find_input(Oop c){wanted=id_unbox(get(c,4));found=0;Oop monitor=get(c,3);CHECK(send(id.root,ALLOCS,&monitor,1));return found?found:id.invalid;}
static Oop callback_input(Oop c){
 Oop input=find_input(c);CHECK(input);Oop token=get(c,5);CHECK(put(c,5,0));if(token)CHECK(send(get(c,1),RELEASE,&token,1));
 Oop suffix=get(c,9);CHECK(send(input,APPEND,&suffix,1));CHECK(send(get(c,0),GC,0,0));return find_input(c);
}
static Oop check_copy(Oop value,unsigned char first,unsigned char second){
 Oop length=send(value,SIZE,0,0);CHECK(length);if(length!=id_size(2))return id.invalid;
 for(size_t i=0;i<2;i++){Oop k=id_size(i),v=send(value,BYTE_AT,&k,1);CHECK(v);if(v!=id_size(i?second:first))return id.invalid;}return 0;
}
static Oop consume(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;CHECK(put(c,6,id_size(id_unbox(get(c,6))+1)));Oop input=callback_input(c);CHECK(input);CHECK(check_copy(a[0],'a','b'));
 switch(id_unbox(get(c,7))){case 1:return 0;case 2:return get(c,12);case 3:return id_failure(get(c,10));case 4:return id_size(9);default:return input;}
}
static Oop factory(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;CHECK(put(c,6,id_size(id_unbox(get(c,6))+1)));CHECK(callback_input(c));CHECK(check_copy(a[0],'x','y'));
 if(get(c,7)==id_size(3))return id_failure(get(c,11));return send(get(c,2),FROM_BYTES,a,n);
}
static Oop discard(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1||a[0]!=id_size(5))return id.invalid;return get(c,13);}
static Oop record(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;CHECK(put(c,8,a[0]));CHECK(send(get(c,0),GC,0,0));return 0;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("release:"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("appendBytes:"),LIT("fromBytes:"),LIT("sizeWord"),LIT("atWord:"),LIT("consume:"),LIT("discard:"),LIT("record:"),LIT("allocation:bytes:identity:")};
static const IdMethodSpec methods[]={{CONSUME,consume},{FROM_BYTES,factory},{DISCARD,discard},{RECORD,record},{ROW,row}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,5,initialize};
