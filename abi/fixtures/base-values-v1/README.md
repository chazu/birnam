# Base values v1

`base_values_v1.h` is the independent arm64 C layout for the completed Nil and
Boolean identities (56 bytes). The Rust mirror is
[`base_values.rs`](../../../rust/birnam-abi/src/base_values.rs).

The production `libbirnam_boolean` native initializer receives Words pointing to
completed Root Registry and Lifetime owners. It constructs two behaviors and
three zero-payload objects through Id messages, declares their layouts through
Lifetime, and registers roots before publishing any references in module state.
It returns a Word pointing to `BaseValuesV1`. The descriptor and method contexts
are owned by the pinned base module; these five canonical references remain
registered for its process lifetime.

The production `libbirnam_nil` native initializer takes that completed descriptor
as a Word and returns Unit after installing predicates. Neither base entry uses
String, Integer, Error, Block or Runtime. Repeated initialization with the same
inputs returns the existing result without reinstalling methods. Missing or
incorrect operand kinds return native InvalidRequest.

Available operations are Object `==`, Boolean `not`, `&`, `|`, and Object/Nil
`isNil`, `notNil`. They return the canonical objects. Nil predicates use its
behavior, so other instances of Nil behave consistently. Native arity/type
failures return InvalidRequest. Printing and Block control are absent here.

After String exists, the native `installPrinting:` extension is sent to nil and
true with two Words: the completed roots and String descriptors. Each module
creates a separate printing context and installs its existing `printString`
behavior using current String `fromBytes:` messages. Cached selectors and
predecessor references are rooted. Existing printing replacements are preserved.
This works before Block/Runtime and does not put a future String slot in either
base owner. Late `module_init` preserves native printing and predicates; Boolean
Block control still mounts later with Execution.

The temporary upper Runtime constructor `birnam_runtime_init_v13` takes completed
roots, Lifetime, this descriptor, Namespace, Object Model, the live Trace graph
and collection state. It adopts the same identities and layouts.
This replaces the intermediate v1/v2/v3/v4/v5/v6/v7 Runtime bridges; legacy `id_init` remains
only for consumers still awaiting migration. This constructor is temporarily
packaged with Runtime in Id and is not part of Id's minimal protocol.

`scripts/id-native-loading-smoke.py PREFIX` exercises the actual installed entries
through an independent C consumer with all successor libraries absent, then
checks the identities, layouts and exact basic bindings after normal startup
and precise GC. Boolean/Nil image smoke scripts cover later printing, control,
method replacement, compiled execution and image restoration.
