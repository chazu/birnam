/* Declared Methods; all context and foreign-object access uses Id messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, COLLECT, ADD, IDENTITY, ALLOCATIONS, METHOD_FOR, FROM,
       SIZE, INDEX, NEW_METHOD, CONTEXT, OWNER, ANSWER, SELF, RESULT, FINALIZE,
       CHOOSE, ALLOCATION, PRESENT, ARG_IDS, SEQUENCE, COUNT };
static Oop names[COUNT];
#define CHECK(v) do { Oop checked = (v); if (id_fault(checked)) return checked; } while (0)
static Oop send(Oop r, size_t s, const Oop *a, size_t n) {
    return id.message(id.root, r, names[s], a, n);
}
static Oop get(Oop c, size_t i) { Oop k=id_size(i); return send(c, AT, &k, 1); }
static Oop put(Oop c, size_t i, Oop v) { Oop a[]={id_size(i),v}; return send(c, PUT, a, 2); }
static Oop increment(Oop c, size_t i) { return put(c,i,id_size(id_unbox(get(c,i))+1)); }
static size_t wanted, found, argument_ids[3], argument_count;
static Oop allocation(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)c;(void)r;
    if(n!=3)return id.invalid;
    if(id_unbox(a[2])==wanted)found++;
    return 0;
}
static Oop present(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)c;
    if(n!=1)return id.invalid;
    wanted=id_unbox(a[0]); found=0;
    CHECK(send(id.root, ALLOCATIONS, &r, 1));
    return id_size(found);
}
static Oop owner(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)r;(void)a;(void)n;return c;
}
static Oop self(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)c;(void)a;(void)n;return r;
}
/* [owner,vtable,selector,replacement,replace,GC,calls,finalizer counter]. */
static Oop answer(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)r;
    if(get(c,4)==id_size(1)) {
        CHECK(put(c,4,id_size(0)));
        Oop binding[]={get(c,2),get(c,3)};
        CHECK(send(get(c,1),ADD,binding,2));
    }
    CHECK(send(get(c,5),COLLECT,0,0));
    if(get(get(c,7),0)!=id_size(0))return id.invalid;
    Oop owned=get(c,0);CHECK(send(id.root,IDENTITY,&owned,1));
    for(size_t i=0;i<n;i++)CHECK(send(id.root,IDENTITY,&a[i],1));
    CHECK(increment(c,6));
    return n?a[n-1]:owned;
}
/* Only a numeric identity is held by the finalizer. */
static Oop finalize(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)r;
    if(n!=1)return id.invalid;
    Oop identity=send(id.root,IDENTITY,a,1);CHECK(identity);
    if(identity!=get(c,1))return id.invalid;
    return increment(c,0);
}
/* [GC,Integer,monitor,failure-or-null]. The monitor stores identities, not roots. */
static Oop sequence_size(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)c;(void)r;(void)a;
    if(n)return id.invalid;argument_count=0;return id_size(3);
}
static Oop sequence_at(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)r;
    if(n!=1)return id.invalid;
    CHECK(send(get(c,0),COLLECT,0,0));
    for(size_t i=0;i<argument_count;i++) {
        Oop identity=id_size(argument_ids[i]);
        if(send(get(c,2),PRESENT,&identity,1)!=id_size(1))return id.invalid;
    }
    size_t index=id_unbox(a[0]);
    if(index>=3)return id.invalid;
    Oop failure=get(c,3);
    if(index==2 && failure)return id_failure(failure);
    Oop bits=id_size(100+index),value=send(get(c,1),FROM,&bits,1);CHECK(value);
    Oop identity=send(id.root,IDENTITY,&value,1);CHECK(identity);
    argument_ids[argument_count++]=id_unbox(identity);
    return value;
}
static Oop arg_ids(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)c;(void)r;(void)a;
    if(n)return id.invalid;
    Oop ids[3];for(size_t i=0;i<argument_count;i++)ids[i]=id_size(argument_ids[i]);
    return send(id.root,SEQUENCE,ids,argument_count);
}
/* [selected,nested,selector,mode,GC,invalid selection]. */
static Oop choose(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)r;(void)a;
    if(n!=1)return id.invalid;
    if(get(c,3)==id_size(1))return 0;
    if(get(c,3)==id_size(2))return get(c,5);
    Oop selector=get(c,2);
    CHECK(send(get(c,1),METHOD_FOR,&selector,1));
    CHECK(send(get(c,4),COLLECT,0,0));
    return get(c,0);
}
/* Raw Oop return semantics: absence, immediate Word, or exact failure identity. */
static Oop result(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;(void)r;(void)a;(void)n;
    size_t mode=id_unbox(get(c,0));
    if(mode==0)return 0;
    if(mode==1)return id_size(123);
    return id_failure(get(c,mode-1));
}
static Oop initialize(const IdProtocol *p,Oop image,const Oop *s,size_t count,const Oop *a,size_t n) {
    if(count!=COUNT || n>1)return p->invalid;
    id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];
    if(!n)return image;
    Oop index=id_size(0),method=send(image,NEW_METHOD,&index,1);CHECK(method);
    return send(method,CONTEXT,a,1);
}
static const IdLiteral literals[]={
    LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("addMethod"),
    LIT("identityOf:"),LIT("allocationsDo:"),LIT("methodFor:"),LIT("fromWord:"),
    LIT("sizeWord"),LIT("atWord:"),LIT("newMethodAt:"),LIT("withContext:"),
    LIT("owner"),LIT("answer"),LIT("self"),LIT("result"),LIT("finalize:"),
    LIT("choose"),LIT("allocation:bytes:identity:"),LIT("present"),
    LIT("argumentIdentities"),LIT("sequence")
};
static const IdMethodSpec methods[]={
    {OWNER,owner},{ANSWER,answer},{SELF,self},{RESULT,result},{FINALIZE,finalize},
    {CHOOSE,choose},{ALLOCATION,allocation},{PRESENT,present},{ARG_IDS,arg_ids},
    {SIZE,sequence_size},{INDEX,sequence_at}
};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,11,initialize};
