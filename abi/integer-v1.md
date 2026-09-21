# Native Integer dependency, v1

`libbirnam_integer.dylib` owns Integer's canonical behavior and prototype, signed
64-bit payload and primitive construction/reading/recognition. Its production
base is constructed through Id before Float, Word, Block or Runtime. Id boots
without Integer; its remaining Machine and optimizer components explicitly
acquire Integer when the host composes the stack.

## Production construction

`birnam_native_module_v1` receives the Id protocol plus six initializer operands:
five Words pointing to completed root-registry, Lifetime, base-values, Namespace
and String descriptors, followed by the actual Error factory as an Object.
Object Model is already installed. It returns a Word pointing to the existing
Integer operations table; no descriptor layout or library is added.

The base constructs and publishes Integer/IntegerVt, records its raw payload
layout in Lifetime, and installs `fromWord:`, `word`, `+`, `-`, `*`, `/`, `mod:`,
`=`, `<`, `<=`, `>`, `>=` and `printString`. Native Word operands carry the signed
64-bit value's bits; they do not require the language Word module. Arithmetic
preserves signed-128-bit intermediates and signed-64-bit bounds, existing Error
propagation, and exact Integer equality. Ordered comparison preserves the
existing conversion to double precision. Errors use the current Namespace
Error/ErrorVt bindings and String/ Error factory messages.

`asFloat` and `to:do:` are absent until their dependencies mount. Once Float is
constructed, it sends Integer `installFloat:` with its completed operations
table as a native Word. This creates a separate extension context and installs
mixed arithmetic/comparisons and `asFloat`, replacing only original base entries
and preserving early replacements, `mod:`, `=` and `printString`. The base has no
future Float slot. Repeated or malformed extension installation is rejected.
The later `module_init` reuses the table and adds only the remaining Block
iteration; its native Execution extension is still part of Design 83.

Upper consumers obtain the identical table through
`birnam_integer_provider_v1()`. The host passes it to each consumer's optional
`birnam_integer_dependency_v1(IntegerApiV1 *) -> int` before `module_init`.
The host also attaches itself and the still-combined Machine library. Success is
1; malformed or different repeated dependencies return 0. The same table can be
reattached. A missing dependency fails mounting before the consumer initializes.
`module_init`'s existing opaque prior-state contract is unchanged.

The [Rust mirror](../rust/birnam-abi/src/integer.rs) and
[independent C header](fixtures/integer-v1/integer_v1.h) define an 80-byte operations
table and a separate 48-byte immutable storage descriptor on arm64 macOS.
The operations table carries version/extent, context, canonical identities,
`make`, `read`, `is_exact`, the storage descriptor, and `allocation_context`.
Consumers copy the live table before calling an operation; instrumentation can
replace entries on the owning thread. Tables, context, and native code remain
live for the existing process lifetime of a mounted module. This introduces no
new unloading policy and no aggregate replacement for IdApi.

Production `make` sends the current Id `allocate` method and validates the
returned allocation's size and behavior before initializing its payload.
The legacy IdApi initializer retains direct allocation for existing native
clients. Both preserve the existing fresh-result contract. `read` requires a
live Integer payload and remains valid after that object's behavior is changed.
`is_exact` checks heap membership and the original canonical behavior, rejecting
null, foreign addresses, and descendants. Image handling deliberately retains
its separate inherited-behavior rule. Rebinding `Integer` or `IntegerVt` does not
change the module's canonical identities; an explicit root provider retains both,
including throughout partially initialized mounting. Native contexts also root
their cached selectors and predecessor references, even after method removal.

The immutable codec describes payload extent/alignment and pure `initialize` and
`read` callbacks. Those callbacks access only the supplied bytes: no Id calls,
allocation, collection, reentry, or mutable module state. The current payload is
one signed 64-bit word, size/alignment 8. Recovery rechecks admission and layouts
after its safepoint, allocates generic storage, initializes it, and immediately
roots the result. Deferred allocations encode opaque bytes in aligned untraced
native storage before charging; Heap settles those bytes without codec callbacks.
Equal values still incur separate identities and charges. Counted slot indices
retain their separate bounded ledger and do not consume the general journal.

The existing optimization requires exact admitted identities, context, code and
codec, and the original direct Id allocator. Production message construction
cannot attest that contract: `allocation_context` and the optional
[v4 numeric declaration](native-numeric-semantics-v4.md) return zero. Ordinary
dispatch remains available; native-aware compiler admission is still unfinished.
Legacy direct construction retains its existing declaration and guards.
The private closure-elision hook is now `birnam_id_try_closure_elision_v2`, carrying
the producer's actual Integer table alongside its IdApi. Generated code's TCC
bridge names, Method ABI, Frame descriptors and durable image bytes are unchanged.

All legacy native consumers must be rebuilt for the current IdApi. Historical manifests are
retained as evidence, not compatibility shims. The independent C fixture checks
bare Id, arithmetic with a signed-128-bit oracle, errors, fresh identity, mixed
Float behavior, replacement, reassociation, namespace rebinding, and collection.

Float must be complete before `installFloat:`, or attached before legacy
IdApi initialization; see the [Float mounting protocol](float-v1.md).

The [nine-symbol export inventory](rust-integer-dylib-v1.json) includes the native
initializer and the legacy Error Value dependency. The independent
`scripts/id-native-loading-smoke.py PREFIX` fixture runs 605 arithmetic cases,
comparison, printing and error checks before Float exists, then repeats after
Runtime adoption with precise GC and early method replacement. Run
`mise run integer-error-value` and `scripts/integer-error-value-image-smoke.sh PREFIX`
for focused debug native and compiled-image checks.
