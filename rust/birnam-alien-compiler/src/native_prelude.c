/* Private generated-unit spelling of the existing Id native message ABI. */
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
typedef void *O;
typedef struct Operand A; typedef struct Outcome R; typedef struct Protocol P;
typedef R (*Entry)(void*,O,const A*,size_t);
struct Operand {uint32_t kind,reserved;union {O object;size_t word;struct{const unsigned char *pointer;size_t length;} bytes;struct{Entry method;void *context;} entry;const P *protocol;} data;};
struct Outcome {uint32_t status,reserved;size_t detail;A value;};
struct Protocol {uint32_t version,byte_size;R(*message)(void*,O,O,const A*,size_t);void *context;O receiver,intern,load;};
typedef struct{uint32_t version,byte_size;R(*initialize)(const P*,const A*,size_t);} Module;
static P id;static O ns,owner,roots;
static A obj(O o){A a={0};a.kind=1;a.data.object=o;return a;}
static A word(size_t w){A a={0};a.kind=2;a.data.word=w;return a;}
static A bytes(const void*p,size_t n){A a={0};a.kind=3;a.data.bytes.pointer=p;a.data.bytes.length=n;return a;}
static A entry(Entry f){A a={0};a.kind=4;a.data.entry.method=f;return a;}
static R bad(void){R r={0};r.status=1;return r;}
static R ok(A a){R r={0};r.value=a;return r;}
typedef struct{R failure;size_t *tokens,count,capacity;void **buffers;size_t buffer_count;} Call;
static O intern(Call*c,const char*n){if(c->failure.status)return 0;A a=bytes(n,strlen(n));R r=id.message(id.context,id.receiver,id.intern,&a,1);if(r.status||r.value.kind!=1){c->failure=r.status?r:bad();return 0;}return r.value.data.object;}
static R raw(Call*c,O receiver,const char*n,const A*a,size_t count){O sel=intern(c,n);if(c->failure.status)return c->failure;R r=id.message(id.context,receiver,sel,a,count);if(r.status)c->failure=r;return r;}
static O keep(Call*c,O o){if(c->failure.status)return 0;if(!o){c->failure=bad();return 0;}if(c->count==c->capacity){size_t cap=c->capacity?c->capacity*2:32;size_t*t=realloc(c->tokens,cap*sizeof(size_t));if(!t){c->failure=bad();return 0;}c->tokens=t;c->capacity=cap;}A a=obj(o);R r=raw(c,roots,"retain:",&a,1);if(r.status||r.value.kind!=2){c->failure=r.status?r:bad();return 0;}c->tokens[c->count++]=r.value.data.word;return o;}
static A send(Call*c,O receiver,const char*n,const A*a,size_t count){if(c->failure.status)return (A){0};R r=raw(c,receiver,n,a,count);if(!r.status&&r.value.kind==1)keep(c,r.value.data.object);return r.value;}
static O object(Call*c,A a){if(a.kind!=1||!a.data.object){if(!c->failure.status)c->failure=bad();return 0;}return a.data.object;}
static O call(Call*c,O receiver,const char*n,const A*a,size_t count){return object(c,send(c,receiver,n,a,count));}
static O named(Call*c,const char*n){A a=obj(intern(c,n));return call(c,ns,"at:",&a,1);}
static R finish(Call*c,A result){R failure=c->failure;c->failure=(R){0};for(size_t i=c->count;i>0;i--){A a=word(c->tokens[i-1]);raw(c,roots,"release:",&a,1);}free(c->tokens);for(size_t i=0;i<c->buffer_count;i++)free(c->buffers[i]);free(c->buffers);return failure.status?failure:c->failure.status?c->failure:ok(result);}

static Call *active;
static O __cbBlock;
static void begin(Call*c){R r=raw(c,named(c,"Alien"),"checkThread",0,0);if(r.status)abort();keep(c,owner);}
static O alien_nil(void){return named(active,"nil");}
static O alien_intern(const char*n){return keep(active,intern(active,n));}
static O alien_send(O r,O sel,O *args,long n){
 Call*c=active;if(c->failure.status)return 0;A*a=calloc(n?n:1,sizeof(A));if(!a){c->failure=bad();return 0;}
 for(long i=0;i<n;i++)a[i]=obj(args[i]);R answer=id.message(id.context,r,sel,a,n);free(a);
 if(answer.status){c->failure=answer;return 0;}return keep(c,object(c,answer.value));
}
static size_t unbox(const char*factory,const char*selector,O o){A a=obj(o);A r=send(active,named(active,factory),selector,&a,1);if(r.kind!=2){active->failure=bad();return 0;}return r.data.word;}
static O box(const char*factory,const char*selector,size_t w){A a=word(w);return call(active,named(active,factory),selector,&a,1);}
static O alien_mkSmallInt(long v){return box("Integer","fromWord:",(size_t)v);}
static long alien_smallIntValue(O v){return (long)unbox("Integer","wordOf:",v);}
static O alien_mkFloat(double v){union{double d;size_t w;}b;b.d=v;return box("Float","fromBits:",b.w);}
static double alien_floatValue(O v){union{double d;size_t w;}b;b.w=unbox("Float","bitsOf:",v);return b.d;}
static O alien_mkWord(size_t v){return box("Word","fromBits:",v);}
static size_t alien_uintValue(O v){A a=obj(v);A behavior=send(active,id.receiver,"behaviorOf:",&a,1);if(behavior.kind==1&&behavior.data.object==named(active,"WordVt")){A r=send(active,v,"bits",0,0);if(r.kind==2)return r.data.word;}return unbox("Integer","wordOf:",v);}
static O alien_mkForeignAddress(size_t v){return box("Alien","fromAddress:",v);}
static size_t alien_foreignAddressValue(O v){return unbox("Alien","addressOf:",v);}
static long alien_isForeignAddress(O v){A a=obj(v);A r=send(active,named(active,"Alien"),"addressOf:",&a,1);return r.kind==2;}
static O alien_mkString(const char*p,long n){A a=bytes(p,n);return call(active,named(active,"String"),"fromBytes:",&a,1);}
static const char*alien_cstr(O o){
 Call*c=active;A a=send(c,o,"bytes",0,0);if(a.kind!=3){c->failure=bad();return 0;}
 size_t n=a.data.bytes.length;char*p=malloc(n+1);if(!p){c->failure=bad();return 0;}
 memcpy(p,a.data.bytes.pointer,n);p[n]=0;
 void**b=realloc(c->buffers,(c->buffer_count+1)*sizeof(void*));if(!b){free(p);c->failure=bad();return 0;}c->buffers=b;b[c->buffer_count++]=p;return p;
}
static void alien_free_cstr(const char*p){(void)p; /* invocation scope owns copies */}
