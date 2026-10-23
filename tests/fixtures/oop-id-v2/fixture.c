#include "id.h"
#include <string.h>
/* This module owns Node payloads. Its state is an Id Sequence manipulated only
   through messages. No foreign payload or Id registry is inspected. */
typedef struct Node {Oop child;Oop size;} Node;
static IdProtocol id;
enum {OBJECT,DELEGATED,ALLOCATE,METHOD,CONTEXT,ADD,BEHAVIOR,SEQUENCE,AT,PUT,REGISTER,RETAIN,RELEASE,TRACE,FINALIZER,COLLECT,REFERENCES,VISIT,FINALIZE,EXERCISE,COUNT,DYNAMIC,SETFAIL,OVERRIDE,LOOKUP,REMOVE,IDENTITY,NEWNAME,LITERAL,INVOKE,FAULTKIND,DETAIL,FAULTMAKE,HELLO,REASSOC,CONTEXT_RUN,EXTENT,NEWFINAL,LOOKUPOVERRIDE,DNU,MISSING,BADREFS,BADALLOC,ALLOCATION,COLLECTARGS,SNAPSHOTARG,APPLYVALUE,APPLY,ARGPROXY,NAMEFACTORY,METHODFACTORY,NSBIND,PREPAREOWNER,NODECHILD,OBSERVE,STOREROOT,STOREEDGE,STORECOMPLETE,VALUEPROBE,ECHONAME,RETIRE,CHARGEGC,GLOBALLOOKUP,SETGLOBALLOOKUP,GLOBALLOOKUPCOUNT,ERRORVALUE,RETHROW,MODELPUT,MODELBOXED,CLONEALLOC,CONVERTINDEX,ROOTSDO,NUMBERIDENTITY,EXTERNALCHARGE,NUMERICWORD,VECTORNEW,VECTORADD,INTEGERINPUT,ERRORFACTORY,NUMERICALLOCATE,NUMERICDECLINE,NUMERICCOMMIT,LOOPVALUE,SUMVALUES,VECTORKEY,PLUS,CLONE,REVERSED,COLLISIONHASH,PUBLISHERRORSTRING,ERRORNAME,ABSENTVALUE,RENDEROUTPUT,PRINTSYSTEM,SYSTEMNAME,NAMES};
static const IdLiteral literals[]={LIT("objectBehavior"),LIT("delegated"),LIT("allocate"),LIT("methodAt:"),LIT("withContext:"),LIT("addMethod"),LIT("behaviorOf:"),LIT("sequence"),LIT("at:"),LIT("at:put:"),LIT("register:"),LIT("retain:"),LIT("release:"),LIT("trace:with:"),LIT("finalize:with:"),LIT("collectPrecisely"),LIT("referencesOf:do:"),LIT("visit:"),LIT("finalize:"),LIT("exercise:"),LIT("finalized"),LIT("dynamicName:"),LIT("failTracing:"),LIT("replacement"),LIT("lookup"),LIT("removeMethod:"),LIT("identityOf:"),LIT("nameWithBytes"),LIT("literalAt:"),LIT("invokeOn:arguments:"),LIT("faultKind"),LIT("detail"),LIT("fault:detail:"),LIT("fixture-hello"),LIT("reassociateBehaviorOf:with:"),LIT("contextRun"),LIT("allocationSizeOf:"),LIT("newFinalizable"),LIT("lookupOverride"),LIT("doesNotUnderstand:"),LIT("missingProbe"),LIT("badCoreReferences"),LIT("badAllocations"),LIT("allocation:bytes:identity:"),LIT("collectArguments"),LIT("snapshotArgument:"),LIT("applyValue"),LIT("apply"),LIT("argumentProxy"),LIT("nameFactory"),LIT("methodFactory"),LIT("binding:value:"),LIT("prepareOwner:"),LIT("nodeWithChild:"),LIT("observe:"),LIT("root:"),LIT("edgeFrom:to:"),LIT("complete:"),LIT("valueProbe"),LIT("echoName"),LIT("retire:"),LIT("chargeAfterGC"),LIT("globalLookup"),LIT("setGlobalLookup:"),LIT("globalLookupCalls"),LIT("caughtFailure"),LIT("rethrow:"),LIT("slotAt:in:put:"),LIT("slot:in:"),LIT("cloneAllocate"),LIT("convertIndex"),LIT("rootsDo:"),LIT("numberIdentity"),LIT("externalCharge"),LIT("numericWord"),LIT("vectorNew"),LIT("vectorAdd"),LIT("integerInput"),LIT("errorFactory"),LIT("numericAllocate"),LIT("numericDecline"),LIT("numericCommit"),LIT("loopValue"),LIT("sumValues"),LIT("vectorKey"),LIT("+"),LIT("clone"),LIT("reversed"),LIT("collisionHash"),LIT("publishErrorString"),LIT("Error"),LIT("absentValue"),LIT("renderOutput"),LIT("print:"),LIT("System")};
static Oop names[NAMES];
static Oop pending_owner;
static Oop call(Oop r,int name,const Oop *a,size_t n){return id.message(id.root,r,names[name],a,n);}
static Oop zero(Oop r,int n){return call(r,n,0,0);}
static Oop one(Oop r,int n,Oop a){return call(r,n,&a,1);}
static Oop two(Oop r,int n,Oop a,Oop b){Oop args[]={a,b};return call(r,n,args,2);}
static Oop get(Oop s,size_t i){return one(s,AT,id_size(i));}
static Oop put(Oop s,size_t i,Oop o){return two(s,PUT,id_size(i),o);}
/* Actor slots: Roots, GC, Node behavior, finalizer count, trace-fail flag,
   entered Method, context Name, original method, custom-lookup calls. */
