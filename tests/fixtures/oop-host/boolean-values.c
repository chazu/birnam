/* Boolean replacements return ordinary Oops and preserve native failures. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[7];
enum{AT,PUT,COLLECT,VALUE,ALTERNATIVE,SELECT,EXTENT};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t name,const Oop*a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop get(Oop c,size_t i){Oop a=id_size(i);return send(c,AT,&a,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop increment(Oop c,size_t i){return put(c,i,id_size(id_unbox(get(c,i))+1));}
/* [GC, answer, calls, fault-or-null] */
static Oop value(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;if(n)return id.invalid;CHECK(increment(c,2));
    Oop fault=get(c,3);if(fault)return id_failure(fault);CHECK(send(get(c,0),COLLECT,0,0));return get(c,1);
}
static Oop alternative(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=3)return id.invalid;CHECK(send(get(c,0),COLLECT,0,0));
    for(size_t i=0;i<n;i++){CHECK(send(id.root,EXTENT,a+i,1));}
    Oop fault=get(c,3);return fault?id_failure(fault):a[1];
}
/* [GC, original selector alias, calls, mode, fault] */
static Oop select(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;if(n!=4)return id.invalid;CHECK(increment(c,2));CHECK(send(get(c,0),COLLECT,0,0));
    for(size_t i=0;i<n;i++){CHECK(send(id.root,EXTENT,a+i,1));}
    size_t mode=id_unbox(get(c,3));if(mode==1)return a[3];if(mode==2)return id_failure(get(c,4));
    return id.message(id.root,r,get(c,1),a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=7)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("value"),LIT("alternative"),LIT("select"),LIT("allocationSizeOf:")};
static const IdMethodSpec methods[]={{VALUE,value},{ALTERNATIVE,alternative},{SELECT,select}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,7,methods,3,initialize};
