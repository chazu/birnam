/* Observe construction through Name loading; never inspect a foreign payload. */
#include "../oop-id-v2/id.h"
#include <string.h>
static IdProtocol id;static Oop names[17],name_behavior,original,receiver_method,gc;
static size_t observed;
enum{AT,SIZE,BEHAVIOR,BIND,ADD,REMOVE,METHOD,ALIAS,LOAD,RESTORE,RECEIVER,GC,BUDGET,PLUS,TO,SPLIT,PRECISE};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t name,const Oop*a,size_t n){return id.message(id.root,r,names[name],a,n);}
static int ends(Oop name,const char*tail){
    Oop size=send(name,SIZE,0,0);if(id_fault(size))return 0;
    size_t n=id_unbox(size),m=strlen(tail);if(n<m)return 0;
    for(size_t i=0;i<m;i++){Oop index=id_size(n-m+i),v=send(name,AT,&index,1);if(id_fault(v)||id_unbox(v)!=(unsigned char)tail[i])return 0;}
    return 1;
}
static Oop receiver(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)a;(void)n;return r;}
static Oop replace(Oop value,Oop selector){
    Oop behavior=send(id.root,BEHAVIOR,&value,1);CHECK(behavior);Oop args[]={selector,receiver_method};return send(behavior,ADD,args,2);
}
static Oop load(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;if(n!=1)return id.invalid;
    if(ends(r,"libbirnam_object_model.dylib")){
        Oop index=id_size(0),ns=send(a[0],AT,&index,1);CHECK(ns);gc=send(ns,AT,&names[GC],1);CHECK(gc);
        Oop one=id_size(1);CHECK(send(gc,PRECISE,&one,1));CHECK(send(gc,BUDGET,&one,1));observed|=1;
    }
    Oop result=send(r,ALIAS,a,n);CHECK(result);
    if(ends(r,"libbirnam_integer.dylib")){
        CHECK(replace(result,names[PLUS]));CHECK(replace(result,names[TO]));observed|=6;
    }
    if(ends(r,"libbirnam_string.dylib")){
        CHECK(replace(result,names[SIZE]));CHECK(replace(result,names[SPLIT]));observed|=24;
    }
    if(ends(r,"libbirnam_portable_code.dylib")){
        // Native values and their numeric/collection extensions are complete.
        // Authored startup has separate collection coverage.
        Oop budget=id_size(1024*1024);CHECK(send(gc,BUDGET,&budget,1));observed|=32;
    }
    return result;
}
static Oop restore(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;
    Oop args[]={id.load,original};CHECK(send(name_behavior,ADD,args,2));CHECK(send(name_behavior,REMOVE,&names[ALIAS],1));return id_size(observed);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*symbols,size_t n,const Oop*a,size_t count){
    if(n!=17||count!=1)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=symbols[i];
    name_behavior=send(id.root,BEHAVIOR,a,1);CHECK(name_behavior);Oop query[]={id.load,a[0]};original=send(id.root,BIND,query,2);CHECK(original);if(!original)return id.invalid;
    receiver_method=send(image,METHOD,&names[RECEIVER],1);CHECK(receiver_method);
    Oop hook=send(image,METHOD,&names[LOAD],1);CHECK(hook);
    Oop alias[]={names[ALIAS],original};CHECK(send(name_behavior,ADD,alias,2));Oop binding[]={id.load,hook};CHECK(send(name_behavior,ADD,binding,2));return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("size"),LIT("behaviorOf:"),LIT("bind:to:"),LIT("addMethod"),LIT("removeMethod:"),
    LIT("methodAt:"),LIT("numbersOriginalLoad:"),LIT("load"),LIT("restore"),LIT("receiver"),LIT("GC"),LIT("budget:"),LIT("+"),LIT("to:"),
    LIT("split:"),LIT("precisePayloads:")};
static const IdMethodSpec methods[]={{LOAD,load},{RESTORE,restore},{RECEIVER,receiver}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,17,methods,3,initialize};
