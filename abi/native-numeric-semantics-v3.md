# Optional native numeric implementation declarations, v3

This optional higher-module export supplies compiler facts about exact native
implementations. It adds no IdApi field and does not alter the Method ABI,
canonical descriptors or image encoding. Modules without it remain callable.

```c
unsigned int birnam_native_numeric_semantics_v3(
    const void *method, void *integer_vtable, const uintptr_t *functions);
```

The query is synchronous, allocation-free and effect-free. It must verify that
`integer_vtable` is the Integer module's canonical behavior. `functions` points
to eleven addresses/identities in order: Integer `is_exact`, Integer `read`, Namespace
presence `contains`, Id `import`, Id `vtableOf`, Integer `make`, Integer context,
Integer storage descriptor, Integer prototype, Id `alloc`, and Namespace presence
context. The floor supplies its original canonical presence function/context. The producer
compares these with its live dependencies and returns zero on any mismatch.
The consumer also requires its current Integer table to match the admitted
contract and the constructor's current allocator to be the canonical Id allocator.
The query never invokes those operations. Wrapping and restoring an operation
withdraws and restores admission on each invocation. It also returns zero for
an unrecognized Method address, otherwise one of these contracts:

| Code | Successful integer operation |
| --- | --- |
| 1 | Signed addition |
| 2 | Signed subtraction |
| 3 | Signed multiplication |
| 4 | Signed division, truncating toward zero |
| 5 | Signed remainder, with the dividend's sign |

The supported subset is one canonical signed-64-bit Integer receiver and one
canonical signed-64-bit Integer argument. The implementation reads their values
before allocating its result. Intermediate mathematical arithmetic uses signed
128-bit range; the result must fit signed 64 bits. Division/remainder by zero and
overflow are outside the successful subset. In particular MIN/-1 overflows,
while MIN remainder -1 equals zero. No algebraic reassociation is implied.

An implementation conforming to these codes follows the existing ErrorVt rule:
if the currently exported `ErrorVt` equals the argument's vtable, return the
original argument without allocation. Otherwise the successful subset performs
exactly one ordinary Integer allocation with the computed value, including the
floor's usual collection check at that allocation. It has no other Id effects.
The lookup/import of `ErrorVt` may intern that symbol: a consumer cannot silently
omit this effect when the symbol does not already exist. Other operands, errors
and unsupported states invoke the actual original Method.

This is an unsafe native-module contract, not an annotation that arbitrary Id
data can attach to opaque code. The native producer is responsible for the same
accuracy as its Method and GC callback declarations. The runtime obtains the
query from the actual loaded image containing the resolved Method and verifies
that the query belongs to that image rather than a dependency. It retains a
native loader reference for the summary's entire lifetime. Unknown codes or
missing declarations withdraw optimization. Merely using an arithmetic selector,
publishing an object with a familiar name, or replacing an Id metadata object
cannot grant this authority.

Compiler plans remain ordinary data derived from these facts; the declaration
does not prescribe which selectors have numeric meaning, which methods remain
bound, which receiver shapes apply, or where optimization is profitable. Every
execution uses current dispatch and guards actual representation and mutable
environment state. Native arithmetic is supplied by the loaded Integer module; it is not
installed as an immutable language rule in the floor.
