/* Check byte arguments through their owner after mutating the original input. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[6];
enum{AT,APPEND,COLLECT,SIZE,BYTE,CALLBACK};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t name,const Oop*a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop get(Oop c,size_t index){Oop i=id_size(index);return send(c,AT,&i,1);}
/* Context: [original Bytes, GC]. */
static Oop callback(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=1)return id.invalid;
    Oop original=get(c,0);CHECK(original);
    const char text[]="changed";Oop values[7];for(size_t i=0;i<7;i++)values[i]=id_size((unsigned char)text[i]);
    CHECK(send(original,APPEND,values,7));CHECK(send(get(c,1),COLLECT,0,0));
    Oop length=send(a[0],SIZE,0,0);CHECK(length);if(length!=id_size(2))return id.invalid;
    for(size_t i=0;i<2;i++){Oop index=id_size(i),value=send(a[0],BYTE,&index,1);CHECK(value);if(value!=id_size(i?'b':'a'))return id.invalid;}
    return original;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=6)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("appendByteValues"),LIT("collectPrecisely"),LIT("sizeWord"),LIT("atWord:"),LIT("callback")};
static const IdMethodSpec methods[]={{CALLBACK,callback}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,6,methods,1,initialize};
