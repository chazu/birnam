/* Independent C consumer of the production Id loading protocol. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-v1/root_registry_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef MODE
#define MODE 0
#endif

__attribute__((destructor)) static void unloaded(void) {
    const char *path = getenv("BIRNAM_NATIVE_UNLOAD_LOG");
    if (path) {
        FILE *file = fopen(path, "a");
        assert(file);
        fprintf(file, "%d\n", MODE);
        fclose(file);
    }
}

#if MODE <= 1
static size_t cookie;
static BirnamObject construction_roots[4];
static unsigned construction_collections;
static void retain_construction(RootRegistryMarkV1 mark) {
    construction_collections++;
    for (size_t i=0;i<4;i++) if (construction_roots[i]) mark(construction_roots[i]);
}
static BirnamOutcome echo(void *context, BirnamObject receiver,
                          const BirnamOperand *arguments, size_t count) {
    (void)receiver;
    assert(context == &cookie && cookie == 0x83 && count == 1);
    return (BirnamOutcome){ .value = arguments[0] };
}
static BirnamOutcome returns_word(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args;(void)count;
    return (BirnamOutcome){ .value = { .kind = BIRNAM_WORD, .data.word = 83 } };
}
static BirnamOutcome rejects_argument(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args;(void)count;
    return (BirnamOutcome){ .status = BIRNAM_INVALID_REQUEST, .detail = 83 };
}
static BirnamObject intern(const BirnamProtocol *id, BirnamOperand name) {
    BirnamOutcome answer = id->message(id->context, id->receiver, id->intern, &name, 1);
    assert(answer.status == BIRNAM_LOADED && answer.value.kind == BIRNAM_OBJECT);
    return answer.value.data.object;
}
static BirnamObject name(const BirnamProtocol *id, const char *bytes) {
    return intern(id, (BirnamOperand){ .kind = BIRNAM_BYTES,
        .data.bytes = { (const uint8_t *)bytes, strlen(bytes) } });
}
static BirnamOutcome send(const BirnamProtocol *id, BirnamObject receiver, BirnamObject selector,
                          const BirnamOperand *args, size_t count) {
    BirnamOutcome answer = id->message(id->context, receiver, selector, args, count);
    assert(answer.status == BIRNAM_LOADED);
    return answer;
}
struct Traversal {
    const BirnamProtocol *id;
    BirnamObject target, behavior, identity, allocate, created;
    size_t visited, found, expected_identity;
    int grow;
};
static BirnamOutcome visit_object(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    (void)receiver;
    struct Traversal *visit = context;
    assert(count == 1 && args[0].kind == BIRNAM_OBJECT);
    visit->visited++;
    if (args[0].data.object == visit->target) visit->found++;
    return (BirnamOutcome){0};
}
static BirnamOutcome visit_allocation(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    struct Traversal *visit = context;
    assert(receiver == visit->id->receiver && count == 3);
    assert(args[0].kind == BIRNAM_OBJECT && args[1].kind == BIRNAM_WORD && args[2].kind == BIRNAM_WORD);
    assert(args[2].data.word != 0);
    /* Reenter Id while visiting its allocation snapshot. */
    BirnamOutcome actual = send(visit->id, receiver, visit->identity, args, 1);
    assert(actual.value.kind == BIRNAM_WORD && actual.value.data.word == args[2].data.word);
    assert(args[0].data.object != visit->created);
    if (args[0].data.object == visit->target) {
        assert(args[1].data.word == 16 && args[2].data.word == visit->expected_identity);
        visit->found++;
    }
    if (visit->grow && !visit->created) {
        BirnamOperand size = { .kind = BIRNAM_WORD, .data.word = 3 };
        actual = send(visit->id, visit->behavior, visit->allocate, &size, 1);
        assert(actual.value.kind == BIRNAM_OBJECT);
        visit->created = actual.value.data.object;
    }
    visit->visited++;
    return (BirnamOutcome){0};
}
static BirnamOutcome stop_visit(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    (void)receiver;(void)args;(void)count;
    (*(size_t *)context)++;
    return (BirnamOutcome){ .status = BIRNAM_INVALID_REQUEST, .detail = 83,
        .value = { .kind = BIRNAM_WORD, .data.word = 84 } };
}
static BirnamObject inspect_storage(const BirnamProtocol *id, BirnamObject object, BirnamObject behavior,
                            BirnamObject method_name, size_t identity, int grow) {
    BirnamObject roots = name(id, "rootsDo:"), allocations = name(id, "allocationsDo:");
    BirnamObject references = name(id, "referencesOf:do:");
    BirnamObject at = name(id, "allocationAt:"), allocation_count = name(id, "allocationCount");
    BirnamOperand address = { .kind=BIRNAM_WORD, .data.word=(size_t)object };
    assert(send(id, id->receiver, at, &address, 1).value.data.object == object);
    size_t absent[] = { 0, (size_t)object + 1, (size_t)object - sizeof(size_t), (size_t)&address };
    for (size_t i=0;i<sizeof(absent)/sizeof(absent[0]);i++) {
        address.data.word=absent[i];
        assert(send(id, id->receiver, at, &address, 1).value.kind == BIRNAM_UNIT);
    }
    BirnamOperand wrong = { .kind=BIRNAM_OBJECT, .data.object=object };
    assert(id->message(id->context, id->receiver, at, &wrong, 1).status == BIRNAM_INVALID_REQUEST);
    assert(id->message(id->context, id->receiver, allocation_count, &wrong, 1).status == BIRNAM_INVALID_REQUEST);
    struct Traversal visit = { .id=id, .target=id->receiver, .behavior=behavior,
        .identity=name(id, "identityOf:"), .allocate=name(id, "allocate"),
        .expected_identity=identity, .grow=grow };
    BirnamOperand callback = { .kind=BIRNAM_NATIVE_ENTRY, .data.entry={visit_object, &visit} };
    BirnamOutcome answer = send(id, id->receiver, roots, &callback, 1);
    assert(answer.value.kind == BIRNAM_UNIT && visit.found == 1);
    visit.target=object; visit.found=visit.visited=0;
    callback.data.entry.method=visit_allocation;
    size_t before = send(id, id->receiver, allocation_count, NULL, 0).value.data.word;
    answer = send(id, id->receiver, allocations, &callback, 1);
    assert(visit.visited == before);
    assert(send(id, id->receiver, allocation_count, NULL, 0).value.data.word == before + !!grow);
    assert(answer.value.kind == BIRNAM_UNIT && visit.found == 1 && visit.visited > 7);
    assert(!grow || visit.created);
    callback.data.entry.method=visit_object;
    BirnamOperand query[] = { { .kind=BIRNAM_OBJECT, .data.object=behavior }, callback };
    visit.target=method_name; visit.found=visit.visited=0;
    answer = send(id, id->receiver, references, query, 2);
    assert(answer.value.kind == BIRNAM_WORD && answer.value.data.word == 1 && visit.found == 1);
    query[0].data.object=object; visit.target=behavior; visit.found=visit.visited=0;
    answer = send(id, id->receiver, references, query, 2);
    assert(answer.value.kind == BIRNAM_WORD && answer.value.data.word == 0 && visit.found == 1 && visit.visited == 1);
    query[0].data.object=method_name; visit.found=visit.visited=0;
    answer = send(id, id->receiver, references, query, 2);
    assert(answer.value.kind == BIRNAM_WORD && answer.value.data.word == 2 && visit.visited == 1);
    query[0].data.object=NULL; visit.visited=0;
    answer = send(id, id->receiver, references, query, 2);
    assert(answer.value.kind == BIRNAM_UNIT && visit.visited == 0);
    /* Visitor failure stops enumeration and preserves the entire native result. */
    size_t calls=0;
    callback.data.entry=(BirnamNativeEntry){stop_visit, &calls};
    answer=id->message(id->context, id->receiver, allocations, &callback, 1);
    assert(calls == 1 && answer.status == BIRNAM_INVALID_REQUEST && answer.detail == 83);
    assert(answer.value.kind == BIRNAM_WORD && answer.value.data.word == 84);
    return visit.created;
}
static void retire_disposable(const BirnamProtocol *id, BirnamObject parent, BirnamObject add) {
    BirnamObject delegated=name(id, "delegated"), allocate=name(id, "allocate");
    BirnamObject retire=name(id, "retireIndexesFor:"), detach=name(id, "detachAllocationAt:");
    BirnamObject identity=name(id, "identityOf:"), symbol_bytes=name(id, "symbolBytesOf:");
    BirnamObject at=name(id, "allocationAt:"), allocation_count=name(id, "allocationCount");
    BirnamObject behavior=send(id, parent, delegated, NULL, 0).value.data.object;
    BirnamOperand size={ .kind=BIRNAM_WORD, .data.word=16 };
    BirnamObject object=send(id, behavior, allocate, &size, 1).value.data.object;
    BirnamObject selector=name(id, "retirementOnlyNativeSelector");
    BirnamOperand method[]={ { .kind=BIRNAM_OBJECT, .data.object=selector },
        { .kind=BIRNAM_NATIVE_ENTRY, .data.entry={echo, &cookie} } };
    send(id, behavior, add, method, 2);
    ((size_t *)object)[0]=83; ((size_t *)object)[1]=84;
    /* Ordinary Object arguments cannot invoke storage destruction. */
    BirnamOperand wrong={ .kind=BIRNAM_OBJECT, .data.object=object };
    assert(id->message(id->context, id->receiver, detach, &wrong, 1).status == BIRNAM_INVALID_REQUEST);
    assert(id->message(id->context, id->receiver, retire, &wrong, 1).status == BIRNAM_INVALID_REQUEST);
    size_t addresses[]={ (size_t)object, (size_t)behavior, (size_t)selector };
    BirnamOperand dead={ .kind=BIRNAM_BYTES, .data.bytes={ (const uint8_t *)addresses, sizeof(addresses) } };
    send(id, id->receiver, retire, &dead, 1);
    wrong.data.object=selector;
    assert(send(id, id->receiver, symbol_bytes, &wrong, 1).value.kind == BIRNAM_UNIT);
    /* Retiring a name removes canonicalization before the old address is freed. */
    assert(name(id, "retirementOnlyNativeSelector") != selector);
    size_t before=send(id, id->receiver, allocation_count, NULL, 0).value.data.word;
    for (size_t i=0;i<3;i++) {
        BirnamOperand address={ .kind=BIRNAM_WORD, .data.word=addresses[i] };
        BirnamOutcome detached=send(id, id->receiver, detach, &address, 1);
        assert(detached.value.kind == BIRNAM_NATIVE_ENTRY && detached.value.data.entry.method);
        wrong.data.object=(BirnamObject)addresses[i];
        assert(send(id, id->receiver, identity, &wrong, 1).value.kind == BIRNAM_UNIT);
        assert(send(id, id->receiver, at, &address, 1).value.kind == BIRNAM_UNIT);
        assert(send(id, id->receiver, allocation_count, NULL, 0).value.data.word == before - i - 1);
        assert(send(id, id->receiver, detach, &address, 1).value.kind == BIRNAM_UNIT);
        /* Finalizers run at this point: detached bytes are still readable. */
        if (i == 0) assert(((size_t *)object)[0] == 83 && ((size_t *)object)[1] == 84);
        BirnamNativeEntry release=detached.value.data.entry;
        BirnamOutcome released=release.method(release.context, NULL, NULL, 0);
        assert(released.status == BIRNAM_LOADED && released.value.kind == BIRNAM_UNIT);
    }
}
struct RegistryVisit {
    const BirnamProtocol *id;
    BirnamObject child, add, key, first;
    size_t calls;
};
static BirnamOutcome visit_binding(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    struct RegistryVisit *visit=context;
    assert(receiver==visit->child && count==3 && args[0].kind==BIRNAM_OBJECT);
    assert(args[0].data.object==visit->first && args[1].kind==BIRNAM_NATIVE_ENTRY);
    assert(args[2].kind==BIRNAM_WORD && args[2].data.word==0);
    visit->calls++;
    BirnamOperand install[]={ {.kind=BIRNAM_OBJECT,.data.object=visit->key},
        {.kind=BIRNAM_WORD,.data.word=(size_t)visit->child},
        {.kind=BIRNAM_NATIVE_ENTRY,.data.entry={echo,&cookie}} };
    assert(send(visit->id,visit->id->receiver,visit->add,install,3).value.kind==BIRNAM_UNIT);
    return (BirnamOutcome){0};
}
static void inspect_registries(const BirnamProtocol *id, BirnamObject object, BirnamObject behavior, BirnamObject method) {
    /* Resolve all names before counting. None of the operations below collects. */
    BirnamObject named=name(id,"symbolNamed:"), symbol_count=name(id,"symbolCount");
    BirnamObject symbol_at=name(id,"symbolAt:"), name_bytes=name(id,"symbolNameBytes");
    BirnamObject epoch=name(id,"behaviorEpoch"), member=name(id,"isBehavior:");
    BirnamObject view=name(id,"behaviorView"), selectors=name(id,"selectorsIn:");
    BirnamObject binding=name(id,"bindingOf:in:"), lookup=name(id,"lookup:in:");
    BirnamObject bindings=name(id,"bindingsIn:do:"), add=name(id,"addMethod:to:as:");
    BirnamObject remove=name(id,"removeMethod:from:"), weak=name(id,"addWeakBinding:to:as:");
    BirnamObject create=name(id,"newBehaviorWithParent:"), bind=name(id,"bind:to:");
    BirnamObject extra=name(id,"registrySnapshotAdded"), retire=name(id,"retireIndexesFor:");
    BirnamObject detach=name(id,"detachAllocationAt:");
    size_t count=send(id,id->receiver,symbol_count,NULL,0).value.data.word;
    size_t bytes=send(id,id->receiver,name_bytes,NULL,0).value.data.word;
    const char *missing="registryNameThatWasNeverInterned";
    BirnamOperand spelling={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)missing,strlen(missing)}};
    assert(send(id,id->receiver,named,&spelling,1).value.kind==BIRNAM_UNIT);
    assert(send(id,id->receiver,symbol_count,NULL,0).value.data.word==count);
    assert(send(id,id->receiver,name_bytes,NULL,0).value.data.word==bytes);
    spelling.data.bytes.pointer=(const uint8_t *)"privateNativeEcho:";
    spelling.data.bytes.length=strlen("privateNativeEcho:");
    assert(send(id,id->receiver,named,&spelling,1).value.data.object==method);
    size_t found=0;
    for(size_t i=0;i<=count;i++) {
        BirnamOperand index={.kind=BIRNAM_WORD,.data.word=i};
        BirnamOutcome value=send(id,id->receiver,symbol_at,&index,1);
        if(i==count) assert(value.value.kind==BIRNAM_UNIT);
        else { assert(value.value.kind==BIRNAM_OBJECT);found+=value.value.data.object==method; }
    }
    assert(found==1 && bytes>count);
    BirnamOperand parent={.kind=BIRNAM_OBJECT,.data.object=behavior};
    assert(id->message(id->context,id->receiver,create,&parent,1).status==BIRNAM_INVALID_REQUEST);
    parent=(BirnamOperand){.kind=BIRNAM_WORD,.data.word=(size_t)behavior};
    BirnamObject child=send(id,id->receiver,create,&parent,1).value.data.object;
    BirnamOperand child_arg={.kind=BIRNAM_OBJECT,.data.object=child};
    assert(send(id,id->receiver,member,&child_arg,1).value.data.word==1);
    BirnamOperand object_arg={.kind=BIRNAM_OBJECT,.data.object=object};
    assert(send(id,id->receiver,member,&object_arg,1).value.data.word==0);
    BirnamOutcome list=send(id,id->receiver,view,NULL,0);assert(list.value.kind==BIRNAM_BYTES);
    found=0;
    for(size_t i=0;i<list.value.data.bytes.length/sizeof(BirnamObject);i++)
        found+=((const BirnamObject *)list.value.data.bytes.pointer)[i]==child;
    assert(found==1);
    BirnamOperand query[]={ {.kind=BIRNAM_OBJECT,.data.object=method}, child_arg };
    assert(send(id,id->receiver,binding,query,2).value.kind==BIRNAM_UNIT);
    assert(send(id,id->receiver,lookup,query,2).value.data.entry.method==echo);
    BirnamOperand install[]={query[0],child_arg,object_arg};
    size_t rejected_epoch=send(id,id->receiver,epoch,NULL,0).value.data.word;
    assert(id->message(id->context,id->receiver,weak,install,3).status==BIRNAM_INVALID_REQUEST);
    assert(id->message(id->context,id->receiver,remove,query,2).status==BIRNAM_INVALID_REQUEST);
    assert(send(id,id->receiver,epoch,NULL,0).value.data.word==rejected_epoch);
    install[1]=(BirnamOperand){.kind=BIRNAM_WORD,.data.word=(size_t)child};
    send(id,id->receiver,weak,install,3);
    assert(send(id,id->receiver,binding,query,2).value.data.object==object);
    assert(send(id,id->receiver,lookup,query,2).value.kind==BIRNAM_UNIT); /* Opaque shadow. */
    assert(send(id,id->receiver,remove,install,2).value.kind==BIRNAM_UNIT);
    install[2]=(BirnamOperand){.kind=BIRNAM_NATIVE_ENTRY,.data.entry={echo,&cookie}};
    size_t before=send(id,id->receiver,epoch,NULL,0).value.data.word;
    assert(send(id,id->receiver,add,install,3).value.kind==BIRNAM_UNIT);
    assert(send(id,id->receiver,epoch,NULL,0).value.data.word==before+1);
    BirnamOutcome previous=send(id,id->receiver,add,install,3);
    assert(previous.value.kind==BIRNAM_NATIVE_ENTRY && previous.value.data.entry.method==echo && previous.value.data.entry.context==&cookie);
    struct RegistryVisit visit={id,child,add,extra,method,0};
    BirnamOperand scan[]={child_arg,{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={visit_binding,&visit}}};
    assert(send(id,id->receiver,bindings,scan,2).value.kind==BIRNAM_UNIT && visit.calls==1);
    list=send(id,id->receiver,selectors,&child_arg,1);
    assert(list.value.kind==BIRNAM_BYTES && list.value.data.bytes.length==2*sizeof(BirnamObject));
    assert(((const BirnamObject *)list.value.data.bytes.pointer)[0]==method);
    assert(((const BirnamObject *)list.value.data.bytes.pointer)[1]==extra);
    size_t calls=0;scan[1].data.entry=(BirnamNativeEntry){stop_visit,&calls};
    BirnamOutcome failure=id->message(id->context,id->receiver,bindings,scan,2);
    assert(calls==1 && failure.status==BIRNAM_INVALID_REQUEST && failure.detail==83 && failure.value.data.word==84);
    BirnamOperand selection[]={query[0],object_arg};
    BirnamOutcome selected=send(id,id->receiver,bind,selection,2);
    assert(selected.value.kind==BIRNAM_NATIVE_ENTRY && selected.value.data.entry.method==echo && selected.value.data.entry.context==&cookie);
    /* This trusted temporary behavior never escapes or participates in upper ownership. */
    size_t address=(size_t)child;
    BirnamOperand dead={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)&address,sizeof(address)}};
    send(id,id->receiver,retire,&dead,1);
    assert(send(id,id->receiver,member,&child_arg,1).value.data.word==0);
    BirnamOperand word={.kind=BIRNAM_WORD,.data.word=address};
    BirnamNativeEntry release=send(id,id->receiver,detach,&word,1).value.data.entry;
    assert(release.method(release.context,NULL,NULL,0).status==BIRNAM_LOADED);
}
static void construct_objects(const BirnamProtocol *id, BirnamObject add, const BirnamNativeEntry *collect) {
    /* Resolve names before construction. These temporary objects do not escape
       initialization; the caller's upper loading turn protects native code. */
    BirnamObject prototype = name(id, "objectPrototype"), behavior_of = name(id, "behaviorOf:");
    BirnamObject delegated = name(id, "delegated"), allocate = name(id, "allocate");
    BirnamObject size_of = name(id, "allocationSizeOf:"), identity_of = name(id, "identityOf:");
    BirnamObject symbol_bytes = name(id, "symbolBytesOf:");
    BirnamObject echo_name = construction_roots[0] = name(id, "privateNativeEcho:");
    BirnamOperand spelling = { .kind = BIRNAM_OBJECT, .data.object = echo_name };
    BirnamOutcome bytes = send(id, id->receiver, symbol_bytes, &spelling, 1);
    assert(bytes.value.kind == BIRNAM_BYTES && bytes.value.data.bytes.length == strlen("privateNativeEcho:"));
    assert(!memcmp(bytes.value.data.bytes.pointer, "privateNativeEcho:", bytes.value.data.bytes.length));
    BirnamOutcome answer = send(id, id->receiver, prototype, NULL, 0);
    assert(answer.value.kind == BIRNAM_OBJECT);
    BirnamOperand argument = answer.value;
    answer = send(id, id->receiver, behavior_of, &argument, 1);
    assert(answer.value.kind == BIRNAM_OBJECT);
    BirnamObject parent = answer.value.data.object;
    answer = send(id, parent, delegated, NULL, 0);
    assert(answer.value.kind == BIRNAM_OBJECT);
    BirnamObject behavior = construction_roots[1] = answer.value.data.object;
    argument = (BirnamOperand){ .kind = BIRNAM_WORD, .data.word = 16 };
    answer = send(id, behavior, allocate, &argument, 1);
    assert(answer.value.kind == BIRNAM_OBJECT);
    BirnamObject object = construction_roots[2] = answer.value.data.object;
    ((size_t *)object)[0] = 83; ((size_t *)object)[1] = 84;
    argument = (BirnamOperand){ .kind = BIRNAM_OBJECT, .data.object = object };
    answer = send(id, id->receiver, size_of, &argument, 1);
    assert(answer.value.kind == BIRNAM_WORD && answer.value.data.word == 16);
    answer = send(id, id->receiver, identity_of, &argument, 1);
    assert(answer.value.kind == BIRNAM_WORD && answer.value.data.word != 0);
    size_t identity = answer.value.data.word;
    if (collect) {
        construction_collections = 0;
        answer = collect->method(collect->context, id->receiver, NULL, 0);
        assert(answer.status == BIRNAM_LOADED && construction_collections != 0);
        answer = send(id, id->receiver, identity_of, &argument, 1);
        assert(answer.value.kind == BIRNAM_WORD && answer.value.data.word == identity);
    }
    BirnamOperand method[] = { { .kind = BIRNAM_OBJECT, .data.object = echo_name },
        { .kind = BIRNAM_NATIVE_ENTRY, .data.entry = { echo, &cookie } } };
    send(id, behavior, add, method, 2);
    answer = send(id, object, echo_name, &argument, 1);
    assert(answer.value.kind == BIRNAM_OBJECT && answer.value.data.object == object);
    assert(((size_t *)object)[0] == 83 && ((size_t *)object)[1] == 84);
    BirnamObject temporary=inspect_storage(id, object, behavior, echo_name, identity, collect == NULL);
    inspect_registries(id, object, behavior, echo_name);
    if (!collect) {
        /* These Id-only probes precede Lifetime. Destroy their temporary raw
           payloads before entering the later validated production bootstrap. */
        size_t addresses[]={(size_t)object,(size_t)behavior,(size_t)temporary};
        BirnamOperand dead={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)addresses,sizeof(addresses)}};
        send(id,id->receiver,name(id,"retireIndexesFor:"),&dead,1);
        BirnamObject detach=name(id,"detachAllocationAt:");
        for(size_t i=0;i<3;i++) {
            BirnamOperand address={.kind=BIRNAM_WORD,.data.word=addresses[i]};
            BirnamOutcome detached=send(id,id->receiver,detach,&address,1);
            assert(detached.value.kind==BIRNAM_NATIVE_ENTRY);
            BirnamNativeEntry release=detached.value.data.entry;
            assert(release.method(release.context,NULL,NULL,0).status==BIRNAM_LOADED);
        }
    }
    if (!collect) retire_disposable(id, parent, add);
}
/* Handwritten C declaration: validation must precede any ownership transfer. */
static void check_runtime_constructor(const BirnamProtocol *id) {
    typedef void *(*RuntimeV12)(const BirnamProtocol *, const void *, const void *,
        const void *, const void *, const void *, const void *, const void *, const void *);
    Dl_info owner;
    assert(dladdr((void *)id->message, &owner));
    void *image = dlopen(owner.dli_fname, RTLD_NOW | RTLD_LOCAL);
    assert(image);
    RuntimeV12 initialize = (RuntimeV12)dlsym(image, "birnam_runtime_init_v12");
    assert(initialize && !dlsym(image, "birnam_runtime_init_v11"));
    assert(!initialize(NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    BirnamProtocol bad = *id;
    bad.version = 99;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    bad = *id;
    bad.byte_size = 8;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    bad = *id;
    bad.message = NULL;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    assert(!initialize(id, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    assert(!dlclose(image));
}
static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *arguments, size_t count) {
    check_runtime_constructor(id);
    assert(id && id->version == 1 && id->byte_size == sizeof(*id));
    assert((count == 2 || count == 4) && arguments[0].kind == BIRNAM_BYTES && arguments[1].kind == BIRNAM_WORD);
    if (count == 4) {
        assert(arguments[2].kind == BIRNAM_WORD && arguments[3].kind == BIRNAM_NATIVE_ENTRY);
        const RootRegistryV1 *roots = (const RootRegistryV1 *)arguments[2].data.word;
        assert(roots && roots->version == 1 && roots->byte_size == sizeof(*roots));
        roots->register_root(roots->context, retain_construction);
    }
    cookie = arguments[1].data.word;
    assert(cookie == 0x83);
    BirnamObject selector = construction_roots[3] = intern(id, arguments[0]);
    const char *name = "addMethod";
    BirnamObject add = intern(id, (BirnamOperand){ .kind = BIRNAM_BYTES,
        .data.bytes = { (const uint8_t *)name, strlen(name) } });
    construct_objects(id, add, count == 4 ? &arguments[3].data.entry : NULL);
    BirnamOperand install[] = {
        { .kind = BIRNAM_OBJECT, .data.object = selector },
        { .kind = BIRNAM_NATIVE_ENTRY, .data.entry = { echo, &cookie } }
    };
    BirnamOutcome answer = id->message(id->context, id->receiver, add, install, 2);
    assert(answer.status == BIRNAM_LOADED && answer.value.kind == BIRNAM_NATIVE_ENTRY);
    const char *extra_names[] = { "nativeReturnsWord", "nativeRejectsArgument" };
    BirnamNativeMethod extra_methods[] = { returns_word, rejects_argument };
    for (size_t i=0;i<2;i++) {
        install[0].data.object = intern(id, (BirnamOperand){ .kind = BIRNAM_BYTES,
            .data.bytes = { (const uint8_t *)extra_names[i], strlen(extra_names[i]) } });
        install[1].data.entry.method = extra_methods[i];
        answer = id->message(id->context, id->receiver, add, install, 2);
        assert(answer.status == BIRNAM_LOADED);
    }
    memset(construction_roots, 0, sizeof(construction_roots));
    return (BirnamOutcome){ .status = MODE ? BIRNAM_INITIALIZER_FAILED : BIRNAM_LOADED,
        .detail = MODE ? 42 : 0, .value = { .kind = BIRNAM_OBJECT, .data.object = selector } };
}
const BirnamModule birnam_native_module_v1 = { 1, sizeof(BirnamModule), initialize };
#elif MODE == 2
/* A mapped library without the descriptor must be closed before entry. */
int deliberately_missing_descriptor;
#elif MODE == 3
const BirnamModule birnam_native_module_v1 = { 99, sizeof(BirnamModule), NULL };
#elif MODE == 4
/* Only the common header exists: admission must not read the next field. */
const struct { uint32_t version, byte_size; } birnam_native_module_v1 = { 1, 8 };
#elif MODE == 5
const BirnamModule birnam_native_module_v1 = { 1, sizeof(BirnamModule), NULL };
#endif
