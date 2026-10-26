#include "id.h"
static IdProtocol id;
static Oop result(Oop root_,Oop c,Oop r,const Oop*a,size_t n){(void)c;(void)r;(void)a;(void)n;return id_size(777);}
static const IdLiteral literals[]={LIT("methodAt:"),LIT("at:put:"),LIT("fault:detail:"),LIT("partial-initialization"),LIT("published")};
static const IdMethodSpec methods[]={{4,result}};
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t ns,const Oop*a,size_t n){
 id=*p;if(ns!=5||n!=1)return id.invalid;
 Oop key=s[4];Oop m=id.message(id.root,image,s[0],&key,1);if(id_fault(m))return m;
 Oop put[]={id_size(0),m};Oop r=id.message(id.root,a[0],s[1],put,2);if(id_fault(r))return r;
 Oop details[]={s[3],a[0]};return id.message(id.root,id.root,s[2],details,2);
}
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,5,methods,1,initialize};
