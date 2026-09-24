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
typedef struct{R failure;size_t *tokens,count,capacity;} Call;
static O intern(Call*c,const char*n){if(c->failure.status)return 0;A a=bytes(n,strlen(n));R r=id.message(id.context,id.receiver,id.intern,&a,1);if(r.status||r.value.kind!=1){c->failure=r.status?r:bad();return 0;}return r.value.data.object;}
static R raw(Call*c,O receiver,const char*n,const A*a,size_t count){O sel=intern(c,n);if(c->failure.status)return c->failure;R r=id.message(id.context,receiver,sel,a,count);if(r.status)c->failure=r;return r;}
static O keep(Call*c,O o){if(c->failure.status)return 0;if(!o){c->failure=bad();return 0;}if(c->count==c->capacity){size_t cap=c->capacity?c->capacity*2:32;size_t*t=realloc(c->tokens,cap*sizeof(size_t));if(!t){c->failure=bad();return 0;}c->tokens=t;c->capacity=cap;}A a=obj(o);R r=raw(c,roots,"retain:",&a,1);if(r.status||r.value.kind!=2){c->failure=r.status?r:bad();return 0;}c->tokens[c->count++]=r.value.data.word;return o;}
static A send(Call*c,O receiver,const char*n,const A*a,size_t count){if(c->failure.status)return (A){0};R r=raw(c,receiver,n,a,count);if(!r.status&&r.value.kind==1)keep(c,r.value.data.object);return r.value;}
static O object(Call*c,A a){if(a.kind!=1||!a.data.object){if(!c->failure.status)c->failure=bad();return 0;}return a.data.object;}
static O call(Call*c,O receiver,const char*n,const A*a,size_t count){return object(c,send(c,receiver,n,a,count));}
static O named(Call*c,const char*n){A a=obj(intern(c,n));return call(c,ns,"at:",&a,1);}
static O integer(Call*c,size_t w){A a=word(w);return call(c,named(c,"Integer"),"fromWord:",&a,1);}
static O string(Call*c,const void*p,size_t n){A a=bytes(p,n);return call(c,named(c,"String"),"fromBytes:",&a,1);}
static O float_bits(Call*c,uint64_t w){A a=word(w);return call(c,named(c,"Float"),"fromBits:",&a,1);}
static O get(Call*c,O m,size_t slot){A a=obj(integer(c,slot));return call(c,m,"oopAt:",&a,1);}
static void put(Call*c,O m,size_t slot,O value){A a[2]={obj(integer(c,slot)),obj(value)};send(c,m,"oopAt:put:",a,2);}
static O action(Call*c,O m,const char*n,size_t slot){A a=obj(integer(c,slot));return call(c,m,n,&a,1);}
static R finish(Call*c,A result){R failure=c->failure;c->failure=(R){0};for(size_t i=c->count;i>0;i--){A a=word(c->tokens[i-1]);raw(c,roots,"release:",&a,1);}free(c->tokens);return failure.status?failure:c->failure.status?c->failure:ok(result);}
