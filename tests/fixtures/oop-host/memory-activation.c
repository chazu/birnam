/* Activate the actual collector after allocating opaque payloads in bare Id. */
#include "../oop-id-v2/id.h"
#include <stdio.h>
static IdProtocol id;static Oop names[24];static size_t expected,watched[3],mask,construction_failures;
enum{OBJECT,DELEGATED,ALLOCATE,IDENTITY,RETAIN,RELEASE,SEQUENCE,ACTIVATE,CHARGED,COLLECT,METHOD,ADD,TRACE,ALLOCATIONS,PARENT,REFERENCES,ALLOCATION,PRECISE,CONTEXT,SAVED_CONTEXT,METHOD_BEHAVIOR,LOOKUP,FAIL_CONTEXT,REMOVE};
#define CHECK(v) do{Oop checked=(v);if(id_fault(checked)){fprintf(stderr,"activation send failed at %d\n",__LINE__);return checked;}}while(0)
#define EXPECT(v) do{if(!(v)){fprintf(stderr,"activation assertion failed at %d\n",__LINE__);return id.invalid;}}while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop allocate(Oop vt,size_t bytes){Oop size=id_size(bytes);return send(vt,ALLOCATE,&size,1);}
static Oop no_references(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;return n==2?0:id.invalid;}
static Oop inventory(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;(void)r;if(n!=3||!((uintptr_t)a[1]&1)||!((uintptr_t)a[2]&1))return id.invalid;
 expected+=(id_unbox(a[1])+15)&~(size_t)7;
 for(size_t i=0;i<3;i++)if(id_unbox(a[2])==watched[i])mask|=(size_t)1<<i;return 0;
}
static Oop list(Oop visitor){expected=mask=0;return send(id.root,ALLOCATIONS,&visitor,1);}
static Oop fail_after_context(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)c;if(n!=1)return id.invalid;CHECK(send(r,SAVED_CONTEXT,a,n));construction_failures++;return id.invalid;
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){
 if(n!=2||count!=24)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];Oop roots=a[0];
 Oop base=send(id.root,OBJECT,0,0);CHECK(base);Oop nodes=send(base,DELEGATED,0,0);CHECK(nodes);
 Oop before=allocate(nodes,8);CHECK(before);Oop identity=send(id.root,IDENTITY,&before,1);CHECK(identity);watched[0]=id_unbox(identity);
 Oop token=send(roots,RETAIN,&before,1);CHECK(token);
 Oop args=send(id.root,SEQUENCE,&roots,1);CHECK(args);Oop gc=id.message(id.root,a[1],id.load,&args,1);CHECK(gc);Oop precise=id_size(1);CHECK(send(gc,PRECISE,&precise,1));
 EXPECT(id_fault(send(gc,ACTIVATE,0,0)));
 EXPECT(send(gc,CHARGED,0,0)==id_size(0));
 EXPECT(id_fault(send(gc,COLLECT,0,0)));
 EXPECT(send(id.root,IDENTITY,&before,1)==identity);
 /* Failure in a current constructor also leaves the prior accounting intact. */
 Oop methods_vt=send(id.root,METHOD_BEHAVIOR,0,0);CHECK(methods_vt);Oop old_context=send(methods_vt,LOOKUP,names+CONTEXT,1);CHECK(old_context);
 Oop alias[]={names[SAVED_CONTEXT],old_context};CHECK(send(methods_vt,ADD,alias,2));
 Oop failing=send(image,METHOD,names+FAIL_CONTEXT,1);CHECK(failing);Oop replacement[]={names[CONTEXT],failing};CHECK(send(methods_vt,ADD,replacement,2));
 EXPECT(id_fault(send(gc,ACTIVATE,0,0)));EXPECT(construction_failures==1);EXPECT(send(gc,CHARGED,0,0)==id_size(0));EXPECT(id_fault(send(gc,COLLECT,0,0)));
 replacement[1]=old_context;CHECK(send(methods_vt,ADD,replacement,2));CHECK(send(methods_vt,REMOVE,names+SAVED_CONTEXT,1));
 Oop owner_vt=send(base,DELEGATED,0,0);CHECK(owner_vt);
 for(size_t i=REFERENCES;i<=ALLOCATION;i++){Oop method=send(image,METHOD,names+i,1);CHECK(method);Oop binding[]={names[i],method};CHECK(send(owner_vt,ADD,binding,2));}
 Oop owner=allocate(owner_vt,0);CHECK(owner);Oop owner_pin=send(roots,RETAIN,&owner,1);CHECK(owner_pin);
 Oop trace[]={nodes,owner};CHECK(send(gc,TRACE,trace,2));CHECK(send(gc,ACTIVATE,0,0));CHECK(list(owner));
 EXPECT(send(gc,CHARGED,0,0)==id_size(expected));CHECK(send(gc,COLLECT,0,0));EXPECT(send(gc,CHARGED,0,0)==id_size(0));EXPECT(send(id.root,IDENTITY,&before,1)==identity);
 Oop child=send(nodes,DELEGATED,0,0);CHECK(child);Oop inherited=allocate(child,8);CHECK(inherited);
 identity=send(id.root,IDENTITY,&inherited,1);CHECK(identity);watched[1]=id_unbox(identity);Oop child_pin=send(roots,RETAIN,&inherited,1);CHECK(child_pin);
 CHECK(send(gc,COLLECT,0,0));CHECK(list(owner));EXPECT((mask&3)==3);
 Oop parent[]={child,0};CHECK(send(id.root,PARENT,parent,2));EXPECT(id_fault(send(gc,COLLECT,0,0)));CHECK(list(owner));EXPECT((mask&3)==3);
 parent[1]=child;CHECK(send(id.root,PARENT,parent,2));EXPECT(id_fault(send(gc,COLLECT,0,0)));
 parent[1]=nodes;CHECK(send(id.root,PARENT,parent,2));CHECK(send(gc,COLLECT,0,0));CHECK(send(roots,RELEASE,&child_pin,1));
 Oop uncovered_vt=send(base,DELEGATED,0,0);CHECK(uncovered_vt);Oop uncovered=allocate(uncovered_vt,8);CHECK(uncovered);
 identity=send(id.root,IDENTITY,&uncovered,1);CHECK(identity);watched[2]=id_unbox(identity);Oop uncovered_pin=send(roots,RETAIN,&uncovered,1);CHECK(uncovered_pin);
 EXPECT(id_fault(send(gc,COLLECT,0,0)));CHECK(list(owner));EXPECT((mask&5)==5);
 CHECK(send(roots,RELEASE,&uncovered_pin,1));CHECK(send(roots,RELEASE,&token,1));CHECK(send(gc,COLLECT,0,0));CHECK(list(owner));EXPECT(mask==0);
 CHECK(send(roots,RELEASE,&owner_pin,1));return id_size(1);
}
static const IdLiteral literals[]={LIT("objectBehavior"),LIT("delegated"),LIT("allocate"),LIT("identityOf:"),LIT("retain:"),LIT("release:"),LIT("sequence"),LIT("activate"),LIT("allocationBytes"),LIT("collectPrecisely"),LIT("methodAt:"),LIT("addMethod"),LIT("trace:with:"),LIT("allocationsDo:"),LIT("parentOf:put:"),LIT("referencesOf:do:"),LIT("allocation:bytes:identity:"),LIT("precisePayloads:"),LIT("withContext:"),LIT("activationOriginalContext:"),LIT("methodBehavior"),LIT("lookup"),LIT("failAfterContext"),LIT("removeMethod:")};
static const IdMethodSpec methods[]={{REFERENCES,no_references},{ALLOCATION,inventory},{FAIL_CONTEXT,fail_after_context}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,24,methods,3,initialize};
