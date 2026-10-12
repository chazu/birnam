/* Private generated-unit spelling of the existing Id native message ABI.
   Static inline lets TinyCC emit only referenced helpers and their transitive
   dependencies. Bodies, unit-local state and ordinary Id sends stay unchanged;
   generated entrypoints and initializers retain their existing linkage. */
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
static inline A obj(O o){A a={0};a.kind=1;a.data.object=o;return a;}
static inline A word(size_t w){A a={0};a.kind=2;a.data.word=w;return a;}
static inline A bytes(const void*p,size_t n){A a={0};a.kind=3;a.data.bytes.pointer=p;a.data.bytes.length=n;return a;}
static inline A entry(Entry f){A a={0};a.kind=4;a.data.entry.method=f;return a;}
static inline R bad(void){R r={0};r.status=1;return r;}
static inline R ok(A a){R r={0};r.value=a;return r;}
typedef struct{R failure;size_t *tokens,count,capacity;O inputs[9];size_t input_count;O covered[32];size_t covered_count,covered_next;int reuse;} Call;
static inline O intern(Call*c,const char*n){if(c->failure.status)return 0;A a=bytes(n,strlen(n));R r=id.message(id.context,id.receiver,id.intern,&a,1);if(r.status||r.value.kind!=1){c->failure=r.status?r:bad();return 0;}return r.value.data.object;}
/* Selector spellings in generated code are static for the Unit's lifetime.
   TCC may store identical literals at different addresses: cache by spelling.
   Root their Symbols in that Unit; retain live lookup on every actual send. */
typedef struct {const char *name;O symbol;} Selector;
#ifndef BIRNAM_SELECTOR_CAPACITY
#define BIRNAM_SELECTOR_CAPACITY 512
#endif
#if BIRNAM_SELECTOR_CAPACITY < 1 || BIRNAM_SELECTOR_CAPACITY > 512 || (BIRNAM_SELECTOR_CAPACITY & (BIRNAM_SELECTOR_CAPACITY - 1))
#error BIRNAM_SELECTOR_CAPACITY_must_be_a_power_of_two_up_to_512
#endif
static Selector selectors[BIRNAM_SELECTOR_CAPACITY];
static int cache_selectors;
static int batch_cleanup=1;
static inline O selector(Call*c,const char*n);
static inline R raw(Call*c,O receiver,const char*n,const A*a,size_t count){O sel=selector(c,n);if(c->failure.status)return c->failure;R r=id.message(id.context,receiver,sel,a,count);if(r.status)c->failure=r;return r;}
/* Bounded hints contain only opaque references whose protection lasts until
   this Call exits. Eviction loses an optimization, never the covering token.
   Entries are added AFTER successful retain or pure capture admission. */
static inline void cover(Call*c,O o){if(!c->reuse)return;if(c->covered_count<32)c->covered[c->covered_count++]=o;else{c->covered[c->covered_next]=o;c->covered_next=(c->covered_next+1)&31;}}
static inline O keep(Call*c,O o){if(c->failure.status)return 0;if(!o){c->failure=bad();return 0;}for(size_t i=0;i<c->input_count;i++)if(c->inputs[i]==o)return o;if(c->reuse)for(size_t i=0;i<c->covered_count;i++)if(c->covered[i]==o)return o;if(c->count==c->capacity){size_t cap=c->capacity?c->capacity*2:32;size_t*t=realloc(c->tokens,cap*sizeof(size_t));if(!t){c->failure=bad();return 0;}c->tokens=t;c->capacity=cap;}A a=obj(o);R r=raw(c,roots,"retain:",&a,1);if(r.status||r.value.kind!=2||!r.value.data.word){c->failure=r.status?r:bad();return 0;}c->tokens[c->count++]=r.value.data.word;cover(c,o);return o;}
static inline O object(Call*c,A a);
/* Execution owns the covering invocation scope; native callers likewise keep
   receiver/arguments live throughout the call under the Id ABI contract.
   Validate the Object carriers without registering the same inputs again.
   Fresh message results still use keep(): input coverage says nothing about them. */
