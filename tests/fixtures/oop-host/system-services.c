/* Collecting service dependencies and mutable byte views through owner messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[8];
enum {AT,PUT,COLLECT,ERROR,SIZE,INDEX,FORWARD,CHANGING};
#define CHECK(v) do { Oop checked=(v);if(id_fault(checked))return checked; }while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
/* [GC,original selector alias,calls,failure-or-null,Namespace,published Error-or-null] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;
    CHECK(send(get(c,0),COLLECT,0,0));
    CHECK(put(c,2,id_size(id_unbox(get(c,2))+1)));
    Oop failure=get(c,3);if(failure)return id_failure(failure);
    Oop error=get(c,5);
    if(error){Oop binding[]={names[ERROR],error};CHECK(send(get(c,4),PUT,binding,2));}
    return id.message(id.root,r,get(c,1),a,n);
}
/* [GC,original bytes alias,path,contents,mutable path Sequence,replacement,
    observed receivers,calls,failure-or-null] */
static Oop changing(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;if(n)return id.invalid;
    CHECK(send(get(c,0),COLLECT,0,0));
    size_t calls=id_unbox(get(c,7));
    CHECK(put(get(c,6),calls,r));CHECK(put(c,7,id_size(calls+1)));
    if(r==get(c,2))return get(c,4);
    if(r==get(c,3)){
        Oop from=get(c,5),to=get(c,4);
        Oop length=send(from,SIZE,0,0);CHECK(length);
        if(send(to,SIZE,0,0)!=length)return id.invalid;
        for(size_t i=0;i<id_unbox(length);i++){
            Oop index=id_size(i),byte=send(from,INDEX,&index,1);CHECK(byte);
            CHECK(put(to,i,byte));
        }
        Oop failure=get(c,8);if(failure)return id_failure(failure);
    }
    return id.message(id.root,r,get(c,1),a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    (void)a;if(count||n!=8)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("Error"),
    LIT("sizeWord"),LIT("atWord:"),LIT("forward"),LIT("changing")};
static const IdMethodSpec methods[]={{FORWARD,forward},{CHANGING,changing}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,8,methods,2,initialize};
