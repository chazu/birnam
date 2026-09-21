# Native Integer dependency, v1

`libbirnam_integer.dylib` owns Integer's canonical behavior and prototype, signed
64-bit payload and primitive construction/reading/recognition. Its production
base is constructed through Id before Float, Word, Block or Runtime. Id boots
without Integer. Machine and compiler modules above Id receive the completed
Integer table as an explicit constructor input.

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
The native control extension installs `to:do:` after Runtime/Execution and
Block are complete, preserving earlier replacements. Bootstrap passes the
completed Integer table explicitly to upper constructors. The old provider,
dependency setter and aggregate `module_init` exports are removed.

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
Internal Rust test support retains direct construction; it is not an exported
initialization path. Both preserve the existing fresh-result contract. `read` requires a
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

Optimization requires exact admitted identities, context, code and codec.
Production message construction cannot attest the old direct allocator contract:
`allocation_context` and the optional [v4 declaration](native-numeric-semantics-v4.md)
still return zero. Its [v5 declaration](native-numeric-semantics-v5.md) identifies
the actual native function/context pair and describes its construction dependencies.
The compiler checks current lookup, allocation and Namespace operations before
omitting their dispatch. Replacements withdraw admission without being invoked
by the guard. Legacy direct construction retains its existing v4 checks.
The private closure-elision hook is now `birnam_id_try_closure_elision_v2`, carrying
the producer's actual Integer table alongside its IdApi. Generated code's TCC
bridge names, Method ABI, Frame descriptors and durable image bytes are unchanged.

The [export inventory](rust-integer-dylib-v1.json) describes the current native
entry and numeric admission declarations. Historical C clients that use removed
initializers require migration; they are not compatibility shims.
The independent `scripts/id-native-loading-smoke.py PREFIX` fixture runs 605
arithmetic cases, comparison, printing and error checks before Float exists,
then repeats after Runtime adoption with precise GC and early method replacement.
Float must be complete before `installFloat:`; see [Float construction](float-v1.md).
