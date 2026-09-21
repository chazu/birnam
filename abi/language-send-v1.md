# Checked language send v1

The upper Runtime exports `birnam_runtime_send_v1`:

```c
Oop birnam_runtime_send_v1(Oop receiver, Oop selector,
                          const Oop *arguments, size_t count);
```

Receiver, selector and arguments are live language objects. A null argument
buffer is valid only for zero arguments. The caller follows the normal
owner-thread and native-root contracts. The entry enters Execution's managed
turn, resolves once and invokes the exact result. Native `Object` results stay
objects, `Unit` becomes nil, and a `NativeEntry` becomes an owned `Method`.
Unsupported operand results and native failures become language errors.

TCC and Portable Code select this completed entry from the actual Runtime image
during construction, before publishing their language execution interfaces.
`birnam_abi::language::select` performs that selection. The frozen `IdApi.send`
table field still serves unsafe legacy native consumers and retains its raw
Method convention. It is not the language execution entry. This adapter remains
a migration boundary; Id's typed native protocol itself is unchanged.

`Object methodFor:` searches the receiver's behavior. `Vtable methodFor:` searches
that behavior directly. Method instances provide `invokeOn:`,
`invokeOn:arguments:` and identity `clone`. `Vtable addMethod:as:` accepts a Method
and a selector, preserving the complete native function/context pair.

Each Method retains either an explicit lease on a registered generated
implementation or the mapped native image containing the function. A native
author must retain the entry's context and register any object references it
contains; retaining the image does not own arbitrary native context memory.
An unregistered JIT function without a retainable image is rejected. Method
invocation roots its Method, receiver and arguments across callbacks and GC.
Finalization releases the generation lease and queues the image handle for an
eligible outer-turn retirement checkpoint.

Language lookup results accept Method and nil. Another live object is an
invalid result, never an executable address. Raw native addresses remain an
unsafe legacy convention until those consumers migrate. Image saving rejects
Method instances as `methodNotImageable`; the native Method prototype remains
a named reference.

Production acceptance lives in `scripts/method-values-smoke.sh` and the
`execution-values-native.c` fixture run by `scripts/id-native-loading-smoke.py`.
