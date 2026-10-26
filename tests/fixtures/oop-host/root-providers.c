/* Child references are private to this fixture and reachable only via rootsDo:. */
#include "../oop-id-v2/id.h"
#include <stdlib.h>
#include <string.h>
static IdProtocol id;
static Oop names[9];
static Oop children[64];
static Oop identities[64];
static size_t alive;
static Oop *order;
static size_t visits, capacity;
enum { VISIT, SEQUENCE, ENUMERATE, CHILD, ORDER, RESET, IDENTITY, ALLOCATION, ALIVE };
static Oop enumerate(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)r;
    if(n!=1 || !((uintptr_t)c&1) || id_unbox(c)>=64) return id.invalid;
    if(visits==capacity) {
        size_t next=capacity?capacity*2:128;
        Oop *grown=realloc(order,next*sizeof(Oop));
        if(!grown) return id.invalid;
        order=grown;capacity=next;
    }
    order[visits++]=c;
    Oop child=children[id_unbox(c)];
    return id.message(id.root,a[0],names[VISIT],&child,1);
}
static Oop child(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)c;(void)r;
    if(n!=2 || !((uintptr_t)a[0]&1) || id_unbox(a[0])>=64) return id.invalid;
    size_t index=id_unbox(a[0]);
    if(!identities[index]) {
        Oop identity=id.message(id.root,id.root,names[IDENTITY],a+1,1);
        if(id_fault(identity)) return identity;
        identities[index]=identity;
    }
    children[index]=a[1];return a[1];
}
static Oop recorded(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)c;(void)r;(void)a;
    if(n) return id.invalid;
    size_t count=visits;
    Oop *copy=malloc((count?count:1)*sizeof(Oop));
    if(!copy) return id.invalid;
    if(count) memcpy(copy,order,count*sizeof(Oop));
    visits=0;
    Oop result=id.message(id.root,id.root,names[SEQUENCE],copy,count);
    free(copy);
    return result;
}
static Oop reset(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)c;(void)r;(void)a;
    if(n) return id.invalid;
    visits=0;return id_size(0);
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)c;(void)r;
    if(n!=3) return id.invalid;
    for(size_t i=0;i<64;i++)if(identities[i]==a[2])alive++;
    return 0;
}
static Oop remaining(Oop root,Oop c,Oop r,const Oop *a,size_t n) {
    (void)root;(void)c;(void)r;(void)a;
    return n?id.invalid:id_size(alive);
}
static Oop initialize(const IdProtocol *p,Oop owner,const Oop *l,size_t count,const Oop *a,size_t n) {
    (void)a;
    if(n || count!=9) return p->invalid;
    id=*p;for(size_t i=0;i<count;i++)names[i]=l[i];return owner;
}
static const IdLiteral literals[]={LIT("visit:"),LIT("sequence"),LIT("enumerate"),LIT("child"),LIT("recorded"),LIT("reset"),LIT("identityOf:"),LIT("allocation:bytes:identity:"),LIT("remaining")};
static const IdMethodSpec methods[]={{ENUMERATE,enumerate},{CHILD,child},{ORDER,recorded},{RESET,reset},{ALLOCATION,allocation},{ALIVE,remaining}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,9,methods,6,initialize};
