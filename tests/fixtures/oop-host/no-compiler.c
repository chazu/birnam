#include "../oop-id-v2/id.h"
static IdProtocol id;
static size_t calls;
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
static Oop initialize(const IdProtocol *protocol, Oop image, const Oop *literals,
                      size_t count, const Oop *args, size_t argc) {
    (void)literals; (void)count; (void)args;
    if (argc) return protocol->invalid;
    id = *protocol;
    return image;
}
static const IdLiteral literals[] = {LIT("deny"), LIT("calls"), LIT("packetCount")};
static const IdMethodSpec methods[] = {{0, deny}, {1, count}, {2, packet_count}};
const IdModule birnam_native_module_v2 = {
    2, sizeof(IdModule), literals, 3, methods, 3, initialize
};
