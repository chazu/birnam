# Raw namespace storage v1

> Historical contract; [v2](namespace-v2.md) adds names-service ownership in 81AX.

Design [81AW](../docs/design/81aw-production-namespace-state.md) moves the ordered
raw export table and address index from Id into `birnam-namespace`. It is separate
from the authored Globals store and System's operating-system Environment module.

The host opens `libbirnam_namespace.dylib` beside the selected floor, obtains
`birnam_namespace_provider_v1`, validates its descriptor and calls the floor's
`birnam_namespace_dependency_v1` before id_init. Provider discovery constructs no
objects or state and needs no IdApi. Each Runtime creates its own opaque state
at the original bootstrap point, after constructing heap/vtables/Symbols. Normal
module_init runs with the native stack and never resets state. All handles and
code stay pinned for every context's lifetime. The floor has no fallback loader
or normal Rust dependency on Namespace.

The immutable descriptor is 80 bytes on arm64: version at 0, byte size at 8,
then eight function pointers beginning at 16. All are required. Null, wrong
version/extent, missing operations and retargeting fail admission; repeated
attachment of the same descriptor succeeds. The [independent C mirror](fixtures/namespace-v1/namespace_v1.h)
checks both descriptor and view layouts.

| Operation | Existing behavior |
| --- | --- |
| create | Construct an independent empty namespace; return a nonnull opaque context |
| destroy | Release the context and its native storage in its owning module |
| bind(context, name, value) | Rebind the existing name slot or append a new slot; return value |
| find(context, name) | Return the current value, or raw null if absent |
| reverse(context, value) | Return the first currently matching name; raw null for a null value or no match |
| count(context) | Return the number of distinct name slots, including null-valued bindings |
| name_at(context, index) | Return the name at that insertion-order index; raw null past the end |
| bindings(context) | Return a borrowed array of ordered name/value pairs and its count |

Contexts are owner-serialized. Operations never dereference Oops or call Id,
allocate Id objects, send, collect or reenter. create/bind may allocate native
storage; reads do not mutate. The binding view expires on the next bind/destroy.
No Vec, HashMap or allocator ownership crosses the ABI. Id synchronously copies
both halves for its existing roots; the namespace's index itself adds no roots.

Id still performs its current owner/phase checks, Symbol interning and weak
internal-dictionary mirror updates. Importing an absent string name still interns
it; binding still updates the raw authority before the mirror. Reverse-name
bytes retain their Symbol lifetime. GC observation and callback-free Block
admission use the same selected state without adding language callbacks.

IdApi v7 and its existing public namespace entries remain unchanged consumers
in this state extraction. Their separate migration must account for live native
callback replacement, compiler context admission, reflection and image lookup.
No new language protocol, namespace aliasing, image format or loading policy is
introduced. The [library inventory](rust-namespace-dylib-v1.json) has three exports.

Run `mise run namespace` and `scripts/namespace-image-smoke.sh PREFIX` for focused
debug native and startup/compiled/image/baked checks. Existing native clients
must explicitly prepare the new dependency; historical IdApi fixtures remain
historical and are not claimed compatible with the current floor.
