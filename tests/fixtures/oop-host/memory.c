/* Test-owned graph payloads and lifecycle observers; all foreign work is Id sends. */
#include "../oop-id-v2/id.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static IdProtocol id;static Oop names[31];
enum{AT,ALLOCATE,DELEGATED,COLLECT,PRECISE,INTERN,EXISTING,NEWNAME,IDENTITY,AT_ALLOC,BEHAVIOR,IS_BEHAVIOR,TRACE,FINALIZE,VISIT,ALLOCATIONS,RETAIN,RELEASE,BUDGET,ADD,REMOVE,LOOKUP,SAVED_VISIT,SAVED_REFS,SAVED_RELEASE,REFS,RELEASE_DETACHED,IS_SYMBOL,SIZE,ROOTS_DO,OBJECT};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked)){fprintf(stderr,"memory send failed at %d\n",__LINE__);return checked;}}while(0)
#define EXPECT(v) do{if(!(v)){fprintf(stderr,"memory assertion failed at %d\n",__LINE__);return id.invalid;}}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop set_method(Oop vt,size_t selector,Oop method){Oop a[]={names[selector],method};return send(vt,ADD,a,2);}
/* Context: [GC,Roots,nodes,owner A,owner B,known Name,unknown Name,weak Name,
   fault,image,GC vtable,Id vtable,first visitor,next visitor,refusing visitor,
   counted references]. Owner callbacks receive [context,tag]. */
