/* Generated Oop-only module. Native state here belongs to this compiled Unit. */
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
typedef void *O;
typedef O (*Entry)(O,O,O,const O*,size_t);
typedef struct {uint32_t version,byte_size;Entry message;O root,intern,load,at,seeds,invalid;} P;
typedef struct {const unsigned char *bytes;size_t length;} Literal;
typedef O (*Control)(O,O,const O*,size_t);
typedef struct {P common;Control control;} CP;
typedef struct {size_t name;Entry entry;uint32_t convention;} MethodSpec;
typedef O (*Initializer)(const CP*,O,const O*,size_t,const O*,size_t);
typedef struct {uint32_t version,byte_size;const Literal*literals;size_t literal_count;const MethodSpec*methods;size_t method_count;Initializer initialize;} Module;
static P id;static Control control;static O ns,owner,roots;
static O symbols[SYMBOL_COUNT],methods[METHOD_COUNT];
static inline O size_oop(size_t n){return (O)((n<<1)|1);}
static inline int failed(O v){return ((uintptr_t)v&3)==2;}
static inline O error_object(O v){return (O)((uintptr_t)v&~(uintptr_t)2);}
typedef struct {O error;O*tokens;size_t count,capacity;O local[17];} Call;
static inline O raw(Call*c,O receiver,size_t selector,const O*a,size_t n){if(c->error)return 0;O result=id.message(id.root,receiver,symbols[selector],a,n);if(failed(result)){c->error=result;return 0;}return result;}
static inline O keep(Call*c,O result){
 if(c->error)return 0;
 if(!result||((uintptr_t)result&1))return result;
 if(!c->capacity){c->tokens=c->local;c->capacity=17;}
 if(c->count+1>=c->capacity){size_t cap=c->capacity?c->capacity*2:32;if(cap<=c->capacity||cap>SIZE_MAX/sizeof(O)){c->error=id.invalid;return 0;}O*p=c->tokens==c->local?malloc(cap*sizeof(O)):realloc(c->tokens,cap*sizeof(O));if(!p){c->error=id.invalid;return 0;}if(c->tokens==c->local)for(size_t i=1;i<=c->count;i++)p[i]=c->local[i];c->tokens=p;c->capacity=cap;}
 O token=raw(c,roots,S_RETAIN,&result,1);if(c->error)return 0;
 if(!((uintptr_t)token&1)||token==size_oop(0)){c->error=id.invalid;return 0;}
 c->tokens[++c->count]=token;return result;
}
static inline O call(Call*c,O receiver,size_t selector,const O*a,size_t n){O result=raw(c,receiver,selector,a,n);return c->error?0:keep(c,result);}
static inline O named(Call*c,size_t name){O key=symbols[name];return call(c,ns,S_AT,&key,1);}
static inline int is_error(Call*c,O value){
 /* The first cold artifact defines Error itself. */
 O factory=named(c,S_ERROR);if(c->error||!factory)return 0;
 O truth=named(c,S_TRUE);return call(c,value,S_IS_ERROR,0,0)==truth;
}
static inline O finish(Call*c,O result){
 O failure=c->error;
 if(c->count){c->tokens[0]=failure?error_object(failure):result;O cleanup=id.message(id.root,roots,symbols[S_CLEANUP],c->tokens,c->count+1);if(!failure&&failed(cleanup))failure=cleanup;}
 if(c->tokens!=c->local)free(c->tokens);return failure?failure:result;
}
static inline O full_word(Call*c,uint64_t bits){O factory=named(c,S_WORD);O a[2]={size_oop((uint32_t)bits),size_oop((uint32_t)(bits>>32))};return call(c,factory,S_FROM_PARTS,a,2);}
static inline O integer(Call*c,uint64_t bits){O word=full_word(c,bits);O factory=named(c,S_INTEGER);return call(c,factory,S_FROM_WORD,&word,1);}
static inline O float_bits(Call*c,uint64_t bits){O word=full_word(c,bits);O factory=named(c,S_FLOAT);return call(c,factory,S_FROM_BITS,&word,1);}
/* Descriptor/program metadata is binary data, independent of String literals. */
static inline O blob(Call*c,const unsigned char*bytes,size_t n){
 const size_t batch=8192;O*values=n?malloc((n<batch?n:batch)*sizeof(O)):0;
 if(n&&!values){c->error=id.invalid;return 0;}
 size_t count=n<batch?n:batch;for(size_t i=0;i<count;i++)values[i]=size_oop(bytes[i]);
 O result=call(c,named(c,S_BYTES),S_FROM_BYTE_VALUES,values,count);
 for(size_t start=count;start<n&&!c->error;start+=count){
  count=n-start<batch?n-start:batch;
  for(size_t i=0;i<count;i++)values[i]=size_oop(bytes[start+i]);
  O next=raw(c,result,S_APPEND_BYTE_VALUES,values,count);
  if(!c->error&&next!=result)keep(c,next);result=next;
 }
 free(values);return result;
}
static inline O string(Call*c,const unsigned char*bytes,size_t n){
 O data=blob(c,bytes,n);O factory=named(c,S_STRING);return call(c,factory,S_FROM_BYTES,&data,1);
}
static inline O global(Call*c,size_t name){O key=symbols[name];return call(c,ns,S_GLOBAL,&key,1);}
/* Machine owns logical slots; generated code never reads its payload. */
static inline O frame_get(Call*c,O machine,size_t slot,size_t selector){O index=size_oop(slot);return call(c,machine,selector,&index,1);}
static inline void frame_put(Call*c,O machine,size_t slot,O value,size_t selector){O index=size_oop(slot);O args[]={index,value};raw(c,machine,selector,args,2);}
