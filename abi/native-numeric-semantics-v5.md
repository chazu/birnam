> Historical optimization-admission evidence. These descriptor-specific exports
> are retired. Production Integer now uses Id messages; Execution must prove
> their effects before enabling optimizations. The recovery/deferred-allocation
> algorithms remain preserved for that integration.

# Native numeric implementation declarations, v5

Production Integer's optional `birnam_native_numeric_semantics_v5` query accepts
the selected native function/context pair and the consumer's admitted
`IntegerApiV1`. It returns the same successful signed-64-bit arithmetic codes as
[v4](native-numeric-semantics-v4.md): add 1, subtract 2, multiply 3, divide 4,
remainder 5. Zero refuses admission. The base entries and the completed mixed
Float extension declare their exact-Integer cases; an arbitrary context paired
with a recognized function is not a recognized implementation.

Queries run on the owning thread with live arguments. The query is pure: no
messages, allocation, collection, callbacks or mutation.
It compares the live Integer operations against their original implementation,
identities, context and codec. The caller obtains the declaration from the image
containing the selected function and retains that image while using the summary.
Compiler bindings and predictions retain and compare both function and context.
This is an optional trusted native implementation contract, not language-level
authority granted by a selector name. Unknown native implementations continue
through ordinary dispatch. The legacy Method path continues to use v4.

## Construction and dependency admission

`birnam_integer_construction_v1(const IntegerApiV1 *, ConstructionV1 *)` is a pure
query describing the immutable dependencies actually used by the original
Integer implementation. It rejects changed operation tables. Its 184-byte result
contains the Id protocol, retained allocation-size and behavior entries, the
allocation selector, Namespace receiver and binding selector, ErrorVt Symbol,
and root registry owner. This description works before Runtime or a collector
exists; it supplies no future provider slot and does not enable optimization.
The [independent C mirror](fixtures/integer-native-v1/integer_native_v1.h)
specifies its layout.

The compiler separately proves, on every use, that the current Integer table
matches its admitted table, the Id and root owners match this Runtime, retained
facts are Id's original pure facts, and lookup and the selected allocator are
canonical. Thus replacing an allocator or lookup cannot have its effects silently
omitted by unboxing, deferred allocation or constructor elision. The original
direct-allocator query remains zero for native construction; no message path is
misrepresented as a legacy function address.

Arithmetic additionally checks the current Namespace binding method and the
current Id intern:/symbolBytesOf: methods. Namespace's optional pure
`birnam_namespace_lookup_semantics_v1(entry, owner, id)` declaration identifies
its actual binding lookup and storage. That lookup validates the already-existing
Symbol through Id's current operations before reading storage. The compiler
requires the known Symbol and canonical operations, including ordinary lookup;
it never invokes a changed operation to ask whether it has effects. Actual
Namespace contents remain live: rebinding ErrorVt to Integer still propagates the
argument through the ordinary successful-operation rule.

The current Runtime allocator is an explicit upper extension. These checks belong
to the compiler/Execution implementation and must move with it when those owners
leave the Id package. They do not add compiler rules or dependencies to the
independently constructed Id carrier. GC activation and the final package split
remain separate Design 83 requirements.

## Validation

`python3 scripts/id-native-loading-smoke.py PREFIX` checks the independent C
layout and declarations against the actual native libraries before and after
Runtime adoption, including wrong contexts and changed operation tables.
`scripts/native-integer-admission-smoke.sh PREFIX` compares all observable counted
effects with optimization disabled/enabled while replacing allocation, Namespace
binding lookup, interning, Symbol validation, lookup, owner/parent queries and
arithmetic. It also requires closure admission to resume after each restoration.