static inline void check_entry(Call*c,O self,const A*args,size_t n){
 c->reuse=1;
 object(c,obj(self));
 if(n<=8&&!c->failure.status){c->inputs[0]=self;for(size_t i=0;i<n&&!c->failure.status;i++)c->inputs[i+1]=object(c,args[i]);if(!c->failure.status)c->input_count=n+1;}
 else for(size_t i=0;i<n&&!c->failure.status;i++)object(c,args[i]);
}
/* Diagnostic compiler control: independent registrations, no batching layer. */
static inline void keep_entry(Call*c,O self,const A*args,size_t n){
 keep(c,self);for(size_t i=0;i<n&&!c->failure.status;i++)keep(c,object(c,args[i]));
}
static inline A send(Call*c,O receiver,const char*n,const A*a,size_t count){if(c->failure.status)return (A){0};R r=raw(c,receiver,n,a,count);if(!r.status&&r.value.kind==1)keep(c,r.value.data.object);return r.value;}
static inline O object(Call*c,A a){if(a.kind!=1||!a.data.object){if(!c->failure.status)c->failure=bad();return 0;}return a.data.object;}
static inline O call(Call*c,O receiver,const char*n,const A*a,size_t count){return object(c,send(c,receiver,n,a,count));}
static inline O cached_name(Call*c,const char*n,int*owned){
 if(owned)*owned=0;
 if(!cache_selectors||c->failure.status)return intern(c,n);
 uint32_t hash=UINT32_C(2166136261);
 for(const unsigned char*p=(const unsigned char*)n;*p;p++){hash^=*p;hash*=UINT32_C(16777619);}
 size_t slot=hash&(BIRNAM_SELECTOR_CAPACITY-1);
 for(size_t probe=0;probe<BIRNAM_SELECTOR_CAPACITY;probe++,slot=(slot+1)&(BIRNAM_SELECTOR_CAPACITY-1)){
  if(selectors[slot].name&&(selectors[slot].name==n||!strcmp(selectors[slot].name,n))){if(owned)*owned=1;return selectors[slot].symbol;}
  if(!selectors[slot].name){
   O symbol=intern(c,n);if(c->failure.status)return 0;
   selectors[slot]=(Selector){n,symbol};
   keep(c,symbol);
   A a=obj(symbol);call(c,owner,"retain:",&a,1);
   if(c->failure.status){selectors[slot]=(Selector){0};return 0;}
   if(owned)*owned=1;
   return symbol;
  }
 }
 return intern(c,n);
}
static inline O selector(Call*c,const char*n){return cached_name(c,n,0);}
/* Literal Symbols share the Unit's already traced name inventory. Cache misses
   and full-cache fallback still acquire protection before any later callback. */
static inline O literal_symbol(Call*c,const char*n){int owned=0;O value=cached_name(c,n,&owned);if(owned)cover(c,value);return keep(c,value);}
static inline O named(Call*c,const char*n){A a=obj(selector(c,n));return call(c,ns,"at:",&a,1);}
/* Ask the actual Block for an ordered borrow, otherwise dispatch its readers.
   All selector identities are resolved BEFORE checking the borrowing mode.
   1 = canonical callback-free reads covered by this live Block; 0 = retain
   replaced/custom results; 2 = nonportable receiver, use owner inspection to
   preserve Method invokeOn: behavior for receivers that have no Block messages.
   Owner queries and fallback reads use Id, never a remembered executable. */
