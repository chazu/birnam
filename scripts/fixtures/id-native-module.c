/* Independent C consumer of the production Id loading protocol. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
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
static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *arguments, size_t count) {
    assert(id && id->version == 1 && id->byte_size == sizeof(*id));
    assert(count == 2 && arguments[0].kind == BIRNAM_BYTES && arguments[1].kind == BIRNAM_WORD);
    cookie = arguments[1].data.word;
    assert(cookie == 0x83);
    BirnamObject selector = intern(id, arguments[0]);
    const char *name = "addMethod";
    BirnamObject add = intern(id, (BirnamOperand){ .kind = BIRNAM_BYTES,
        .data.bytes = { (const uint8_t *)name, strlen(name) } });
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
