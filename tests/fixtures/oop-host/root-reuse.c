/* Generated-call lifetime observation through weak allocation identities. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT,CLONE,COLLECT,IDENTITY,ALLOCATIONS,ALIAS,SEQUENCE,FRESH,ECHO,CHECKPOINT,
       RETAIN,RESET,COUNTS,REMAINING,ROW,COUNT };
static Oop names[COUNT],marker;
static size_t created[128],created_count,retained,marker_retained,checkpoints,mode,remaining;
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked))return checked;}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop index=id_size(i);return send(c,AT,&index,1);}
/* Context: [Object, GC, marker identity, exact native failure]. */
static Oop fresh(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n||created_count==128)return id.invalid;
 Oop value=send(get(c,0),CLONE,0,0);CHECK(value);
 Oop identity=send(id.root,IDENTITY,&value,1);CHECK(identity);created[created_count++]=id_unbox(identity);return value;
}
static Oop echo(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;return n==1?a[0]:id.invalid;}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=3)return id.invalid;
 size_t identity=id_unbox(a[2]);for(size_t i=0;i<created_count;i++)if(identity==created[i])remaining++;
 if(identity==id_unbox(get(c,2)))marker=a[0];return 0;
}
static Oop scan(Oop r){remaining=0;marker=0;CHECK(send(id.root,ALLOCATIONS,&r,1));return id_size(remaining);}
static Oop checkpoint(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)a;if(n)return id.invalid;checkpoints++;
 CHECK(send(get(c,1),COLLECT,0,0));CHECK(scan(r));
 if(remaining!=created_count||!marker)return id.invalid;return marker;
}
static Oop retain(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;if(n!=1)return id.invalid;
 if(a[0]&&!((uintptr_t)a[0]&1)){
  Oop value=send(id.root,IDENTITY,a,1);CHECK(value);size_t identity=id_unbox(value);
  if(identity==id_unbox(get(c,2)))marker_retained++;
  for(size_t i=0;i<created_count;i++)if(identity==created[i]){
   retained++;if(mode==1)return id_failure(get(c,3));if(mode==2)return id_size(0);if(mode==3)return 0;break;
  }
 }
 return send(r,ALIAS,a,n);
}
static Oop reset(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=1)return id.invalid;
 created_count=retained=marker_retained=checkpoints=remaining=0;marker=0;mode=id_unbox(a[0]);return 0;
}
static Oop counts(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;(void)a;if(n)return id.invalid;
 Oop values[]={id_size(created_count),id_size(retained),id_size(marker_retained),id_size(checkpoints)};return send(id.root,SEQUENCE,values,4);
}
static Oop remaining_values(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)a;if(n)return id.invalid;return scan(r);}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=COUNT||n)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("clone"),LIT("collectPrecisely"),LIT("identityOf:"),LIT("allocationsDo:"),LIT("reuseOriginalRetain:"),LIT("sequence"),LIT("fresh"),LIT("echo:"),LIT("checkpoint"),LIT("retain"),LIT("reset:"),LIT("counts"),LIT("remaining"),LIT("allocation:bytes:identity:")};
static const IdMethodSpec methods[]={{FRESH,fresh},{ECHO,echo},{CHECKPOINT,checkpoint},{RETAIN,retain},{RESET,reset},{COUNTS,counts},{REMAINING,remaining_values},{ROW,row}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,8,initialize};