static inline void load_captures(Call*c,O self,O*out,size_t count){
 O bulk=selector(c,"tryBorrowCaptureOperands:"),read=selector(c,"captureWordAt:"),permit=selector(c,"captureReadMode"),fallback=selector(c,"captureReadModeOf:"),inspect=selector(c,"captureWord:of:");
 if(c->failure.status)return;
 A length=word(count);R snapshot=id.message(id.context,self,bulk,&length,1);
 if(snapshot.status&&snapshot.status!=6){c->failure=snapshot;return;}
 if(!snapshot.status&&snapshot.value.kind==3){
  /* Copy the returned ABI borrow before any callback. The try-borrow contract
     covers every returned Object by this live receiver's immutable edges. */
  if(count>SIZE_MAX/sizeof(O)||snapshot.value.data.bytes.length!=count*sizeof(O)||(count&&!snapshot.value.data.bytes.pointer)){c->failure=bad();return;}
  if(count)memcpy(out,snapshot.value.data.bytes.pointer,count*sizeof(O));
  for(size_t i=0;i<count;i++)if(!object(c,obj(out[i])))return;
  for(size_t i=0;i<count;i++)cover(c,out[i]);
  return;
 }
 if(!snapshot.status&&snapshot.value.kind!=0){c->failure=bad();return;}
 R grant=id.message(id.context,self,permit,0,0);
 O factory=0;
 /* Non-Block Method receivers need owner inspection. If a Block's direct
    query is absent, this fallback still admits its CURRENT reader instead of
    bypassing it through inspection. Resolve the factory before its query. */
 if(grant.status==6){factory=named(c,"Block");if(c->failure.status)return;A target=obj(self);grant=id.message(id.context,factory,fallback,&target,1);}
 size_t mode=0;
 if(grant.status){if(grant.status!=6){c->failure=grant;return;}}
 else if(grant.value.kind==2&&grant.value.data.word<=2)mode=grant.value.data.word;
 else {c->failure=bad();return;}
 if(mode==2&&!factory)factory=named(c,"Block");
 for(size_t i=0;i<count&&!c->failure.status;i++){
  A arguments[2]={word(i),obj(self)};
  R result=id.message(id.context,mode==2?factory:self,mode==2?inspect:read,arguments,mode==2?2:1);
  if(result.status){c->failure=result;return;}
  O value=object(c,result.value);if(!c->failure.status)out[i]=mode==1?value:keep(c,value);
 }
 /* Only a completely successful pure batch proves the immutable-edge cover.
    Replacement/fallback results above have their own ordinary registrations. */
 if(mode==1&&!c->failure.status)for(size_t i=0;i<count;i++)cover(c,out[i]);
}
/* Error is installed from authored code after the compiler floor exists.
   Before that, native failures propagate and no Error family is available. */
static inline int is_error(Call*c,O value){A key=obj(selector(c,"Error"));A factory=send(c,ns,"at:",&key,1);if(!c->failure.status&&factory.kind==0)return 0;object(c,factory);return call(c,value,"isError",0,0)==named(c,"true");}
/* Preserve source lookup: Globals first, then bootstrap exports on nil. */
static inline O global(Call*c,const char*n){A key=obj(literal_symbol(c,n));return call(c,ns,"resolveGlobal:",&key,1);}
static inline O global_put(Call*c,const char*n,O value){A a[2]={obj(keep(c,selector(c,n))),obj(value)};return call(c,named(c,"Globals"),"at:put:",a,2);}
static inline O integer(Call*c,size_t w){A a=word(w);return call(c,named(c,"Integer"),"fromWord:",&a,1);}
static inline O string(Call*c,const void*p,size_t n){A a=bytes(p,n);return call(c,named(c,"String"),"fromBytes:",&a,1);}
static inline O float_bits(Call*c,uint64_t w){A a=word(w);return call(c,named(c,"Float"),"fromBits:",&a,1);}
static inline O get(Call*c,O m,size_t slot){A a=obj(integer(c,slot));return call(c,m,"oopAt:",&a,1);}
static inline void put(Call*c,O m,size_t slot,O value){A a[2]={obj(integer(c,slot)),obj(value)};send(c,m,"oopAt:put:",a,2);}
static inline O action(Call*c,O m,const char*n,size_t slot){A a=obj(integer(c,slot));return call(c,m,n,&a,1);}
/* Optional frame preparation is selected through Id. Test presence before
   invocation: an UNBOUND failure from an entered guard must not be mistaken
   for an absent protocol and replayed as a baseline call. */
