#include "../oop-id-v2/id.h"
static IdProtocol id;
static size_t calls;
static Oop names[12];
static Oop send(Oop r, size_t selector, const Oop *a, size_t n) { return id.message(id.root,r,names[selector],a,n); }
static Oop field(Oop c,size_t i) { Oop index=id_size(i); return send(c,4,&index,1); }
#define CHECK(v) do { if(id_fault(v)) return v; } while(0)
static Oop deny(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)context; (void)receiver; (void)args; (void)count;
    calls++;
    return id.invalid;
}
static Oop count(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)context; (void)receiver; (void)args;
    return count ? id.invalid : id_size(calls);
}
static Oop packet_count(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)context; (void)receiver; (void)args;
    return id_size(count);
}
/* Codegen must retain its own snapshot after this preparation call returns.
   Make the caller's byte object's future slicing fail, then collect precisely. */
static Oop stage_and_change(Oop root,Oop context,Oop receiver,const Oop *args,size_t count) {
    (void)root;
    if(count!=4) return id.invalid;
    Oop original=field(context,0); CHECK(original);
    Oop gc=field(context,1); CHECK(gc);
    Oop denial=field(context,2); CHECK(denial);
    Oop arguments=send(id.root,6,args,count); CHECK(arguments);
    Oop invocation[]={receiver,arguments};
    Oop result=send(original,7,invocation,2); CHECK(result);
    Oop keep[]={id_size(3),result}; Oop held=send(context,5,keep,2); CHECK(held);
    Oop behavior=send(id.root,8,args,1); CHECK(behavior);
    Oop binding[]={names[10],denial}; Oop installed=send(behavior,9,binding,2); CHECK(installed);
    Oop collected=send(gc,11,0,0); CHECK(collected);
    return field(context,3);
}
static Oop initialize(const IdProtocol *protocol, Oop image, const Oop *literals,
                      size_t count, const Oop *args, size_t argc) {
    (void)literals; (void)count; (void)args;
    if (argc || count != 12) return protocol->invalid;
    id = *protocol;
    for(size_t i=0;i<count;i++) names[i]=literals[i];
    return image;
}
static const IdLiteral literals[] = {LIT("deny"), LIT("calls"), LIT("packetCount"), LIT("stageAndChange"), LIT("at:"), LIT("at:put:"), LIT("sequence"), LIT("invokeOn:arguments:"), LIT("ensurePrivateVtableFor:"), LIT("addMethod"), LIT("sliceFromWord:toWord:"), LIT("collectPrecisely")};
static const IdMethodSpec methods[] = {{0, deny}, {1, count}, {2, packet_count}, {3, stage_and_change}};
const IdModule birnam_native_module_v2 = {
    2, sizeof(IdModule), literals, 12, methods, 4, initialize
};
