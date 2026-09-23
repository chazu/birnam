/* A real Id-loaded executable fixture. Its context is owned by the explicit
 * language owner; no dependency services or foreign payload layouts cross. */
#include "id_protocol.h"

static BirnamOutcome answer_owner(void *context, BirnamObject receiver,
                                  const BirnamOperand *args, size_t count) {
    (void)receiver;
    (void)args;
    (void)count;
    return (BirnamOutcome){ .status = BIRNAM_LOADED,
        .value = { .kind = BIRNAM_OBJECT, .data.object = context } };
}

static BirnamOutcome initialize(const BirnamProtocol *id,
                                const BirnamOperand *args, size_t count) {
    if (!id || id->version != 1 || id->byte_size != sizeof(*id) ||
        !id->message || count != 2 || !args ||
        args[0].kind != BIRNAM_OBJECT || args[1].kind != BIRNAM_OBJECT)
        return (BirnamOutcome){ .status = BIRNAM_INVALID_REQUEST };
    static const uint8_t name[] = "fromNative:owner:";
    BirnamOperand text = { .kind = BIRNAM_BYTES,
        .data.bytes = { name, sizeof(name) - 1 } };
    BirnamOutcome selector = id->message(id->context, id->receiver, id->intern, &text, 1);
    if (selector.status != BIRNAM_LOADED) return selector;
    BirnamOperand operands[] = {
        { .kind = BIRNAM_NATIVE_ENTRY,
          .data.entry = { answer_owner, args[1].data.object } },
        args[1]
    };
    return id->message(id->context, args[0].data.object,
                       selector.value.data.object, operands, 2);
}

__attribute__((visibility("default")))
const BirnamModule birnam_native_module_v1 = { 1, sizeof(BirnamModule), initialize };
