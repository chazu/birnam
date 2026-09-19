# Optional native numeric implementation declarations, v1

Historical declaration for IdApi v1-v3. Current consumers use
[v2](native-numeric-semantics-v2.md) with an explicit Integer dependency.

This optional higher-module export supplies compiler facts about exact native
implementations. It adds no IdApi field and does not alter the Method ABI,
canonical descriptors or image encoding. Modules without it remain callable.

```c
unsigned int birnam_native_numeric_semantics_v1(
    const void *method, void *integer_vtable, const uintptr_t *functions);
```

The query is synchronous, allocation-free and effect-free. It must verify that
`integer_vtable` is the canonical Integer vtable of the floor against which its
native implementation was initialized; otherwise it returns zero. `functions`
points to six addresses in this order: `isSmallInt`, `smallIntValue`,
`isExported`, `import`, `vtableOf`, `mkSmallInt`. A producer using those IdApi
entries must compare them with its current table and decline on any mismatch;
in particular, an instrumented boxing/import function is not bypassed. The
consumer revalidates this context for each invocation, so changing and restoring
an API wrapper can withdraw and restore optimization. It also returns zero for
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
