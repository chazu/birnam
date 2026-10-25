#include "id.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>
static IdProtocol id;static Oop roots,gc,image,controller;
static const char*words[]={"sequence","nameWithBytes","objectBehavior","allocate","delegated","behaviorOf:","reassociateBehaviorOf:with:","methodAt:","withContext:","addMethod","removeMethod:","lookup","at:","at:put:","retain:","release:","activate","automaticCollection:","collectPrecisely","prepareSend","prepareEntry","answerTo:with:","drive:budget:","echo","record","wait","tail","forward","localLookup","selected","data","active","stats","callback","allocationCount","identityOf:","invokeOn:arguments:","faultKind","rejectLookup","after","helper","resumeLookup","syncLeaf","saveEcho","testControl"};
enum{SEQ,NAME,BASE,ALLOC,DELEGATE,BEHAVIOR,REASSOC,METHOD,CONTEXT,ADD,REMOVE,LOOKUP,AT,PUT,RETAIN,RELEASE,ACTIVATE,AUTO,GC,SEND,ENTRY,ANSWER,DRIVE,ECHO,RECORD,WAIT,TAIL,FORWARD,LOCAL,SELECTED,DATA,ACTIVE,STATS,CALLBACK,COUNT,IDENTITY,INVOKE,FAULTK,REJECT,AFTER,HELPER,RESUME,SYNC,SAVE,CONTROL,NAMES};
static Oop names[NAMES];
static Oop raw(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop check(Oop v){if(id_fault(v)){fprintf(stderr,"unexpected failure %p\n",v);abort();}return v;}
static Oop send(Oop r,size_t s,const Oop*a,size_t n){Oop v=raw(r,s,a,n);if(id_fault(v))fprintf(stderr,"failed send %s receiver=%p\n",words[s],r);return check(v);}
static Oop one(Oop r,size_t s,Oop v){return send(r,s,&v,1);}
static Oop get(Oop r,size_t i){return one(r,AT,id_size(i));}
static void put(Oop r,size_t i,Oop v){Oop a[]={id_size(i),v};send(r,PUT,a,2);}
static Oop seq(const Oop*a,size_t n){return send(id.root,SEQ,a,n);}
static Oop token(Oop v){return one(roots,RETAIN,v);}
static Oop keep(Oop v){token(v);return v;}
static void release(Oop t){one(roots,RELEASE,t);}
static Oop load(Oop path,const Oop*a,size_t n){Oop input=seq(a,n);return check(id.message(id.root,path,id.load,&input,1));}
static Oop method_from(Oop module,size_t s,Oop context){Oop m=one(module,METHOD,names[s]);return one(m,CONTEXT,context);}
static Oop method(size_t s,Oop context){return method_from(image,s,context);}
static void bind(Oop vt,size_t s,Oop m){Oop a[]={names[s],m};send(vt,ADD,a,2);}
static Oop control(size_t op,const Oop*a,size_t n){Oop passed[n+1];passed[0]=names[op];for(size_t i=0;i<n;i++)passed[i+1]=a[i];return raw(controller,CONTROL,passed,n+1);}
static Oop prepare(size_t op,const Oop*a,size_t n){return check(control(op,a,n));}
static Oop step(Oop work,size_t budget){Oop a[]={work,id_size(budget)};return control(DRIVE,a,2);}
static void drain(Oop work){Oop t=token(work);size_t turns=0;while(work){
 Oop next=check(step(work,1));Oop nt=next?token(next):0;release(t);work=next;t=nt;
 if(++turns%83==0)send(gc,GC,0,0);assert(turns<200000);
 }printf("DRIVE transitions=%zu\n",turns);}
static Oop recorder(Oop*state){Oop init[]={id_size(0),0};*state=keep(seq(init,2));return keep(method(RECORD,*state));}
static void clear_result(Oop state){put(state,0,id_size(0));put(state,1,0);}
int main(int argc,char**argv){assert(argc==8);void*h=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(h);
 IdSeed seeds[6+NAMES];for(size_t i=0;i<6+NAMES;i++){const char*s=i<6?argv[i+2]:words[i-6];seeds[i]=(IdSeed){(const unsigned char*)s,strlen(s)};}
 const IdProtocol*(*boot)(const IdSeed*,size_t)=dlsym(h,"birnam_id_bootstrap_v2");assert(boot);id=*boot(seeds,6+NAMES);assert(id.version==2&&id.byte_size==sizeof id);
 Oop paths[6];for(size_t i=0;i<6+NAMES;i++){Oop ix=id_size(i);Oop name=check(id.message(id.root,id.seeds,id.at,&ix,1));if(i<6)paths[i]=name;else names[i-6]=check(id.message(id.root,name,id.intern,0,0));}
 roots=load(paths[0],0,0);gc=load(paths[1],&roots,1);send(gc,ACTIVATE,0,0);one(gc,AUTO,id_size(0));
 image=load(paths[2],0,0);Oop native=load(paths[3],0,0);
 Oop noargs=seq(0,0);assert(id_fault(id.message(id.root,paths[4],id.load,&noargs,1)));assert(id_fault(id.message(id.root,paths[5],id.load,&noargs,1)));printf("INCOMPATIBLE_V3_REFUSED\n");
 Oop base=send(id.root,BASE,0,0),vt=keep(send(base,DELEGATE,0,0)),r=keep(one(vt,ALLOC,id_size(0)));
 controller=keep(one(vt,ALLOC,id_size(0)));bind(vt,CONTROL,method(CONTROL,0));
 Oop state,k=recorder(&state);Oop bytes[]={id_size('v'),id_size('a'),id_size('l')};Oop value=send(id.root,NAME,bytes,3);
 Oop echo=method(ECHO,0);bind(vt,ECHO,echo);
 Oop request[]={r,names[ECHO],k,value};Oop work=prepare(SEND,request,4);request[3]=id_size(314); /* copied before native return */
 drain(work);assert(id_unbox(get(state,0))==1&&get(state,1)==value);
 printf("CPS_NATIVE_ECHO_OK\n");
 bind(vt,SYNC,method(SYNC,0));size_t sync_before=id_unbox(send(id.root,COUNT,0,0));
 assert(send(r,SYNC,&value,1)==value);assert(id_unbox(send(id.root,COUNT,0,0))==sync_before);printf("MIXED_NATIVE_CONVENTIONS_OK\n");
 /* Continuation Oop returned as data is not executed or classified. */
 clear_result(state);bind(vt,DATA,method(DATA,0));Oop data_req[]={r,names[DATA],k};drain(prepare(SEND,data_req,3));assert(get(state,1)==k);
 printf("CONTINUATION_AS_DATA_OK\n");
 /* A paused work item is the only owner of its argument after old roots drop. */
 clear_result(state);Oop wait_ctx_values[]={echo,gc,0};Oop wait_ctx=keep(seq(wait_ctx_values,3));bind(vt,WAIT,method(WAIT,wait_ctx));
 value=send(id.root,NAME,bytes,3);Oop wait_req[]={r,names[WAIT],k,value};work=prepare(SEND,wait_req,4);Oop wt=token(work);
 Oop pending=check(step(work,100));assert(pending);Oop pt=token(pending);release(wt);send(gc,GC,0,0);
 drain(pending);release(pt);assert(get(state,1)==value&&id_unbox(get(state,0))==1);
 printf("PAUSED_ROOT_HANDOFF_OK\n");
 /* A native boundary refuses parking and closes its saved answer context. */
 assert(id_fault(raw(r,WAIT,&value,1)));Oop late=get(wait_ctx,2);Oop late_req[]={late,value};work=prepare(ANSWER,late_req,2);assert(id_fault(step(work,100)));
 Oop native_ctx_values[]={r,names[WAIT],value};Oop native_ctx=seq(native_ctx_values,3);bind(vt,CALLBACK,method_from(native,CALLBACK,native_ctx));
 clear_result(state);Oop native_req[]={r,names[CALLBACK],k};drain(prepare(SEND,native_req,3));assert(get(state,1)==id_size(71));
 printf("NATIVE_BARRIER_AND_LATE_REPLY_OK\n");
 clear_result(state);bind(vt,ACTIVE,method(ACTIVE,0));Oop active_req[]={r,names[ACTIVE],k};drain(prepare(SEND,active_req,3));assert(get(state,1)==id_size(99));
 printf("OVERLAPPING_DRIVE_REFUSED\n");
 /* Selected entry retains code/context even when binding and Method vtable change. */
 clear_result(state);echo=method(ECHO,value);bind(vt,ECHO,echo);Oop selected_req[]={echo,r,k,value};work=prepare(ENTRY,selected_req,4);wt=token(work);
 one(vt,REMOVE,names[ECHO]);Oop method_behavior=one(id.root,BEHAVIOR,echo);Oop poison=keep(send(method_behavior,DELEGATE,0,0));Oop poison_meta=keep(send(one(id.root,BEHAVIOR,poison),DELEGATE,0,0));
 bind(poison_meta,LOOKUP,method_from(native,REJECT,0));
 Oop poison_assoc[]={poison,poison_meta};send(id.root,REASSOC,poison_assoc,2);
 Oop reassoc[]={echo,poison};send(id.root,REASSOC,reassoc,2);
 Oop echo_args=seq(&value,1);Oop explicit_invoke[]={r,echo_args};assert(id_fault(raw(echo,INVOKE,explicit_invoke,2)));
 send(gc,GC,0,0);drain(work);release(wt);assert(get(state,1)==value);printf("SELECTED_ENTRY_ROOTS_AND_NO_RELOOKUP_OK\n");
 /* Local metacircular lookup selects, parks, then survives binding replacement. */
 clear_result(state);echo=method(ECHO,0);bind(vt,ECHO,echo);Oop meta=one(id.root,BEHAVIOR,vt);Oop original=one(meta,LOOKUP,names[LOOKUP]);
 Oop stats_values[]={id_size(0),id_size(0),0,id_size(0)};Oop stats=keep(seq(stats_values,4));
 Oop helper_values[]={stats,gc};Oop helper_context=seq(helper_values,2);bind(vt,HELPER,method(HELPER,helper_context));
 Oop local_values[]={original,gc,names[ECHO],stats,method(SELECTED,0),r,names[HELPER],method(RESUME,0)};Oop local_ctx=keep(seq(local_values,8));
 Oop custom_meta=keep(send(meta,DELEGATE,0,0));bind(custom_meta,LOOKUP,method(LOCAL,local_ctx));Oop assoc[]={vt,custom_meta};send(id.root,REASSOC,assoc,2);
 Oop local_req[]={r,names[ECHO],k,value};work=prepare(SEND,local_req,4);wt=token(work);pending=check(step(work,100));assert(pending&&get(stats,1)==id_size(1));pt=token(pending);release(wt);
 bind(vt,ECHO,method(DATA,0));send(gc,GC,0,0);drain(pending);release(pt);assert(get(state,1)==value&&get(stats,0)==id_size(1)&&get(stats,3)==id_size(1));
 Oop duplicate_k=get(stats,2);Oop duplicate_reply[]={duplicate_k,echo};assert(id_fault(step(prepare(ANSWER,duplicate_reply,2),100)));put(stats,2,0);
 assoc[1]=meta;send(id.root,REASSOC,assoc,2);printf("CUSTOM_LOOKUP_PAUSE_REPLACEMENT_OK\n");
 /* Captured context survives GC, then becomes collectible after completion. */
 clear_result(state);Oop context_only=send(id.root,NAME,bytes,3);Oop ctx_values[]={gc,context_only};Oop owned_context=seq(ctx_values,2);
 Oop context_method=method(AFTER,owned_context);bind(vt,AFTER,context_method);
 Oop context_req[]={context_method,r,k};work=prepare(ENTRY,context_req,3);wt=token(work);one(vt,REMOVE,names[AFTER]);
 send(gc,GC,0,0);drain(work);release(wt);assert(get(state,1)==context_only);send(gc,GC,0,0);
 assert(id_fault(raw(id.root,IDENTITY,&owned_context,1)));clear_result(state);send(gc,GC,0,0);
 assert(id_fault(raw(id.root,IDENTITY,&context_only,1)));printf("CAPTURED_CONTEXT_LIFETIME_OK\n");
 /* Keeping a consumed native answer handle does not keep its answer alive. */
 Oop saved_context=keep(seq((Oop[]){0},1));bind(vt,SAVE,method(SAVE,saved_context));
 Oop forgotten=send(id.root,NAME,bytes,3);assert(send(r,SAVE,&forgotten,1)==forgotten);
 send(gc,GC,0,0);assert(id_fault(raw(id.root,IDENTITY,&forgotten,1)));
 Oop stale_reply[]={get(saved_context,0),id_size(5)};assert(id_fault(step(prepare(ANSWER,stale_reply,2),100)));
 printf("CONSUMED_REPLY_RELEASES_ANSWER_OK\n");
 /* Two schedulable work roots, distinct answers, collection between turns. */
 Oop states[2],ks[2],jobs[2],tokens[2],answers[2];
 for(size_t i=0;i<2;i++){ks[i]=recorder(&states[i]);answers[i]=one(vt,ALLOC,id_size(0));Oop req[]={r,names[WAIT],ks[i],answers[i]};jobs[i]=prepare(SEND,req,4);tokens[i]=token(jobs[i]);}
 size_t rounds=0;while(jobs[0]||jobs[1]){for(size_t i=0;i<2;i++){if(!jobs[i])continue;Oop next=check(step(jobs[i],1));Oop nt=next?token(next):0;release(tokens[i]);jobs[i]=next;tokens[i]=nt;send(gc,GC,0,0);}assert(++rounds<100);}
 for(size_t i=0;i<2;i++)assert(get(states[i],0)==id_size(1)&&get(states[i],1)==answers[i]);printf("INTERLEAVED_WORK_GC_OK\n");
 /* Root lookup also uses the public primitive, with no constructor regress. */
 Oop global=method(FORWARD,original);bind(meta,LOOKUP,global);bind(vt,ECHO,method(ECHO,0));assert(raw(r,ECHO,&value,1)==value);
 /* Collection still refuses allocating lookup/reporting before retirement. */
 assert(id_fault(raw(gc,GC,0,0)));assert(!id_fault(raw(id.root,IDENTITY,&value,1)));
 bind(meta,LOOKUP,original);send(gc,GC,0,0);printf("ALLOCATING_LOOKUP_GC_REFUSED_SAFELY\n");printf("ROOT_METACIRCULAR_LOOKUP_OK\n");
 /* Repeated tails through custom lookup with precise GC: no accumulating work. */
 bind(custom_meta,LOOKUP,method(FORWARD,original));assoc[1]=custom_meta;send(id.root,REASSOC,assoc,2);
 clear_result(state);Oop tail_values[]={names[TAIL],gc,value};Oop tail_ctx=seq(tail_values,3);bind(vt,TAIL,method(TAIL,tail_ctx));send(gc,GC,0,0);size_t before=id_unbox(send(id.root,COUNT,0,0));
 Oop tail_req[]={r,names[TAIL],k,id_size(10000)};drain(prepare(SEND,tail_req,4));assert(get(state,1)==value);send(gc,GC,0,0);size_t after=id_unbox(send(id.root,COUNT,0,0));assert(after<=before+8);
 Oop stats_method=method(STATS,0);Oop invoke_args=seq(0,0);Oop inv[]={r,invoke_args};Oop measurements=send(stats_method,INVOKE,inv,2);
 assert(get(measurements,0)==id_size(10001));assert(id_unbox(get(measurements,2))>=10000);assert(id_unbox(get(measurements,1))<4096);
 printf("TAIL_GC_OK custom=true before=%zu after=%zu stack_span=%zu\n",before,after,id_unbox(get(measurements,1)));
 printf("REAL_ID_CONTINUATION_OK\n");return 0;
}
