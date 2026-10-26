#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop raw_read, restore;
static Oop absent(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)context;
    if (count != 2) return id.invalid;
    Oop value = id.message(id.root, receiver, raw_read, args, count);
    return id_fault(value) ? value : id_size(value == 0);
}
static Oop restore_absent(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)context;
    if (count != 4) return id.invalid;
    Oop values[] = {args[0], args[1], args[2], 0, args[3]};
    return id.message(id.root, receiver, restore, values, 5);
}
static Oop initialize(const IdProtocol *protocol, Oop image, const Oop *literals,
                      size_t count, const Oop *args, size_t argc) {
    (void)args;
    if (argc || count != 4) return protocol->invalid;
    id = *protocol; raw_read = literals[0]; restore = literals[1];
    return image;
}
static const IdLiteral literals[] = {
    LIT("rawCurrentValue:attr:"), LIT("restoreValue:attr:expected:previous:reservation:"),
    LIT("rawAbsent:attr:"), LIT("restoreAbsent:attr:expected:reservation:")
};
static const IdMethodSpec methods[] = {{2, absent}, {3, restore_absent}};
const IdModule birnam_native_module_v2 = {
    2, sizeof(IdModule), literals, 4, methods, 2, initialize
};
