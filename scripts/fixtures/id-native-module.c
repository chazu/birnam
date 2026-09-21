/* Independent C consumer of the production Id loading protocol. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-v1/root_registry_v1.h"
#include <assert.h>
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
    struct Traversal visit = { .id=id, .target=id->receiver, .behavior=behavior,
        .identity=name(id, "identityOf:"), .allocate=name(id, "allocate"),
        .expected_identity=identity, .grow=grow };
    BirnamOperand callback = { .kind=BIRNAM_NATIVE_ENTRY, .data.entry={visit_object, &visit} };
    BirnamOutcome answer = send(id, id->receiver, roots, &callback, 1);
    assert(answer.value.kind == BIRNAM_UNIT && visit.found == 1);
    visit.target=object; visit.found=visit.visited=0;
    callback.data.entry.method=visit_allocation;
    answer = send(id, id->receiver, allocations, &callback, 1);
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
    for (size_t i=0;i<3;i++) {
        BirnamOperand address={ .kind=BIRNAM_WORD, .data.word=addresses[i] };
        BirnamOutcome detached=send(id, id->receiver, detach, &address, 1);
        assert(detached.value.kind == BIRNAM_NATIVE_ENTRY && detached.value.data.entry.method);
        wrong.data.object=(BirnamObject)addresses[i];
        assert(send(id, id->receiver, identity, &wrong, 1).value.kind == BIRNAM_UNIT);
        assert(send(id, id->receiver, detach, &address, 1).value.kind == BIRNAM_UNIT);
        /* Finalizers run at this point: detached bytes are still readable. */
        if (i == 0) assert(((size_t *)object)[0] == 83 && ((size_t *)object)[1] == 84);
        BirnamNativeEntry release=detached.value.data.entry;
        BirnamOutcome released=release.method(release.context, NULL, NULL, 0);
        assert(released.status == BIRNAM_LOADED && released.value.kind == BIRNAM_UNIT);
    }
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
static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *arguments, size_t count) {
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
