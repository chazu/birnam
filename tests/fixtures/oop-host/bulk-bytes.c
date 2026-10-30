/* Custom providers exercise bulk delivery, failures, reentry and expired sinks. */
#include "../oop-id-v2/id.h"
static IdProtocol id; static Oop names[8];
enum { AT, PUT, APPEND, COLLECT, FROM, SIZE, COPY, SCALAR };
#define CHECK(v) do { Oop x=(v); if(id_fault(x)) return x; } while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop v=id_size(i);return send(c,AT,&v,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
/* [length, bulk calls, scalar calls, saved sink, flags, GC, Bytes, nested input] */
static Oop length(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;return n?id.invalid:get(c,0);}
static Oop scalar(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;(void)n;CHECK(put(c,2,id_size(id_unbox(get(c,2))+1)));return id.invalid;
}
static Oop copy(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=3)return id.invalid;
 size_t from=id_unbox(a[0]),count=id_unbox(a[1]),flags=id_unbox(get(c,4));
 CHECK(put(c,1,id_size(id_unbox(get(c,1))+1)));CHECK(put(c,3,a[2]));
 if(flags&1)CHECK(send(get(c,5),COLLECT,0,0));
 if(flags&2){Oop input=get(c,7);CHECK(send(get(c,6),FROM,&input,1));}
 if(flags&32)return id.invalid;
 if(flags&8){Oop bad=id_size(256);return send(a[2],APPEND,&bad,1);}
 if(flags&16){Oop bad=id_size(256);if(!id_fault(send(a[2],APPEND,&bad,1)))return id.invalid;}
 if(flags&4&&count)count--;
 Oop values[8192];
 for(size_t i=0;i<count;){size_t take=count-i;if(take>8192)take=8192;
  for(size_t j=0;j<take;j++)values[j]=id_size(((from+i+j)^165)&255);
  CHECK(send(a[2],APPEND,values,take));i+=take;
 }
 if(flags&64){Oop extra=id_size(0);return send(a[2],APPEND,&extra,1);}
 return 0;
}
/* Sink context: [source, total, seen, sequence?, GC]. */
static Oop receive(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;size_t seen=id_unbox(get(c,2)),total=id_unbox(get(c,1));
 if(n>total-seen)return id.invalid;
 for(size_t i=0;i<n;i++)if(a[i]!=id_size((seen+i)&255))return id.invalid;
 CHECK(put(c,2,id_size(seen+n)));
 if(!seen){
  Oop source=get(c,0);
  if(id_unbox(get(c,3))){CHECK(put(source,total-1,id_size(254)));}
  else {Oop extra[8192];for(size_t i=0;i<8192;i++)extra[i]=id_size(7);CHECK(send(source,APPEND,extra,8192));}
  CHECK(send(get(c,4),COLLECT,0,0));
 }
 return r;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*l,size_t n,const Oop*a,size_t argc){
 (void)a;if(n!=8||argc)return p->invalid;id=*p;for(size_t i=0;i<n;i++)names[i]=l[i];return image;
}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("appendByteValues"),LIT("collectPrecisely"),LIT("fromBytes:"),LIT("sizeWord"),LIT("copyByteValuesFrom:count:to:"),LIT("atWord:")};
static const IdMethodSpec methods[]={{SIZE,length},{COPY,copy},{SCALAR,scalar},{APPEND,receive}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,8,methods,4,initialize};
