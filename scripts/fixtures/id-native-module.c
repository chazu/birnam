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
