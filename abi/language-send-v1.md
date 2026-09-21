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

Each Method retains the owning TCC Unit in its traced payload when the function
belongs to an anonymous materialization. Registered generated implementations
also retain their generation lease; other native functions retain their mapped
image. A native
author must retain the entry's context and register any object references it
contains; retaining the image does not own arbitrary native context memory.
An unregistered JIT function without a known Unit or retainable image is rejected. Method
invocation roots its Method, receiver and arguments across callbacks and GC.
Finalization releases the generation lease and queues the image handle for an
eligible outer-turn retirement checkpoint.

TCC and Codegen select two completed Runtime exports during construction:

```c
int birnam_runtime_register_method_owner_v1(const void *function, Oop unit);
void birnam_runtime_forget_method_owner_v1(Oop unit);
```

Register the actual owning Unit before publishing a generated function. The
address-to-Unit table is weak; the caller retains the Unit across registration
and publication. Registration returns 1 on success and 0 for invalid inputs or
a conflicting owner. Forget its addresses before finalization releases the Unit.
These are trusted native operations, not language pointer APIs.

Ordinary source compilation uses the same ownership path. TCC fills the
generated `__birnam_native_unit` reference during relocation, before publishing
the Unit. Generated Block installation registers its methods with that Unit;
mapped artifacts keep using their image/generation ownership. Source evaluation
roots its active Unit and result temporarily. Its caches belong to the Unit,
and surviving Block/Method/binding references replace the old process-wide list
of retained source Units.

Bindings retain Units through [Lifetime v2](fixtures/lifetime-v2/README.md):
each behavior/selector pair contributes an edge to its Unit. Trace follows that
edge only when the behavior is reachable, so unreachable Unit/behavior cycles
can collect. Removing or replacing a binding retains the old Unit until the
outer turn returns; a lookup that sends messages or collects cannot free it
under an unfinished dispatch. No executable ownership is added to Id.

Synchronous portable Block entries check arity and require sufficiently large,
traced object slots for captures before reading the receiver's payload.

Language lookup results accept Method and nil. Another live object is an
invalid result, never an executable address. Raw native addresses remain an
unsafe legacy convention until those consumers migrate. Image saving rejects
Method instances as `methodNotImageable`; the native Method prototype remains
a named reference.

Production acceptance lives in `scripts/method-values-smoke.sh`,
`scripts/method-unit-lifetime-smoke.sh` and the
`execution-values-native.c` fixture run by `scripts/id-native-loading-smoke.py`.
