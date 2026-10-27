/* [factory alias, replacement mode, calls, retained observations]. */
#include "../oop-id-v2/id.h"
static IdProtocol id;static Oop names[4];
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop get(Oop c,size_t i){Oop k=id_size(i);return id.message(id.root,c,names[0],&k,1);}
static Oop put(Oop c,size_t i,Oop value){Oop a[]={id_size(i),value};return id.message(id.root,c,names[1],a,2);}
static Oop constructor(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;size_t index=id_unbox(get(c,2));if(index>=16)return id.invalid;
 Oop seen=get(c,3),alias=get(c,0),replacement[]={id_size(10)};int replace=get(c,1)==id_size(1);
 CHECK(put(c,2,id_size(index+1)));Oop answer=id.message(id.root,r,alias,replace?replacement:a,n);CHECK(answer);
 CHECK(put(seen,index,answer));return answer;
}
static Oop getter(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)a;if(n)return id.invalid;size_t index=id_unbox(get(c,2));if(index>=16)return id.invalid;
 CHECK(put(c,2,id_size(index+1)));CHECK(put(get(c,3),index,r));return id_size(99);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=4)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("constructor"),LIT("getter")};
static const IdMethodSpec methods[]={{2,constructor},{3,getter}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,4,methods,2,initialize};
