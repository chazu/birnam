/* Matched caller workload: version 1 raw carriers versus version 2 Oops.
   Native loading is outside timing. This compares complete primitive paths,
   including root operations and explicit GC, not equivalent implementation work. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <dlfcn.h>
#include <assert.h>
#ifdef V2
#include "id.h"
static IdProtocol id;
#else
#include "../../../rust/birnam-abi/tests/fixtures/id-protocol-v1/id_protocol.h"
typedef BirnamObject Oop;
static BirnamProtocol id;
static BirnamOperand object(Oop v){BirnamOperand a={0};a.kind=BIRNAM_OBJECT;a.data.object=v;return a;}
static BirnamOperand word(size_t v){BirnamOperand a={0};a.kind=BIRNAM_WORD;a.data.word=v;return a;}
static BirnamOperand bytes(const char*p,size_t n){BirnamOperand a={0};a.kind=BIRNAM_BYTES;a.data.bytes.pointer=(const unsigned char*)p;a.data.bytes.length=n;return a;}
static BirnamOperand raw(Oop r,Oop s,const BirnamOperand*a,size_t n){BirnamOutcome v=id.message(id.context,r,s,a,n);if(v.status){fprintf(stderr,"baseline failure %u %zu\n",v.status,v.detail);exit(2);}return v.value;}
#endif
static const char*words[]={"sequence","objectBehavior","objectPrototype","behaviorOf:","allocate","retain:","release:","activate","automaticCollection:","collectPrecisely","allocationCount","nameWithBytes","identityOf:"};
enum{SEQ,BASE,PROTO,BEHAVIOR,ALLOC,RETAIN,RELEASE,ACTIVATE,AUTO,COLLECT,COUNT,NEWNAME,IDENTITY};
static Oop selectors[13],root,path_roots,path_gc;
#ifdef V2
static Oop checked(Oop v){if(id_fault(v)){fprintf(stderr,"candidate failure\n");exit(2);}return v;}
static Oop send(Oop r,Oop s,const Oop*a,size_t n){return checked(id.message(id.root,r,s,a,n));}
static Oop *name_arguments;
static Oop make_name(const char*b,size_t n){assert(n<=1024);for(size_t i=0;i<n;i++)name_arguments[i]=id_size((unsigned char)b[i]);Oop name=send(root,selectors[NEWNAME],name_arguments,n);return name;}
static Oop load(Oop path,Oop argument,int count){Oop seq=send(root,selectors[SEQ],&argument,count);return send(path,id.load,&seq,1);}
static Oop alloc(Oop behavior){Oop a=id_size(0);return send(behavior,selectors[ALLOC],&a,1);}
static Oop retain(Oop roots,Oop value){return send(roots,selectors[RETAIN],&value,1);}
static void release(Oop roots,Oop token){send(roots,selectors[RELEASE],&token,1);}
static size_t count_objects(void){return id_unbox(send(root,selectors[COUNT],0,0));}
#else
static Oop intern(const char*b,size_t n){BirnamOperand a=bytes(b,n);return raw(id.receiver,id.intern,&a,1).data.object;}
static Oop load(const char*path,Oop argument,int count){BirnamOperand a[]={bytes(path,strlen(path)),object(argument)};return raw(id.receiver,id.load,a,1+count).data.object;}
static Oop alloc(Oop behavior){BirnamOperand a=word(0);return raw(behavior,selectors[ALLOC],&a,1).data.object;}
static Oop retain(Oop roots,Oop value){BirnamOperand a=object(value);return (Oop)raw(roots,selectors[RETAIN],&a,1).data.word;}
static void release(Oop roots,Oop token){BirnamOperand a=word((size_t)token);raw(roots,selectors[RELEASE],&a,1);}
static size_t count_objects(void){return raw(id.receiver,selectors[COUNT],0,0).data.word;}
#endif
static uint64_t now(void){return clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID);}
static void result(const char*name,size_t n,uint64_t elapsed){printf("{\"case\":\"%s\",\"iterations\":%zu,\"cpu_ns\":%llu,\"ns_per_op\":%.6f}\n",name,n,(unsigned long long)elapsed,(double)elapsed/n);}
int main(int argc,char**argv){assert(argc==4);void*h=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(h);
#ifdef V2
 name_arguments=malloc(1024*sizeof(Oop));
 const char *all[15];all[0]=argv[2];all[1]=argv[3];for(size_t i=0;i<13;i++)all[i+2]=words[i];
 IdSeed seeds[15];for(size_t i=0;i<15;i++){seeds[i].bytes=(const unsigned char*)all[i];seeds[i].length=strlen(all[i]);}
 const IdProtocol*(*boot)(const IdSeed*,size_t)=dlsym(h,"birnam_id_bootstrap_v2");assert(boot);id=*boot(seeds,15);root=id.root;
 Oop values[15];for(size_t i=0;i<15;i++){Oop at=id_size(i);values[i]=send(id.seeds,id.at,&at,1);}
 path_roots=values[0];path_gc=values[1];for(size_t i=0;i<13;i++)selectors[i]=send(values[i+2],id.intern,0,0);
 Oop roots=load(path_roots,0,0);Oop behavior=send(root,selectors[BASE],0,0);
#else
 const BirnamModule *m=dlsym(h,"birnam_native_module_v1");assert(m);BirnamOutcome initialized=m->initialize(0,0,0);assert(initialized.status==0&&initialized.value.kind==BIRNAM_PROTOCOL);id=*initialized.value.data.protocol;root=id.receiver;
 for(size_t i=0;i<13;i++)selectors[i]=intern(words[i],strlen(words[i]));
 Oop roots=load(argv[2],0,0);Oop prototype=raw(root,selectors[PROTO],0,0).data.object;BirnamOperand p=object(prototype);Oop behavior=raw(root,selectors[BEHAVIOR],&p,1).data.object;
#endif
 Oop cache[13];for(size_t i=0;i<13;i++)cache[i]=retain(roots,selectors[i]);
 const size_t n=40000;Oop*tokens=malloc(n/8*sizeof(Oop));
 uint64_t start=now();for(size_t i=0;i<n;i++){Oop o=alloc(behavior);if(i%8==0)tokens[i/8]=retain(roots,o);}result("allocate-and-retain-eighth",n,now()-start);
 char stable[65];memset(stable,'s',64);stable[64]=0;
#ifdef V2
 Oop name=make_name(stable,64);Oop name_token=retain(roots,name);Oop symbol=send(name,id.intern,0,0);
#else
 Oop symbol=intern(stable,64);
#endif
 Oop symbol_token=retain(roots,symbol);const size_t hits=300000;start=now();for(size_t i=0;i<hits;i++){
#ifdef V2
 assert(send(name,id.intern,0,0)==symbol);
#else
 assert(intern(stable,64)==symbol);
#endif
 }result("intern-existing-64",hits,now()-start);
 const size_t fresh=8192;start=now();for(size_t i=0;i<fresh;i++){char b[65];memset(b,'x',64);b[64]=0;char prefix[17];snprintf(prefix,sizeof(prefix),"unique%08zx",i);memcpy(b,prefix,strlen(prefix));
#ifdef V2
 Oop name=make_name(b,64);send(name,id.intern,0,0);
#else
 intern(b,64);
#endif
 }result("produce-and-intern-new-64",fresh,now()-start);
 for(size_t i=0;i<n/8;i+=2)release(roots,tokens[i]);free(tokens);
#ifdef V2
 Oop gc=load(path_gc,roots,1);
#else
 Oop gc=load(argv[3],roots,1);raw(gc,selectors[ACTIVATE],0,0);BirnamOperand off=word(0);raw(gc,selectors[AUTO],&off,1);
#endif
 size_t before=count_objects();start=now();
#ifdef V2
 size_t reclaimed=id_unbox(send(gc,selectors[COLLECT],0,0));
#else
 size_t reclaimed=raw(gc,selectors[COLLECT],0,0).data.word;
#endif
 result("collect-garbage",1,now()-start);size_t after=count_objects();printf("{\"heap_before\":%zu,\"heap_after\":%zu,\"reclaimed\":%zu}\n",before,after,reclaimed);
 assert(reclaimed>n/2);(void)cache;(void)symbol_token;
 return 0;
}