#define REQUIRE(v) do {Oop check_=(v); if(id_fault(check_))return check_;}while(0)
static Oop references(Oop root_,Oop context,Oop receiver,const Oop *args,size_t n){
 (void)receiver;if(n!=2)return id.invalid;
 Oop flag=get(context,4);if(id_fault(flag))return flag;
 if(id_unbox(flag)==4){
  Oop replacement=pending_owner;pending_owner=0;
  REQUIRE(two(get(context,1),TRACE,get(context,2),replacement));
  REQUIRE(put(context,4,id_size(0)));
 }
 if(id_unbox(flag)==6 && !get(context,20)){REQUIRE(put(context,20,args[1]));}
 if(id_unbox(flag)==1)return id.invalid;
 if(id_unbox(flag)==3){(void)call(args[1],VISIT,0,0);}
 Oop extent=one(id.root,EXTENT,args[0]);if(id_fault(extent)||id_unbox(extent)!=sizeof(Node))return id.invalid;
 if(id_unbox(flag)==2){Oop base=zero(id.root,OBJECT);REQUIRE(one(base,ALLOCATE,id_size(0)));}
 Node *node=(Node *)args[0];
 REQUIRE(one(id.root,BEHAVIOR,node->child));REQUIRE(one(id.root,BEHAVIOR,node->size));
 REQUIRE(one(args[1],VISIT,node->child));REQUIRE(one(args[1],VISIT,node->size));
 return 0;
}
static Oop finalize(Oop root_,Oop context,Oop receiver,const Oop *args,size_t n){
 (void)receiver;if(n!=1)return id.invalid;
 Oop extent=one(id.root,EXTENT,args[0]);if(id_fault(extent)||id_unbox(extent)!=sizeof(Node))return id.invalid;
 if(id_unbox(get(context,9)))return id.invalid;
 Node *node=(Node *)args[0];if(!( (uintptr_t)node->size&1))return id.invalid;
 if(id_unbox(node->size)!=id_unbox(get(context,3)))return id.invalid; /* creation order */
 REQUIRE(put(context,3,id_size(id_unbox(get(context,3))+1)));
 /* Reentrant collection during finalization must be a harmless no-op. */
 Oop nested=zero(get(context,1),COLLECT);if(id_fault(nested)||id_unbox(nested)!=0)return id.invalid;
 return 0;
}
static Oop replacement(Oop root_,Oop context,Oop receiver,const Oop *args,size_t n){(void)receiver;(void)args;(void)n;return get(context,6);}
static Oop exercise(Oop root_,Oop context,Oop receiver,const Oop *args,size_t n){
 if(n!=1)return id.invalid;
 Oop vt=get(context,2),gc=get(context,1),roots=get(context,0);
 Oop method=one(vt,LOOKUP,names[EXERCISE]);if(id_fault(method))return method;
 /* Remove the currently entered method, leaving its context protected by Id's
    explicit invocation roots. We do not retain the Method in a root registry. */
 REQUIRE(one(vt,REMOVE,names[EXERCISE]));
 Oop child=args[0];
 Node *live=(Node *)one(vt,ALLOCATE,id_size(sizeof(Node)));if(id_fault((Oop)live))return (Oop)live;
 live->child=child;live->size=id_size(64);
 Oop token=one(roots,RETAIN,(Oop)live);if(id_fault(token))return token;
 for(size_t i=0;i<128;i++){
  Node *dead=(Node *)one(vt,ALLOCATE,id_size(sizeof(Node)));if(id_fault((Oop)dead))return(Oop)dead;
  dead->child=0;dead->size=id_size(i);
  REQUIRE(two(gc,FINALIZER,(Oop)dead,context));
 }
 REQUIRE(zero(gc,COLLECT));
 /* receiver and argument were protected without explicit retain calls. */
 REQUIRE(one(id.root,IDENTITY,receiver));REQUIRE(one(id.root,IDENTITY,args[0]));
 if(live->child!=child||id_unbox(live->size)!=64||id_unbox(get(context,3))!=128)return id.invalid;
 Oop symbol=id.message(id.root,child,id.intern,0,0);if(id_fault(symbol))return symbol;
 REQUIRE(two(vt,ADD,names[EXERCISE],method));
 REQUIRE(one(roots,RELEASE,token));
 return symbol;
}
static Oop finalized(Oop root_,Oop c,Oop r,const Oop*a,size_t n){(void)r;(void)a;return n?id.invalid:get(c,3);}
static Oop fail_tracing(Oop root_,Oop c,Oop r,const Oop*a,size_t n){(void)r;return n==1?put(c,4,a[0]):id.invalid;}
static Oop dynamic(Oop root_,Oop c,Oop r,const Oop*a,size_t n){(void)c;(void)r;if(n!=1)return id.invalid;size_t length=id_unbox(a[0]);if(length>1024)return id.invalid;
 Oop bytes[1024];for(size_t i=0;i<length;i++)bytes[i]=id_size('a'+i%26);
 /* Entire native-to-name request uses Oop byte values in one message. */
 return call(id.root,NEWNAME,bytes,length);
}
static Oop context_run(Oop root_,Oop context,Oop receiver,const Oop*a,size_t n){
 (void)a;if(n)return id.invalid;
 Oop vt=one(id.root,BEHAVIOR,receiver);if(id_fault(vt))return vt;
 REQUIRE(one(vt,REMOVE,names[CONTEXT_RUN]));
 REQUIRE(zero(get(context,0),COLLECT));
 Oop name=get(context,1);if(id_fault(name))return name;
 return id.message(id.root,name,id.intern,0,0);
}
static Oop new_finalizable(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;(void)a;if(n)return id.invalid;
 Node *node=(Node*)one(get(c,2),ALLOCATE,id_size(sizeof(Node)));if(id_fault((Oop)node))return(Oop)node;
 node->child=0;node->size=id_size(17);REQUIRE(two(get(c,1),FINALIZER,(Oop)node,c));return(Oop)node;
}
static Oop lookup_override(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=1)return id.invalid;
 REQUIRE(zero(get(c,1),COLLECT)); /* GC while the caller is still looking up its method. */
 REQUIRE(put(c,8,id_size(id_unbox(get(c,8))+1)));
 if(a[0]==names[MISSING])return get(c,10);
 Oop args=call(id.root,SEQUENCE,a,n);if(id_fault(args))return args;
 return two(get(c,7),INVOKE,r,args);
}
static Oop dnu(Oop root_,Oop c,Oop r,const Oop*a,size_t n){(void)root_;(void)c;(void)r;(void)a;return id_size(n);}
static Oop bad_refs(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=2)return id.invalid;
 Oop scratch=get(c,12);REQUIRE(put(scratch,0,a[0]));REQUIRE(put(scratch,1,a[1]));
 Oop result=two(get(c,11),INVOKE,r,scratch);if(id_fault(result))return result;
 (void)call(a[1],VISIT,0,0); /* deliberately swallow a detected invalid emission */
 return result;
}
static Oop bad_allocations(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=1)return id.invalid;
 (void)call(a[0],ALLOCATION,0,0);
 Oop scratch=get(c,12);REQUIRE(put(scratch,0,a[0]));
 return two(get(c,11),INVOKE,r,scratch);
}
static Oop collect_arguments(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;REQUIRE(zero(get(c,1),COLLECT));
 for(size_t i=0;i<n;i++)REQUIRE(one(id.root,IDENTITY,a[i]));
 return id_size(n);
}
static Oop snapshot_argument(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 REQUIRE(put(get(c,12),0,get(c,6)));
 REQUIRE(zero(get(c,1),COLLECT));
 REQUIRE(one(id.root,IDENTITY,a[0]));return a[0];
}
static Oop apply_value(Oop root_,Oop context,Oop receiver,const Oop*args,size_t n){
 (void)root_;(void)args;if(n<1)return id.invalid;
 /* receiver is a reference-storage object, not a native Method record. */
 REQUIRE(zero(get(context,1),COLLECT));
 return get(receiver,0);
}
static Oop argument_proxy(Oop root_,Oop context,Oop receiver,const Oop*args,size_t n){
 (void)root_;(void)receiver;if(n!=2)return id.invalid;
 /* This argument container has no Sequence payload. */
 return two(args[0],APPLY,args[1],get(context,6));
}
static Oop construct_with(Oop context,Oop receiver,const Oop*args,size_t n,size_t saved,size_t counter){
 REQUIRE(put(context,counter,id_size(id_unbox(get(context,counter))+1)));
 REQUIRE(zero(get(context,1),COLLECT));
 Oop arguments=call(id.root,SEQUENCE,args,n);if(id_fault(arguments))return arguments;
 return two(get(context,saved),INVOKE,receiver,arguments);
}
static Oop name_factory(Oop root_,Oop c,Oop r,const Oop*a,size_t n){return construct_with(c,r,a,n,13,15);}
static Oop method_factory(Oop root_,Oop c,Oop r,const Oop*a,size_t n){return construct_with(c,r,a,n,14,16);}
static Oop namespace_binding(Oop root_,Oop context,Oop receiver,const Oop*args,size_t n){
 (void)root_;(void)receiver;if(n!=2)return id.invalid;
 REQUIRE(two(get(context,17),PUT,args[0],0));
 REQUIRE(zero(get(context,1),COLLECT));
 REQUIRE(one(id.root,IDENTITY,args[0]));
 if(args[1] && !((uintptr_t)args[1]&1))REQUIRE(one(id.root,IDENTITY,args[1]));
 REQUIRE(put(context,18,id_size(id_unbox(get(context,18))+1)));
 return 0;
}
static Oop prepare_owner(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 pending_owner=a[0];return put(c,4,id_size(4));
}
static Oop node_child(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 Node *node=(Node*)one(get(c,2),ALLOCATE,id_size(sizeof(Node)));if(id_fault((Oop)node))return(Oop)node;
 node->child=a[0];node->size=id_size(0);return(Oop)node;
}
static Oop storage_allocation(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=3)return id.invalid;
 REQUIRE(one(id.root,IDENTITY,a[0]));
 size_t count=id_unbox(get(c,21));REQUIRE(put(c,21,id_size(count+1)));
 if(!count){
  if(id_unbox(get(c,4))==0 && get(c,20)){
   Oop expired=one(get(c,20),VISIT,id.root);if(!id_fault(expired))return id.invalid;
  }
  Oop nested=one(get(c,19),OBSERVE,c);if(!id_fault(nested))return id.invalid;
  Oop detail=zero(nested,DETAIL);if(id_fault(detail)||id_unbox(detail)!=2)return id.invalid;
  REQUIRE(zero(get(c,1),COLLECT));
 }
 return 0;
}
static Oop storage_edge(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1 && n!=2)return id.invalid;
 for(size_t i=0;i<n;i++)REQUIRE(one(id.root,IDENTITY,a[i]));
 size_t index=n==1?22:23;REQUIRE(put(c,index,id_size(id_unbox(get(c,index))+1)));return 0;
}
static Oop storage_complete(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)c;(void)r;return n==1?a[0]:id.invalid;
}
static Oop value_probe(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;(void)a;if(n)return id.invalid;
 REQUIRE(put(c,24,id_size(id_unbox(get(c,24))+1)));REQUIRE(zero(get(c,1),COLLECT));return get(c,6);
}
static Oop echo_name(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 REQUIRE(zero(get(c,1),COLLECT));return a[0];
}
static Oop retire_objects(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;
 for(size_t i=0;i<n;i++)REQUIRE(one(id.root,IDENTITY,a[i]));
 if(id_unbox(get(c,26)))return id.invalid;
 REQUIRE(put(c,25,id_size(id_unbox(get(c,25))+n)));
 Oop nested=zero(get(c,1),COLLECT);if(id_fault(nested)||id_unbox(nested)!=0)return id.invalid;
 return 0;
}
static Oop charge_after_gc(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 REQUIRE(zero(get(c,1),COLLECT));
 return two(get(c,27),APPLY,id.root,a[0]);
}
static Oop original_global_lookup;
static size_t global_lookup_calls;
static Oop set_global_lookup(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)c;(void)r;if(n!=1)return id.invalid;original_global_lookup=a[0];global_lookup_calls=0;return 0;
}
static Oop global_lookup(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)c;if(n!=1)return id.invalid;global_lookup_calls++;
 return two(original_global_lookup,APPLY,r,a[0]);
}
static Oop global_lookup_count(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)c;(void)r;(void)a;return n?id.invalid:id_size(global_lookup_calls);
}
static Oop error_value(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)c;(void)r;(void)a;return n?id.invalid:id_error_value(id.invalid);
}
static Oop rethrow_value(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)c;(void)r;return n==1?id_failure(a[0]):id.invalid;
}
static Oop clone_allocate(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=1)return id.invalid;
 Oop edit[]={id_size(0),get(c,29),get(c,31)};REQUIRE(call(get(c,28),MODELPUT,edit,3));
 REQUIRE(zero(get(c,1),COLLECT));
 return two(get(c,30),APPLY,r,a[0]);
}
static Oop root_observer(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 if(a[0]==get(c,33))REQUIRE(put(c,34,id_size(id_unbox(get(c,34))+1)));
 return 0;
}
static Oop root_occurrences(Oop c){
 REQUIRE(put(c,34,id_size(0)));REQUIRE(one(get(c,28),ROOTSDO,c));return get(c,34);
}
static Oop convert_index(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 Oop before=root_occurrences(c);if(id_fault(before)||!id_unbox(before))return id.invalid;
 REQUIRE(zero(get(c,1),COLLECT));
 if(!id_unbox(get(c,32))){
  REQUIRE(put(c,32,id_size(1)));REQUIRE(two(get(c,28),MODELBOXED,a[0],get(c,29)));
  REQUIRE(put(c,32,id_size(0)));
  if(root_occurrences(c)!=before)return id.invalid;
 }
 return id_size(0);
}
static Oop number_identity(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)c;(void)r;return n==1?a[0]:id.invalid;
}
static Oop external_charge(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 REQUIRE(zero(get(c,1),COLLECT));return two(get(c,35),APPLY,get(c,1),a[0]);
}

