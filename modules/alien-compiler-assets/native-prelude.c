/* Generated C owns foreign marshalling; every Id message carries only Oops. */
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
typedef void *O;
typedef O (*Entry)(O,O,O,const O*,size_t);
typedef struct {
 uint32_t version,byte_size;
 O (*message)(O,O,O,const O*,size_t);
 O root,intern,load,at,seeds,invalid;
} P;
typedef struct {const unsigned char *bytes;size_t length;} Literal;
typedef struct {size_t name;Entry entry;} MethodSpec;
typedef struct {
 uint32_t version,byte_size;
 const Literal *literals;size_t literal_count;
 const MethodSpec *methods;size_t method_count;
 O (*initialize)(const P*,O,const O*,size_t,const O*,size_t);
} Module;
#define L(s) {(const unsigned char*)s,sizeof(s)-1}
static const Literal literals[]={L("nameWithBytes"),L("Roots"),L("at:"),L("retain:"),L("releaseTokensProtecting"),L("invoke")};
static P id;static O ns,owner,roots,keys[6];
static int failed(O v){return ((uintptr_t)v&3)==2;}
static O reference(O v){return (O)((uintptr_t)v&~(uintptr_t)2);}
static O immediate(size_t v){return (O)((v<<1)|1);}
static O bad(void){return id.invalid;}
typedef struct {O failure;O *tokens;size_t count,capacity;void **buffers;size_t buffer_count;} Call;
static Call *active;
static O __cbBlock;
static O intern(Call*c,const char*n){
 if(c->failure)return 0;
 for(size_t i=0;i<6;i++)if(!strcmp(n,(const char*)literals[i].bytes))return keys[i];
 size_t len=strlen(n);O*a=malloc((len?len:1)*sizeof(O));if(!a){c->failure=bad();return 0;}
 for(size_t i=0;i<len;i++)a[i]=immediate((unsigned char)n[i]);
 O name=id.message(id.root,id.root,keys[0],a,len);free(a);
 if(failed(name)){c->failure=name;return 0;}
 O result=id.message(id.root,name,id.intern,0,0);
 if(failed(result)){c->failure=result;return 0;}return result;
}
static O raw(Call*c,O receiver,const char*n,const O*a,size_t count){
 O selector=intern(c,n);if(c->failure)return 0;
 O result=id.message(id.root,receiver,selector,a,count);
 if(failed(result)){c->failure=result;return 0;}return result;
}
static O keep(Call*c,O o){
 if(c->failure)return 0;
 if(c->count==c->capacity){size_t cap=c->capacity?c->capacity*2:32;O*t=realloc(c->tokens,(cap+1)*sizeof(O));if(!t){c->failure=bad();return 0;}c->tokens=t;c->capacity=cap;}
 O token=raw(c,roots,"retain:",&o,1);if(c->failure)return 0;
 c->tokens[++c->count]=token;return o;
}
static O call(Call*c,O receiver,const char*n,const O*a,size_t count){return keep(c,raw(c,receiver,n,a,count));}
static O named(Call*c,const char*n){O name=intern(c,n);return call(c,ns,"at:",&name,1);}
static O finish(Call*c,O result){
 O failure=c->failure;
 if(c->tokens){c->tokens[0]=failure?reference(failure):result;
  O cleaned=id.message(id.root,roots,keys[4],c->tokens,c->count+1);
  if(!failure&&failed(cleaned))failure=cleaned;
 }
 free(c->tokens);for(size_t i=0;i<c->buffer_count;i++)free(c->buffers[i]);free(c->buffers);
 return failure?failure:result;
}
static void begin(Call*c){raw(c,named(c,"Alien"),"checkThread",0,0);if(c->failure)abort();keep(c,owner);}
static O alien_nil(void){return named(active,"nil");}
static O alien_intern(const char*n){return keep(active,intern(active,n));}
static O alien_send(O r,O selector,O*args,long n){
 Call*c=active;if(c->failure)return 0;
 O result=id.message(id.root,r,selector,args,(size_t)n);
 if(failed(result)){c->failure=result;return 0;}return keep(c,result);
}
static size_t bits(Call*c,O value){
 if(c->failure)return 0;
 if(((uintptr_t)value&1)==1)return (uintptr_t)value>>1;
 O lo=call(c,value,"low32",0,0),hi=call(c,value,"high32",0,0);
 if(((uintptr_t)lo&1)!=1||((uintptr_t)hi&1)!=1){c->failure=bad();return 0;}
 return ((uintptr_t)lo>>1)|(((uintptr_t)hi>>1)<<32);
}
static O word(Call*c,size_t value){
 if(value<=UINTPTR_MAX>>1)return immediate(value);
 O args[2]={immediate(value&0xffffffff),immediate(value>>32)};
 return call(c,named(c,"Word"),"fromUnsignedLow:high:",args,2);
}
static size_t unbox(const char*factory,const char*selector,O value){return bits(active,call(active,named(active,factory),selector,&value,1));}
static O box(const char*factory,const char*selector,size_t value){O arg=word(active,value);return call(active,named(active,factory),selector,&arg,1);}
static O alien_mkSmallInt(long v){return box("Integer","fromWord:",(size_t)v);}
static long alien_smallIntValue(O v){return (long)unbox("Integer","wordOf:",v);}
static O alien_mkFloat(double v){union{double d;size_t w;}b;b.d=v;return box("Float","fromBits:",b.w);}
static double alien_floatValue(O v){union{double d;size_t w;}b;b.w=unbox("Float","bitsOf:",v);return b.d;}
static O alien_mkWord(size_t v){return box("Word","fromBits:",v);}
static size_t alien_uintValue(O v){
 O behavior=call(active,id.root,"behaviorOf:",&v,1);
 if(behavior==named(active,"WordVt"))return bits(active,call(active,v,"bits",0,0));
 return unbox("Integer","wordOf:",v);
}
static O alien_mkForeignAddress(size_t v){return box("Alien","fromAddress:",v);}
static size_t alien_foreignAddressValue(O v){return unbox("Alien","addressOf:",v);}
static long alien_isForeignAddress(O v){return call(active,named(active,"Alien"),"addressOf:",&v,1)!=0;}
static O alien_mkString(const char*p,long length){
 Call*c=active;if(length<0){c->failure=bad();return 0;}size_t n=(size_t)length;
 O*a=malloc((n?n:1)*sizeof(O));if(!a){c->failure=bad();return 0;}
 for(size_t i=0;i<n;i++)a[i]=immediate((unsigned char)p[i]);
 O bytes=call(c,id.root,"sequence",a,n);free(a);return call(c,named(c,"String"),"fromBytes:",&bytes,1);
}
static const char*alien_cstr(O value){
 Call*c=active;O bytes=call(c,value,"bytes",0,0);size_t n=bits(c,call(c,bytes,"sizeWord",0,0));
 if(c->failure||n==SIZE_MAX)return 0;
 char*p=malloc(n+1);if(!p){c->failure=bad();return 0;}
 void**b=realloc(c->buffers,(c->buffer_count+1)*sizeof(void*));if(!b){free(p);c->failure=bad();return 0;}c->buffers=b;b[c->buffer_count++]=p;
 for(size_t i=0;i<n&&!c->failure;i++){O index=immediate(i);p[i]=(char)bits(c,call(c,bytes,"atWord:",&index,1));}p[n]=0;return p;
}
static void alien_free_cstr(const char*p){(void)p; /* invocation scope owns copies */}
