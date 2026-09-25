#include "id_protocol.h"
BirnamOutcome birnam_id_protocol_echo(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    if(context!=receiver || count!=1 || !args || args[0].kind>BIRNAM_PROTOCOL || args[0].reserved) {
        return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST,.detail=count};
    }
    switch(args[0].kind) {
    case BIRNAM_WORD:
        if(args[0].data.word!=SIZE_MAX) return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST,.detail=10};
        break;
    case BIRNAM_BYTES:
        if(args[0].data.bytes.length!=8 || args[0].data.bytes.pointer[6]!=0 || args[0].data.bytes.pointer[7]!=255)
            return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST,.detail=11};
        break;
    case BIRNAM_NATIVE_ENTRY:
        if(args[0].data.entry.method!=birnam_id_protocol_echo || args[0].data.entry.context!=receiver)
            return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST,.detail=12};
        break;
    case BIRNAM_PROTOCOL: {
        const BirnamProtocol *protocol=args[0].data.protocol;
        if(protocol->version!=1 || protocol->byte_size!=sizeof(*protocol) || protocol->message || protocol->context || protocol->receiver!=receiver || !protocol->intern || !protocol->load)
            return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST,.detail=13};
        break;
    }
    default: break;
    }
    return (BirnamOutcome){.status=BIRNAM_LOADED,.value=args[0]};
}

/* Independent owner: the enumeration argument is an Object receiving a send. */
BirnamOutcome birnam_gc_enumerate(void *context, BirnamObject receiver,
                                const BirnamOperand *args, size_t count) {
    (void)receiver;
    const BirnamProtocol *id = context;
    if (count != 1 || !args || args[0].kind != BIRNAM_OBJECT || !args[0].data.object)
        return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST};
    const BirnamOperand name = {.kind=BIRNAM_BYTES, .data.bytes={(const uint8_t *)"visit:", 6}};
    BirnamOutcome selector = id->message(id->context, id->receiver, id->intern, &name, 1);
    if (selector.status != BIRNAM_LOADED) return selector;
    const BirnamOperand edge = {.kind=BIRNAM_OBJECT, .data.object=id->receiver};
    return id->message(id->context, args[0].data.object, selector.value.data.object, &edge, 1);
}