static Oop numeric_word(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=1)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));return a[0]==r?id_size(0):a[0];
}
/* Independent persistent vector fixture: a Sequence of count and values. */
static Oop vector_new(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;(void)a;if(n)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));
 return one(id.root,SEQUENCE,id_size(0));
}
static Oop vector_add(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=1)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));
 size_t count=id_unbox(get(r,0));if(count>=64)return id.invalid;
 Oop values[66];values[0]=id_size(count+1);
 for(size_t i=0;i<count;i++)values[i+1]=get(r,i+1);
 values[count+1]=a[0];return call(id.root,SEQUENCE,values,count+2);
}

static Oop integer_input(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=1)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));return a[0]==r?id_size(0):((uintptr_t)a[0]&1)?a[0]:0;
}
static Oop error_factory(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=2)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));
 Oop behavior=one(id.root,BEHAVIOR,r);if(id_fault(behavior))return behavior;
 Oop error=call(id.root,SEQUENCE,a,n);if(id_fault(error))return error;
 REQUIRE(two(id.root,REASSOC,error,behavior));return error;
}
static Oop numeric_allocate(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=1)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));
 return two(get(c,30),APPLY,r,a[0]);
}

static Oop numeric_decline(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;(void)a;if(n!=3)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));return 0;
}
static Oop numeric_commit(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));return a[0];
}
static Oop loop_value(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 REQUIRE(put(c,36,id_size(id_unbox(get(c,36))+1)));REQUIRE(zero(get(c,1),COLLECT));return a[0];
}