typedef struct{Oop value,edges[2];size_t identity,edge_count,visits[2];}Node;
static Node nodes[128];static size_t node_count,phase_checks[2],finalized[2],first_visits,later_visits,reference_calls;
static size_t finalized_ids[2],first_id,later_id;
static size_t watched[8],watch_count,live_mask,symbol_count,symbol_bytes;
static Oop first_final,later_final,retired_behavior,replace_on,switch_behavior;static int fail_trace,ignore_visit_failure;
static Oop allocate(Oop vt,size_t bytes){Oop n=id_size(bytes);return send(vt,ALLOCATE,&n,1);}
static Oop new_node(Oop vt){
 EXPECT(node_count<128);Oop value=allocate(vt,sizeof(uintptr_t));CHECK(value);Oop identity=send(id.root,IDENTITY,&value,1);CHECK(identity);
 nodes[node_count++]=(Node){.value=value,.identity=id_unbox(identity)};return value;
}
static Node*node_by_id(size_t identity){for(size_t i=0;i<node_count;i++)if(nodes[i].identity==identity)return nodes+i;return 0;}
static Node*node(Oop value){Oop identity=send(id.root,IDENTITY,&value,1);return id_fault(identity)?0:node_by_id(id_unbox(identity));}
static Oop phase(Oop context,size_t tag){
 EXPECT(send(get(context,0),COLLECT,0,0)==id_size(0));Oop zero=id_size(0);EXPECT(id_fault(send(get(context,2),ALLOCATE,&zero,1)));EXPECT(id_fault(send(get(context,2),DELEGATED,0,0)));
 Oop spelling=id_size('x');EXPECT(id_fault(send(id.root,NEWNAME,&spelling,1)));
 EXPECT(id_fault(id.message(id.root,get(context,6),id.intern,0,0)));CHECK(id.message(id.root,get(context,5),id.intern,0,0));phase_checks[tag]++;return 0;
}
static Oop trace(Oop root,Oop pair,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=2)return id.invalid;Oop context=get(pair,0);size_t tag=id_unbox(get(pair,1));EXPECT(tag<2);Node*entry=node(a[0]);EXPECT(entry);entry->visits[tag]++;CHECK(phase(context,tag));
 if(fail_trace)return id_failure(get(context,8));
 if(ignore_visit_failure){EXPECT(id_fault(send(a[1],VISIT,0,0)));return 0;}
 if(a[0]==replace_on&&switch_behavior){Oop args[]={switch_behavior,get(context,4)};CHECK(send(get(context,0),TRACE,args,2));}
 Oop edges[2]={entry->edges[0],entry->edges[1]};size_t count=entry->edge_count;for(size_t i=0;i<count;i++)CHECK(send(a[1],VISIT,edges+i,1));return 0;
}
static Oop finalize(Oop root,Oop pair,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;Oop context=get(pair,0);size_t tag=id_unbox(get(pair,1));EXPECT(tag<2);CHECK(phase(context,tag));
 Oop address=id_size((uintptr_t)a[0]);EXPECT(send(id.root,AT_ALLOC,&address,1)==0);EXPECT(*(uintptr_t*)a[0]==0xcafe);finalized[tag]++;Oop identity=send(id.root,IDENTITY,a,1);CHECK(identity);finalized_ids[tag]=id_unbox(identity);
 if(a[0]==first_final&&later_final){address=id_size((uintptr_t)later_final);EXPECT(send(id.root,AT_ALLOC,&address,1)==later_final);
  /* Finalization still distinguishes marked owners from pending dead objects. */
  Oop dead_owner[]={later_final,later_final};EXPECT(id_fault(send(get(context,0),FINALIZE,dead_owner,2)));
  Oop args[]={later_final,get(context,4)};CHECK(send(get(context,0),FINALIZE,args,2));EXPECT(send(get(context,7),EXISTING,0,0)==0);EXPECT(send(id.root,IS_BEHAVIOR,&retired_behavior,1)==id_size(0));}
 return 0;
}
static Oop first_visit(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;first_visits++;CHECK(set_method(get(c,10),VISIT,get(c,13)));return send(r,SAVED_VISIT,a,n);}
static Oop next_visit(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;later_visits++;return send(r,SAVED_VISIT,a,n);}
static Oop refuse(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;(void)n;return id_failure(get(c,8));}
static Oop counted_refs(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;reference_calls++;return send(r,SAVED_REFS,a,n);}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=3)return id.invalid;size_t identity=id_unbox(a[2]);for(size_t i=0;i<watch_count;i++)if(identity==watched[i])live_mask|=(size_t)1<<i;
 Oop symbol=send(id.root,IS_SYMBOL,a,1);CHECK(symbol);if(symbol==id_size(1)){Oop bytes=send(a[0],SIZE,0,0);CHECK(bytes);symbol_count++;symbol_bytes+=id_unbox(bytes);}return 0;
}
static Oop inventory(Oop c){live_mask=symbol_count=symbol_bytes=0;Oop visitor=get(c,3);return send(id.root,ALLOCATIONS,&visitor,1);}
static Oop watch(Oop const*v,size_t n){EXPECT(n<=8);watch_count=n;for(size_t i=0;i<n;i++){Oop identity=send(id.root,IDENTITY,v+i,1);CHECK(identity);watched[i]=id_unbox(identity);}return 0;}
static Oop name(const char*s){size_t n=strlen(s);if(n>80)return id.invalid;Oop bytes[80];for(size_t i=0;i<n;i++)bytes[i]=id_size((unsigned char)s[i]);return send(id.root,NEWNAME,bytes,n);}
static Oop run(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;Oop gc=get(c,0),roots=get(c,1),vt=get(c,2),gc_vt=get(c,10),id_vt=get(c,11);
 /* Every reported edge uses current visitor lookup, including mid-walk replacement. */
 Oop original=send(gc_vt,LOOKUP,names+VISIT,1);CHECK(original);CHECK(set_method(gc_vt,SAVED_VISIT,original));CHECK(set_method(gc_vt,VISIT,get(c,12)));CHECK(send(gc,PRECISE,0,0));EXPECT(first_visits==1&&later_visits>1);
 CHECK(set_method(gc_vt,VISIT,get(c,14)));Oop dead=allocate(vt,0);CHECK(dead);CHECK(watch(&dead,1));EXPECT(send(gc,PRECISE,0,0)==id_failure(get(c,8)));CHECK(inventory(c));EXPECT(live_mask==1);
 CHECK(set_method(gc_vt,VISIT,original));EXPECT(id_fault(send(gc,VISIT,&gc,1)));Oop malformed[]={gc,id_size(0),id_size(1)};EXPECT(id_fault(send(gc,ALLOCATIONS,malformed,3)));EXPECT(id_fault(send(gc,ROOTS_DO,0,0)));
 CHECK(send(gc_vt,REMOVE,names+SAVED_VISIT,1));CHECK(send(gc,PRECISE,0,0));CHECK(inventory(c));EXPECT(live_mask==0);
 Oop args[]={vt,get(c,3)};CHECK(send(gc,TRACE,args,2));Oop parent=new_node(vt);CHECK(parent);Oop child=new_node(vt);CHECK(child);
 node(parent)->edges[0]=node(parent)->edges[1]=child;node(parent)->edge_count=2;node(child)->edges[0]=parent;node(child)->edge_count=1;
 Oop parent_pin=send(roots,RETAIN,&parent,1);CHECK(parent_pin);Oop pair[]={parent,child};CHECK(watch(pair,2));CHECK(send(gc,PRECISE,0,0));CHECK(inventory(c));EXPECT(live_mask==3);EXPECT(node(parent)->visits[0]==1&&node(child)->visits[0]==1);
 original=send(id_vt,LOOKUP,names+REFS,1);CHECK(original);CHECK(set_method(id_vt,SAVED_REFS,original));CHECK(set_method(id_vt,REFS,get(c,15)));CHECK(send(gc,PRECISE,0,0));EXPECT(reference_calls>0);CHECK(set_method(id_vt,REFS,original));CHECK(send(id_vt,REMOVE,names+SAVED_REFS,1));
 dead=new_node(vt);CHECK(dead);CHECK(watch(&dead,1));fail_trace=1;EXPECT(id_fault(send(gc,PRECISE,0,0)));CHECK(inventory(c));EXPECT(live_mask==1);fail_trace=0;
 ignore_visit_failure=1;EXPECT(id_fault(send(gc,PRECISE,0,0)));CHECK(inventory(c));EXPECT(live_mask==1);ignore_visit_failure=0;CHECK(send(gc,PRECISE,0,0));CHECK(inventory(c));EXPECT(live_mask==0);
 Oop base=send(id.root,OBJECT,0,0);CHECK(base);Oop left_vt=send(base,DELEGATED,0,0);CHECK(left_vt);Oop left_pin=send(roots,RETAIN,&left_vt,1);CHECK(left_pin);
 args[0]=left_vt;CHECK(send(gc,TRACE,args,2));Oop left=new_node(left_vt);CHECK(left);Oop right=new_node(vt);CHECK(right);
 node(parent)->edges[0]=left;node(parent)->edges[1]=right;replace_on=right;switch_behavior=left_vt;
 for(size_t i=0;i<node_count;i++)nodes[i].visits[0]=nodes[i].visits[1]=0;
 CHECK(send(gc,PRECISE,0,0));EXPECT(node(right)->visits[0]>0&&node(left)->visits[0]==0&&node(left)->visits[1]>0);switch_behavior=0;
 first_final=new_node(vt);CHECK(first_final);*(uintptr_t*)first_final=0xcafe;later_final=new_node(vt);CHECK(later_final);*(uintptr_t*)later_final=0xcafe;first_id=node(first_final)->identity;later_id=node(later_final)->identity;
 Oop symbol=id.message(id.root,get(c,7),id.intern,0,0);CHECK(symbol);retired_behavior=send(base,DELEGATED,0,0);CHECK(retired_behavior);args[0]=retired_behavior;CHECK(send(gc,TRACE,args,2));
 Oop doomed[]={first_final,later_final,symbol,retired_behavior};CHECK(watch(doomed,4));args[0]=first_final;CHECK(send(gc,FINALIZE,args,2));args[0]=later_final;CHECK(send(gc,FINALIZE,args,2));
 CHECK(send(gc,PRECISE,0,0));EXPECT(finalized[0]==1&&finalized[1]==1);EXPECT(finalized_ids[0]==first_id&&finalized_ids[1]==later_id);CHECK(inventory(c));EXPECT(live_mask==0);EXPECT(send(get(c,7),EXISTING,0,0)==0);first_final=later_final=0;
 for(size_t i=0;i<64;i++)CHECK(new_node(vt));CHECK(send(gc,PRECISE,0,0));EXPECT(finalized[0]==1&&finalized[1]==1);
 /* A real native local survives conservative and automatic collection. */
 volatile Oop native_only=new_node(vt);CHECK(native_only);Oop observed=native_only;CHECK(watch(&observed,1));CHECK(send(gc,COLLECT,0,0));CHECK(inventory(c));EXPECT(live_mask==1);
 size_t before=phase_checks[0];Oop one=id_size(1);CHECK(send(gc,BUDGET,&one,1));CHECK(new_node(vt));CHECK(new_node(vt));EXPECT(phase_checks[0]>before);observed=native_only;CHECK(send(id.root,IDENTITY,&observed,1));
 Oop budget=id_size(1<<20);CHECK(send(gc,BUDGET,&budget,1));Oop graph[]={parent,left,right};CHECK(watch(graph,3));CHECK(send(roots,RELEASE,&parent_pin,1));CHECK(send(gc,PRECISE,0,0));CHECK(inventory(c));EXPECT(live_mask==0);EXPECT(phase_checks[0]>0&&phase_checks[1]>0);
 CHECK(send(roots,RELEASE,&left_pin,1));CHECK(send(gc,PRECISE,0,0));watch_count=0;CHECK(inventory(c));size_t baseline_count=symbol_count,baseline_bytes=symbol_bytes;
 for(size_t batch=0;batch<3;batch++){
  Oop*pins=malloc(5000*sizeof(Oop));EXPECT(pins);
  for(size_t i=0;i<5000;i++){char text[64];snprintf(text,sizeof(text),"churn-%zu-%zu",batch,i);Oop literal=name(text);CHECK(literal);Oop value=id.message(id.root,literal,id.intern,0,0);CHECK(value);pins[i]=send(roots,RETAIN,&value,1);CHECK(pins[i]);}
  CHECK(send(gc,PRECISE,0,0));CHECK(inventory(c));EXPECT(symbol_count>=baseline_count+5000&&symbol_bytes>baseline_bytes);
  for(size_t i=0;i<5000;i++)CHECK(send(roots,RELEASE,pins+i,1));free(pins);CHECK(send(gc,PRECISE,0,0));CHECK(inventory(c));EXPECT(symbol_count==baseline_count&&symbol_bytes==baseline_bytes);
 }
 return id_size(1);
}
static Oop release_failure(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;(void)a;if(n)return id.invalid;Oop gc=get(c,0),id_vt=get(c,11);Oop original=send(id_vt,LOOKUP,names+RELEASE_DETACHED,1);CHECK(original);CHECK(set_method(id_vt,SAVED_RELEASE,original));CHECK(send(gc,PRECISE,0,0));
 Oop dead=allocate(get(c,2),sizeof(uintptr_t));CHECK(dead);*(uintptr_t*)dead=0xabc123;Oop identity=send(id.root,IDENTITY,&dead,1);CHECK(identity);CHECK(set_method(id_vt,RELEASE_DETACHED,get(c,14)));
 EXPECT(send(gc,PRECISE,0,0)==id_failure(get(c,8)));Oop address=id_size((uintptr_t)dead);EXPECT(send(id.root,AT_ALLOC,&address,1)==0);EXPECT(*(uintptr_t*)dead==0xabc123);EXPECT(id_fault(send(gc,PRECISE,0,0)));
 CHECK(send(id.root,SAVED_RELEASE,&identity,1));EXPECT(id_fault(send(id.root,SAVED_RELEASE,&identity,1)));return id_size(1);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(count!=39||n)return p->invalid;id=*p;for(size_t i=0;i<31;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("allocate"),LIT("delegated"),LIT("collect"),LIT("collectPrecisely"),LIT("intern"),LIT("existingSymbol"),LIT("nameWithBytes"),LIT("identityOf:"),LIT("allocationAt:"),LIT("behaviorOf:"),LIT("isBehavior:"),LIT("trace:with:"),LIT("finalize:with:"),LIT("visit:"),LIT("allocationsDo:"),LIT("retain:"),LIT("release:"),LIT("budget:"),LIT("addMethod"),LIT("removeMethod:"),LIT("lookup"),LIT("memorySavedVisit:"),LIT("memorySavedReferences:do:"),LIT("memorySavedRelease:"),LIT("referencesOf:do:"),LIT("releaseDetachedAllocation:"),LIT("isSymbol:"),LIT("sizeWord"),LIT("rootsDo:"),LIT("objectBehavior"),LIT("trace"),LIT("finalize"),LIT("firstVisit"),LIT("nextVisit"),LIT("refuse"),LIT("countedReferences"),LIT("allocation:bytes:identity:"),LIT("run")};
static const IdMethodSpec methods[]={{31,trace},{32,finalize},{33,first_visit},{34,next_visit},{35,refuse},{36,counted_refs},{37,allocation},{38,run},{24,release_failure}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,39,methods,9,initialize};
