#include "id.h"
#include <assert.h>
static IdContinuationProtocol api;
static IdProtocol id;
enum {SEND,ENTRY,ANSWER,DRIVE,PAUSE,SEQ,AT,PUT,COLLECT,WITH,REMOVE,ADD,LOOKUP,IDENTITY,
 ECHO,RECORD,WAIT,TAIL,FORWARD,LOCAL,SELECTED,AFTER,HELPER,DATA,ACTIVE,STATS,RESUME,SYNC,SAVE,CONTROL,INVOKE,CAPTURE,CAPREG,CONTROL_SITE,WORDOF,LOW32,BOUNDARY_SITE,EXIT_RETURN,BOUNDARY_CALL,BOUNDARY_EXIT,READ_SLOT,DELIVERY,ACK,SYNC_GC,NAMES};
static const IdLiteral literals[]={LIT("prepareSend"),LIT("prepareEntry"),LIT("answerTo:with:"),LIT("drive:budget:"),LIT("pause:"),LIT("sequence"),LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("withContext:"),LIT("removeMethod:"),LIT("addMethod"),LIT("lookup"),LIT("identityOf:"),LIT("echo"),LIT("record"),LIT("wait"),LIT("tail"),LIT("forward"),LIT("localLookup"),LIT("selected"),LIT("after"),LIT("helper"),LIT("data"),LIT("active"),LIT("stats"),LIT("resumeLookup"),LIT("syncLeaf"),LIT("saveEcho"),LIT("testControl"),LIT("invokeOn:arguments:"),LIT("captureFactory"),LIT("captureRegistration"),LIT("controlSite"),LIT("wordOf:"),LIT("low32"),LIT("boundarySite"),LIT("exitReturn"),LIT("boundaryCallSite:"),LIT("exitBoundaryAt:"),LIT("oopAt:"),LIT("boundaryDeliveryPending"),LIT("acknowledgeBoundaryDelivery"),LIT("syncCollect")};
static Oop names[NAMES];
static size_t ticks,lookup_calls;
static uintptr_t stack_low=UINTPTR_MAX,stack_high;
static Oop send(Oop r,size_t sel,const Oop*a,size_t n){return id.message(id.root,r,names[sel],a,n);}
static Oop ctl(size_t op,const Oop*a,size_t n){return api.control(id.root,names[op],a,n);}
static Oop get(Oop c,size_t i){Oop v=id_size(i);return send(c,AT,&v,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop answer(Oop k,Oop value){Oop a[]={k,value};return ctl(ANSWER,a,2);}
#define CHECK(v) do{Oop result_=(v);if(id_fault(result_))return result_;}while(0)
static Oop echo(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=2)return id.invalid;return answer(a[0],a[1]);}
/* record context = [count, value]. */
static Oop record(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=2)return id.invalid;
 CHECK(put(c,1,a[1]));CHECK(put(c,0,id_size(id_unbox(get(c,0))+1)));return 0;}
/* wait context = [echo template, GC, saved incoming continuation]. */
static Oop wait_method(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=2)return id.invalid;
 CHECK(put(c,2,a[0]));Oop args[]={get(c,0),r,a[0],a[1]};Oop next=ctl(ENTRY,args,4);CHECK(next);
 Oop paused=ctl(PAUSE,&next,1);if(id_fault(paused))return paused;
 /* The pending work is now an Id root, before native code has returned. */
 CHECK(send(get(c,1),COLLECT,0,0));return 0;}
/* tail context = [selector, GC, answer value]. */
static Oop tail(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=2)return id.invalid;
 volatile size_t native_slot=n;uintptr_t here=(uintptr_t)&native_slot;
 if(here<stack_low)stack_low=here;if(here>stack_high)stack_high=here;
 size_t left=id_unbox(a[1]);ticks++;
 if(ticks%113==0){CHECK(send(get(c,1),COLLECT,0,0));}
 if(left==0)return answer(a[0],get(c,2));
 Oop args[]={r,get(c,0),a[0],id_size(left-1)};return ctl(SEND,args,4);}
