#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop symbols[9];
enum { SEQUENCE, AT, PUT, COLLECT, INVOKE, FORWARD, FAIL, FIXED, CAPTURE };
static Oop send(Oop receiver, size_t selector, const Oop *args, size_t count) {
    return id.message(id.root, receiver, symbols[selector], args, count);
}
#define CHECK(value) do { if (id_fault(value)) return value; } while (0)
static Oop forward(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    Oop index = id_size(0);
    Oop original = send(context, AT, &index, 1); CHECK(original);
    index = id_size(1);
    Oop gc = send(context, AT, &index, 1); CHECK(gc);
    index = id_size(2);
    Oop calls = send(context, AT, &index, 1); CHECK(calls);
    Oop update[] = { index, id_size(id_unbox(calls) + 1) };
    Oop changed = send(context, PUT, update, 2); CHECK(changed);
    Oop collected = send(gc, COLLECT, 0, 0); CHECK(collected);
    Oop values = send(id.root, SEQUENCE, args, count); CHECK(values);
    Oop invocation[] = { receiver, values };
    return send(original, INVOKE, invocation, 2);
}
static Oop capture(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    if (count != 1) return id.invalid;
    Oop update[] = { id_size(3), args[0] };
    Oop changed = send(context, PUT, update, 2); CHECK(changed);
    return forward(root, context, receiver, args, count);
}
static Oop fail(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)receiver; (void)args; (void)count;
    return id_failure(context);
}
static Oop fixed(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)receiver; (void)args; (void)count; return context;
}
static Oop initialize(const IdProtocol *protocol, Oop image, const Oop *names,
                      size_t count, const Oop *args, size_t argc) {
    (void)args;
    if (argc || count != 9) return protocol->invalid;
    id = *protocol;
    for (size_t i = 0; i < count; i++) symbols[i] = names[i];
    return image;
}
static const IdLiteral literals[] = {
    LIT("sequence"), LIT("at:"), LIT("at:put:"), LIT("collectPrecisely"),
    LIT("invokeOn:arguments:"), LIT("collectingForward"), LIT("fail"), LIT("fixed"), LIT("captureForward")
};
static const IdMethodSpec methods[] = {{FORWARD, forward}, {FAIL, fail}, {FIXED, fixed}, {CAPTURE, capture}};
const IdModule birnam_native_module_v2 = {
    2, sizeof(IdModule), literals, 9, methods, 4, initialize
};
