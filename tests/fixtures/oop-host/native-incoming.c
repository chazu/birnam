/* Only primitive Oop messages are used by the bootstrap observer. */
#include "../oop-id-v2/id.h"
#include <string.h>
static IdProtocol id;static Oop names[22],name_behavior,original,portable;
static size_t seen;
enum{AT,PUT,SIZE,BEHAVIOR,BIND,ADD,REMOVE,METHOD,ALIAS,LOAD,RESTORE,SEEN,MUTATE,APPEND,COLLECT,SEQ,INVOKE,INTEGER,STRING,FLOAT,VECTOR,MAP};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t name,const Oop*a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop get(Oop c,size_t i){Oop a=id_size(i);return send(c,AT,&a,1);}
static Oop put(Oop c,size_t i,Oop value){Oop a[]={id_size(i),value};return send(c,PUT,a,2);}
static int ends(Oop name,const char*tail){
    Oop size=send(name,SIZE,0,0);if(id_fault(size))return 0;
    size_t n=id_unbox(size),m=strlen(tail);if(n<m)return 0;
    for(size_t i=0;i<m;i++){Oop index=id_size(n-m+i),v=send(name,AT,&index,1);if(id_fault(v)||id_unbox(v)!=(unsigned char)tail[i])return 0;}
    return 1;
}
static Oop load(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;if(n!=1)return id.invalid;
    int execution=ends(r,"libbirnam_execution.dylib");
    int early_portable=!(seen&2)&&ends(r,"libbirnam_portable_code.dylib");
    if(execution||early_portable){
        Oop ns=get(a[0],0);CHECK(ns);
        for(size_t i=INTEGER;i<=MAP;i++){Oop value=send(ns,AT,&names[i],1);CHECK(value);if(value)return id.invalid;}
        seen|=execution?1:2;
    }
    Oop result=send(r,ALIAS,a,n);CHECK(result);
    // Demonstrate PortableCode's early initializer independently of production
    // load order. The later normal load must reuse that initialized owner.
    if(execution){Oop result=id.message(id.root,portable,id.load,a,n);CHECK(result);}
    return result;
}
static Oop restore(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;
    Oop values[]={id.load,original};CHECK(send(name_behavior,ADD,values,2));
    CHECK(send(name_behavior,REMOVE,&names[ALIAS],1));return id_size(seen);
}
static Oop seen_value(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:id_size(seen);}
/* [captured Word factory, GC, mutable Bytes, calls] */
static Oop mutate(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;CHECK(put(c,3,id_size(id_unbox(get(c,3))+1)));
    Oop z[8];for(size_t i=0;i<8;i++)z[i]=id_size('z');CHECK(send(get(c,2),APPEND,z,8));
    CHECK(send(get(c,1),COLLECT,0,0));
    Oop values=send(id.root,SEQ,a,n);CHECK(values);Oop invoke[]={r,values};return send(get(c,0),INVOKE,invoke,2);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    if(n!=22||count!=2)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];portable=a[1];
    name_behavior=send(id.root,BEHAVIOR,a,1);CHECK(name_behavior);
    Oop query[]={id.load,a[0]};original=send(id.root,BIND,query,2);CHECK(original);if(!original)return id.invalid;
    Oop hook=send(image,METHOD,&names[LOAD],1);CHECK(hook);
    Oop alias[]={names[ALIAS],original};CHECK(send(name_behavior,ADD,alias,2));
    Oop binding[]={id.load,hook};CHECK(send(name_behavior,ADD,binding,2));return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("size"),LIT("behaviorOf:"),LIT("bind:to:"),
    LIT("addMethod"),LIT("removeMethod:"),LIT("methodAt:"),LIT("incomingOriginalLoad:"),LIT("load"),LIT("restore"),LIT("seen"),
    LIT("mutate"),LIT("appendByteValues"),LIT("collectPrecisely"),LIT("sequence"),LIT("invokeOn:arguments:"),
    LIT("Integer"),LIT("String"),LIT("Float"),LIT("Vector"),LIT("Map")};
static const IdMethodSpec methods[]={{LOAD,load},{RESTORE,restore},{SEEN,seen_value},{MUTATE,mutate}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,22,methods,4,initialize};