/* No messages are needed to construct a lookup return, even at the root. */
static Oop forward(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=2)return id.invalid;lookup_calls++;
 Oop args[]={c,r,a[0],a[1]};return ctl(ENTRY,args,4);}
/* local context = [original lookup, GC, watched selector, stats, selected template, receiver, helper selector, resume template]. */
static Oop local_lookup(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=2)return id.invalid;
 Oop k=a[0],selector=a[1];
 if(selector==get(c,2)){
  Oop stats=get(c,3);CHECK(put(stats,0,id_size(id_unbox(get(stats,0))+1)));
  Oop values[]={k,stats,get(c,1)};Oop context=send(id.root,SEQ,values,3);CHECK(context);
  k=send(get(c,4),WITH,&context,1);CHECK(k);
  Oop saved[]={get(c,0),r,k,selector};Oop resume_context=send(id.root,SEQ,saved,4);CHECK(resume_context);
  Oop resume=send(get(c,7),WITH,&resume_context,1);CHECK(resume);
  Oop call[]={get(c,5),get(c,6),resume,id_size(7)};return ctl(SEND,call,4);
 }
 Oop args[]={get(c,0),r,k,selector};return ctl(ENTRY,args,4);}
static Oop resume_lookup(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n!=2)return id.invalid;
 Oop call[]={get(c,0),get(c,1),get(c,2),get(c,3)};return ctl(ENTRY,call,4);}
static Oop helper(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=2)return id.invalid;
 Oop stats=get(c,0);CHECK(put(stats,3,id_size(id_unbox(get(stats,3))+1)));CHECK(send(get(c,1),COLLECT,0,0));return answer(a[0],a[1]);}
/* The selected Method must survive replacement and GC while this answer parks. */
static Oop selected(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=2)return id.invalid;
 Oop k=get(c,0);CHECK(put(get(c,1),1,id_size(1)));CHECK(put(get(c,1),2,k));
 Oop next=answer(k,a[1]);CHECK(next);Oop paused=ctl(PAUSE,&next,1);if(id_fault(paused))return paused;
 CHECK(send(get(c,2),COLLECT,0,0));return 0;}
/* context = [GC, payload], reachable only through the captured Method. */
static Oop context_value(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;
 CHECK(send(get(c,0),COLLECT,0,0));return answer(a[0],get(c,1));}
static Oop data(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;return answer(a[0],a[0]);}
static Oop active(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;
 Oop args[]={0,id_size(1)};Oop rejected=ctl(DRIVE,args,2);if(!id_fault(rejected))return id.invalid;return answer(a[0],id_size(99));}
static Oop stats(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;
 Oop values[]={id_size(ticks),id_size(stack_low==UINTPTR_MAX?0:stack_high-stack_low),id_size(lookup_calls)};
 Oop result=send(id.root,SEQ,values,3);CHECK(result);return answer(a[0],result);}
static Oop save_echo(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=2)return id.invalid;CHECK(put(c,0,a[0]));return answer(a[0],a[1]);}
/* Test controller uses the public native handoff, as Execution will. */
static Oop control_command(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)c;(void)r;if(!n)return id.invalid;return api.control(root,a[0],a+1,n-1);}
static Oop capture_factory(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(!n)return id.invalid;
 CHECK(put(get(c,1),0,a[0]));Oop arguments=send(id.root,SEQ,a,n);CHECK(arguments);
 Oop invoke[]={r,arguments};return send(get(c,0),INVOKE,invoke,2);}
/* Observe the public descriptor/selection arguments without reading Method data. */
static Oop capture_registration(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=4&&n!=5&&n!=6)return id.invalid;
 CHECK(put(get(c,1),0,a[1]));CHECK(put(get(c,1),1,a[2]));
 Oop arguments=send(id.root,SEQ,a,n);CHECK(arguments);Oop invoke[]={r,arguments};return send(get(c,0),INVOKE,invoke,2);}
/* context = [original callSite, mode selector, site index, result slot, pc,
              attempts, refusals, Integer]. First matching call only. */