static Oop sum_values(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=2)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));return one(a[0],PLUS,a[1]);
}
static Oop vector_key(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;if(n!=1)return id.invalid;
 REQUIRE(put(c,36,id_size(id_unbox(get(c,36))+1)));REQUIRE(zero(get(c,1),COLLECT));
 Oop vector=get(c,37);if(vector)REQUIRE(zero(vector,REVERSED));
 return zero(a[0],CLONE);
}
static Oop collision_hash(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;(void)a;if(n)return id.invalid;REQUIRE(zero(get(c,1),COLLECT));return get(c,38);
}
static Oop publish_error_string(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;if(n!=1)return id.invalid;
 REQUIRE(two(get(c,17),PUT,names[ERRORNAME],get(c,40)));REQUIRE(zero(get(c,1),COLLECT));
 return two(get(c,39),APPLY,r,a[0]);
}
static Oop absent_value(Oop root_,Oop c,Oop r,const Oop*a,size_t n){(void)root_;(void)c;(void)r;(void)a;(void)n;return 0;}
static Oop render_output(Oop root_,Oop c,Oop r,const Oop*a,size_t n){
 (void)root_;(void)r;(void)a;if(n)return id.invalid;
 Oop system=one(get(c,17),AT,names[SYSTEMNAME]);if(id_fault(system))return system;
 REQUIRE(one(system,PRINTSYSTEM,get(c,6)));REQUIRE(zero(get(c,1),COLLECT));return get(c,6);
}
static const IdMethodSpec methods[]={{REFERENCES,references},{FINALIZE,finalize},{EXERCISE,exercise},{COUNT,finalized},{DYNAMIC,dynamic},{SETFAIL,fail_tracing},{OVERRIDE,replacement},{CONTEXT_RUN,context_run},{NEWFINAL,new_finalizable},{LOOKUPOVERRIDE,lookup_override},{DNU,dnu},{BADREFS,bad_refs},{BADALLOC,bad_allocations},{COLLECTARGS,collect_arguments},{SNAPSHOTARG,snapshot_argument},{APPLYVALUE,apply_value},{ARGPROXY,argument_proxy},{NAMEFACTORY,name_factory},{METHODFACTORY,method_factory},{NSBIND,namespace_binding},{PREPAREOWNER,prepare_owner},{NODECHILD,node_child},{ALLOCATION,storage_allocation},{STOREROOT,storage_edge},{STOREEDGE,storage_edge},{STORECOMPLETE,storage_complete},{VALUEPROBE,value_probe},{ECHONAME,echo_name},{RETIRE,retire_objects},{CHARGEGC,charge_after_gc},{GLOBALLOOKUP,global_lookup},{SETGLOBALLOOKUP,set_global_lookup},{GLOBALLOOKUPCOUNT,global_lookup_count},{ERRORVALUE,error_value},{RETHROW,rethrow_value},{CLONEALLOC,clone_allocate},{CONVERTINDEX,convert_index},{VISIT,root_observer},{NUMBERIDENTITY,number_identity},{EXTERNALCHARGE,external_charge},{NUMERICWORD,numeric_word},{VECTORNEW,vector_new},{VECTORADD,vector_add},{INTEGERINPUT,integer_input},{ERRORFACTORY,error_factory},{NUMERICALLOCATE,numeric_allocate},{NUMERICDECLINE,numeric_decline},{NUMERICCOMMIT,numeric_commit},{LOOPVALUE,loop_value},{SUMVALUES,sum_values},{VECTORKEY,vector_key},{COLLISIONHASH,collision_hash},{PUBLISHERRORSTRING,publish_error_string},{ABSENTVALUE,absent_value},{RENDEROUTPUT,render_output}};
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t ns,const Oop*a,size_t n){
 id=*p;if(ns!=NAMES||n!=2)return id.invalid;for(size_t i=0;i<ns;i++)names[i]=s[i];
 Oop base=zero(id.root,OBJECT);if(id_fault(base))return base;
 Oop vt=zero(base,DELEGATED);if(id_fault(vt))return vt;
 Oop actor_values[]={a[0],a[1],vt,id_size(0),id_size(0),0,0,0,id_size(0),id_size(0),0,0,0,0,0,id_size(0),id_size(0),0,id_size(0),0,0,id_size(0),id_size(0),id_size(0),id_size(0),id_size(0),id_size(0),0,0,0,0,0,id_size(0),0,id_size(0),0,id_size(0),0,id_size(0),0,0};
 Oop actor=call(id.root,SEQUENCE,actor_values,41);if(id_fault(actor))return actor;
 /* A separate behavior on an ordinary opaque actor would need an owner too.
    Use a dedicated behavior inheriting Sequence behavior for these methods. */
 Oop actor_vt=zero(one(id.root,BEHAVIOR,actor),DELEGATED);if(id_fault(actor_vt))return actor_vt;
 REQUIRE(two(id.root,REASSOC,actor,actor_vt));
 for(size_t i=0;i<sizeof(methods)/sizeof(methods[0]);i++){
  Oop m=one(image,METHOD,names[methods[i].name]);if(id_fault(m))return m;
  m=one(m,CONTEXT,actor);if(id_fault(m))return m;
  REQUIRE(two(actor_vt,ADD,names[methods[i].name],m));
  if(methods[i].name==EXERCISE){REQUIRE(two(vt,ADD,names[EXERCISE],m));}
 }
 REQUIRE(two(a[1],TRACE,vt,actor));
 /* Keep a second Name solely through an entered Method's context. */
 Oop name=one(image,LITERAL,id_size(HELLO));if(id_fault(name))return name;REQUIRE(put(actor,6,name));
 return actor;
}
__attribute__((visibility("default")))const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,NAMES,methods,sizeof(methods)/sizeof(methods[0]),initialize};