static inline R try_frame_call(Call*c,O machine,size_t site,const unsigned char*slots,size_t length,const char*name,size_t tail){
 O sel=selector(c,"tryPrepareCall:slots:selector:in:tail:");
 A binding[2]={obj(sel),obj(machine)};
 R bound=raw(c,id.receiver,"bind:to:",binding,2);
 if(bound.status)return bound;
 if(bound.value.kind==0)return ok((A){0});
 if(bound.value.kind!=4||!bound.value.data.entry.method){c->failure=bad();return c->failure;}
 A args[5]={word(site),bytes(slots,length),obj(selector(c,name)),obj(ns),word(tail)};
 return raw(c,machine,"tryPrepareCall:slots:selector:in:tail:",args,5);
}
static inline R try_frame_call_copies(Call*c,O machine,size_t site,const unsigned char*slots,size_t length,const unsigned char*copies,size_t copy_length,const char*name,size_t tail){
 O sel=selector(c,"tryPrepareCall:slots:copies:selector:in:tail:");
 A binding[2]={obj(sel),obj(machine)};
 R bound=raw(c,id.receiver,"bind:to:",binding,2);
 if(bound.status)return bound;
 if(bound.value.kind==0)return ok((A){0});
 if(bound.value.kind!=4||!bound.value.data.entry.method){c->failure=bad();return c->failure;}
 A args[6]={word(site),bytes(slots,length),bytes(copies,copy_length),obj(selector(c,name)),obj(ns),word(tail)};
 return raw(c,machine,"tryPrepareCall:slots:copies:selector:in:tail:",args,6);
}
static inline R finish(Call*c,A result){
 R failure=c->failure;c->failure=(R){0};int handled=0;
 /* First-use selector caching can append temporary roots. Prepare cleanup
    before taking the final token count, so fallback does not leak them. */
 selector(c,"release:");
 /* Cleanup consumes token ownership but must keep the returned Object live
    through every current callback. Resolve the selector before borrowing the
    final token array: a cold cache miss can append/reallocate tokens. */
 if(!failure.status&&result.kind==1&&result.data.object&&c->count){
  O sel=selector(c,"releaseMany:protecting:batch:");
  if(!c->failure.status){
   A a[3]={bytes(c->tokens,c->count*sizeof(size_t)),result,word(batch_cleanup)};
   R r=id.message(id.context,roots,sel,a,3);
   handled=r.status!=6; /* Missing operation: drain below, but return failure. */
   if(r.status)failure=r;else if(r.value.kind!=2||r.value.data.word!=1)failure=bad();
  }else{failure=c->failure;c->failure=(R){0};}
 }else if(batch_cleanup&&c->count>=4){
  O sel=selector(c,"tryReleaseMany:");
  if(!c->failure.status){
   A a=bytes(c->tokens,c->count*sizeof(size_t));R r=id.message(id.context,roots,sel,&a,1);
   if(r.status)c->failure=r;
   else if(r.value.kind!=2||r.value.data.word>1)c->failure=bad();else handled=r.value.data.word==1;
  }
  if(c->failure.status){if(!failure.status)failure=c->failure;c->failure=(R){0};}
 }
 if(!handled)for(size_t i=c->count;i>0;i--){
  A a=word(c->tokens[i-1]);raw(c,roots,"release:",&a,1);
  if(c->failure.status){if(!failure.status)failure=c->failure;c->failure=(R){0};}
 }
 free(c->tokens);return failure.status?failure:c->failure.status?c->failure:ok(result);
}

/* A final send in an otherwise root-free call needs no temporary result token:
   there is no callback between its result and the receiving caller. Resolve
   its selector first; a cold cache miss may create roots and require fallback.
   This is ordinary Id dispatch, not tail-frame elimination or a cached entry. */
static inline R return_call(Call*c,O receiver,const char*n,const A*a,size_t count){
 O sel=selector(c,n);if(c->failure.status)return finish(c,(A){0});
 if(!c->count){R r=id.message(id.context,receiver,sel,a,count);free(c->tokens);if(r.status)return r;if(r.value.kind!=1||!r.value.data.object)return bad();return r;}
 return finish(c,obj(call(c,receiver,n,a,count)));
}
