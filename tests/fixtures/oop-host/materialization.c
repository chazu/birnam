/* Replaceable materialization dependencies, including collection and native failure. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[7];
/* [GC,answer,calls,failure-or-null] */
static Oop result(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;(void)a;(void)n;
    Oop index=id_size(2),count=id.message(id.root,c,names[0],&index,1);
    if(id_fault(count))return count;
    Oop update[]={index,id_size(id_unbox(count)+1)};
    Oop changed=id.message(id.root,c,names[1],update,2);if(id_fault(changed))return changed;
    index=id_size(0);Oop gc=id.message(id.root,c,names[0],&index,1);if(id_fault(gc))return gc;
    Oop collected=id.message(id.root,gc,names[2],0,0);if(id_fault(collected))return collected;
    index=id_size(3);Oop failure=id.message(id.root,c,names[0],&index,1);if(id_fault(failure))return failure;
    if(failure)return id_failure(failure);
    index=id_size(1);return id.message(id.root,c,names[0],&index,1);
}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return id.message(id.root,c,names[0],&k,1);}
/* [GC,alias,calls,failure-or-null] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;Oop count=get(c,2);if(id_fault(count))return count;
    Oop update[]={id_size(2),id_size(id_unbox(count)+1)};
    Oop v=id.message(id.root,c,names[1],update,2);if(id_fault(v))return v;
    Oop failure=get(c,3);if(failure)return id_failure(failure);
    v=id.message(id.root,get(c,0),names[2],0,0);if(id_fault(v))return v;
    return id.message(id.root,r,get(c,1),a,n);
}
/* [capture Vector,before,after,compiled,at alias,compile alias] */
static Oop timed_at(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;if(r==get(c,0))return get(c,get(c,3)==id_size(1)?2:1);
    return id.message(id.root,r,get(c,4),a,n);
}
static Oop timed_compile(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;Oop update[]={id_size(3),id_size(1)};
    Oop v=id.message(id.root,c,names[1],update,2);if(id_fault(v))return v;
    return id.message(id.root,r,get(c,5),a,n);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){
    (void)a;if(n!=7||count)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=s[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("result"),LIT("forward"),LIT("timedAt"),LIT("timedCompile")};
static const IdMethodSpec methods[]={{3,result},{4,forward},{5,timed_at},{6,timed_compile}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,7,methods,4,initialize};
