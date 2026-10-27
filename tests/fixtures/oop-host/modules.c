/* Native initializer/reentry and root/binding snapshots through ordinary Oops. */
#include "../oop-id-v2/id.h"
#include <stdlib.h>
static IdProtocol id;static Oop names[5];
enum{AT,PUT,RETAIN,LOAD,VISIT};
static Oop *roots;static size_t root_count,root_capacity;typedef struct{Oop key,value;}Binding;static Binding*bindings;static size_t binding_count,binding_capacity;
static Oop extra,extra_token,nested;static int retain_extra,retained,depth;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop get(Oop c,size_t i){Oop k=id_size(i);return id.message(id.root,c,names[AT],&k,1);}
/* [alias,calls] */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;Oop pair[]={id_size(1),id_size(id_unbox(get(c,1))+1)};CHECK(id.message(id.root,c,names[PUT],pair,2));return id.message(id.root,r,get(c,0),a,n);}
/* [Roots,Bootstrap,nested path]. Snapshots are observations, not GC roots. */
static Oop visit(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;if(root_count==root_capacity){size_t capacity=root_capacity?root_capacity*2:256;Oop*p=realloc(roots,capacity*sizeof(Oop));if(!p)return id.invalid;roots=p;root_capacity=capacity;}roots[root_count++]=a[0];
 if(retain_extra&&!retained){retained=1;extra_token=id.message(id.root,get(c,0),names[RETAIN],&extra,1);CHECK(extra_token);}return 0;
}
static Oop binding(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=2)return id.invalid;if(binding_count==binding_capacity){size_t capacity=binding_capacity?binding_capacity*2:256;Binding*p=realloc(bindings,capacity*sizeof(Binding));if(!p)return id.invalid;bindings=p;binding_capacity=capacity;}bindings[binding_count++]=(Binding){a[0],a[1]};return 0;
}
static Oop configure_visit(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=2)return id.invalid;extra=a[0];retain_extra=a[1]==id_size(1);retained=0;extra_token=0;root_count=0;return 0;}
static Oop reset(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;root_count=binding_count=0;return 0;}
static Oop contains(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=2)return id.invalid;for(size_t i=0;i<binding_count;i++)if(bindings[i].key==a[0]&&bindings[i].value==a[1])return id_size(1);return id_size(0);}
static Oop key_count(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;size_t count=0;for(size_t i=0;i<binding_count;i++)if(bindings[i].key==a[0])count++;return id_size(count);}
static Oop root_matches(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;size_t count=0;for(size_t i=0;i<root_count;i++)if(roots[i]==a[0])count++;return id_size(count);}
static Oop retained_token(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n?id.invalid:extra_token;}
static Oop configure_load(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;nested=a[0];depth=0;return 0;}
static Oop reentrant_load(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;if(!depth){depth=1;Oop args[]={get(c,2),nested};Oop result=id.message(id.root,get(c,1),names[LOAD],args,2);CHECK(result);if(result!=nested)return id.invalid;depth=0;}return a[0];
}
static Oop dispose(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;free(roots);free(bindings);roots=0;bindings=0;root_count=root_capacity=binding_count=binding_capacity=0;extra=extra_token=nested=0;return 0;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=16||n)return p->invalid;id=*p;for(size_t i=0;i<5;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("retain:"),LIT("loadNative:"),LIT("visit:"),LIT("forward"),LIT("binding:value:"),LIT("configureVisit"),LIT("reset"),LIT("containsBinding"),LIT("bindingKeyCount"),LIT("rootCountFor:"),LIT("retainedToken"),LIT("configureLoad"),LIT("reentrantLoad"),LIT("dispose")};
static const IdMethodSpec methods[]={{5,forward},{4,visit},{6,binding},{7,configure_visit},{8,reset},{9,contains},{10,key_count},{11,root_matches},{12,retained_token},{13,configure_load},{14,reentrant_load},{15,dispose}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,16,methods,12,initialize};