static Oop control_site(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=1)return id.invalid;
 Oop word=a[0];if(!((uintptr_t)word&1)){word=send(get(c,7),WORDOF,a,1);CHECK(word);if(!((uintptr_t)word&1)){word=send(word,LOW32,0,0);CHECK(word);}}
 Oop arguments=send(id.root,SEQ,a,n);CHECK(arguments);Oop invoke[]={r,arguments};
 Oop original=send(get(c,0),INVOKE,invoke,2);CHECK(original);
 if(word!=get(c,2)||get(c,5)!=id_size(0))return original;
 CHECK(put(c,5,id_size(1)));
 Oop operands[]={get(c,3),get(c,4)};
 Oop bad[]={id_size(1000000),operands[1]};
 Oop rejected=id.message(id.root,r,get(c,1),bad,2);if(!id_fault(rejected))return id.invalid;
 Oop action=id.message(id.root,r,get(c,1),operands,2);
 if(id_fault(action)){CHECK(put(c,6,id_size(id_unbox(get(c,6))+1)));return original;}
 return action;}
/* context = [original callSite, calls to skip, boundary count]. */
static Oop boundary_site(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=1)return id.invalid;
 Oop skip=get(c,1);
 if(skip==id_size(0)&&get(c,2)==id_size(0)){CHECK(put(c,2,id_size(1)));return send(r,BOUNDARY_CALL,a,n);}
 if(skip!=id_size(0))CHECK(put(c,1,id_size(id_unbox(skip)-1)));
 Oop arguments=send(id.root,SEQ,a,n);CHECK(arguments);Oop invoke[]={r,arguments};return send(get(c,0),INVOKE,invoke,2);}
/* context = [original returnAt, Integer, expected word, attempts, refusals,
              true, acknowledgements]. Select the leaf's returned value through
   public slot and Integer operations; never inspect frame/action payloads. */
static Oop exit_return(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;if(n!=1)return id.invalid;
 Oop pending=send(r,DELIVERY,0,0);CHECK(pending);
 if(pending==get(c,5)){CHECK(send(r,ACK,0,0));CHECK(put(c,6,id_size(id_unbox(get(c,6))+1)));}
 Oop value=send(r,READ_SLOT,a,n);CHECK(value);Oop word=send(get(c,1),WORDOF,&value,1);
 if(!id_fault(word)&&!((uintptr_t)word&1))word=send(word,LOW32,0,0);
 if(word==get(c,2)&&get(c,3)==id_size(0)){
  CHECK(put(c,3,id_size(1)));Oop action=send(r,BOUNDARY_EXIT,a,n);
  if(!id_fault(action))return action;CHECK(put(c,4,id_size(1)));
 }
 Oop arguments=send(id.root,SEQ,a,n);CHECK(arguments);Oop invoke[]={r,arguments};return send(get(c,0),INVOKE,invoke,2);}
static Oop sync_collect(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;CHECK(send(c,COLLECT,0,0));return a[0];}
static Oop sync_leaf(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;return n==1?a[0]:id.invalid;}
static const IdContinuationMethodSpec methods[]={{ECHO,echo,1},{RECORD,record,1},{WAIT,wait_method,1},{TAIL,tail,1},{FORWARD,forward,1},{LOCAL,local_lookup,1},{SELECTED,selected,1},{DATA,data,1},{AFTER,context_value,1},{ACTIVE,active,1},{STATS,stats,1},{RESUME,resume_lookup,1},{HELPER,helper,1},{SYNC,sync_leaf,0},{SAVE,save_echo,1},{CONTROL,control_command,0},{CAPTURE,capture_factory,0},{CAPREG,capture_registration,0},{CONTROL_SITE,control_site,0},{BOUNDARY_SITE,boundary_site,0},{EXIT_RETURN,exit_return,0},{SYNC_GC,sync_collect,0}};
static Oop initialize(const IdContinuationProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){
 if(p->common.version!=3||p->common.byte_size!=sizeof(*p)||!p->control||n!=NAMES||count)return p->common.invalid;
 api=*p;id=p->common;for(size_t i=0;i<n;i++)names[i]=s[i];return image;
}
const IdContinuationModule birnam_native_module_v3={3,sizeof(IdContinuationModule),literals,NAMES,methods,sizeof(methods)/sizeof(methods[0]),initialize};
