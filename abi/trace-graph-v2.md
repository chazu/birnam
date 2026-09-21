# Native live graph v2

Trace v2 adds diagnostic inspection to the completed live graph. Its initializer
still accepts Id and four completed predecessors: Root Registry, Lifetime, Object
Model and Native Mark. It returns a Word pointing to the 64-byte `GraphV2`.
There is no Runtime, compiler, allocation-registry borrow or later provider slot.
The [C mirror](fixtures/trace-graph-v2/trace_graph_v2.h) fixes the graph and snapshot
layouts independently of Rust.

The roots, edges and trace operations retain their [v1 contracts](trace-graph-v1.md).
Validation accepts an exact layout or an effective native trace hook for a
nonempty ordinary payload, inspecting declarations without invoking the hook.
Production Collect activates before Runtime and uses this graph throughout.

`inspect(context, visitor_context, visitor)` supplies one callback-scoped
`SnapshotV1`. Its scratch arrays contain:

- Allocations, sorted by stable identity: object, requested and padded payload
  sizes, total allocation bytes, behavior, core-representation/layout/finalizer
  flags and the effective native trace hook.
- Id's primordial roots and the live Symbol objects, reported separately so an
  upper compatibility policy can retain Symbols without reading Id's interner.
- Id-owned edges and Object Model's behavior-owner edges. Payload words are not
  read; native root providers, trace hooks and finalizers are not invoked.

Trace obtains storage facts through the exact Id operations selected during its
construction. Later application replacements are still visible to ordinary sends
but cannot turn this inspection into an allocating application callback. Lifetime
descriptions and hook inheritance are read afresh on each inspection. Allocation
and descriptor borrows end before the visitor receives the scratch arrays.
The visitor must not retain arrays or reclaim/mutate the observed heap. The
caller establishes that exclusion; inspection creates no new execution or GC
phase. A missing visitor returns `INVALID`, and visitor status propagates unchanged.

Permanence, execution roots, deferred allocations, observation identities and
coverage-gap policy remain upper responsibilities. Runtime's protected storage
view copies this snapshot before joining it with Collect's permanence records,
its own roots and journals, and the registered provider declarations. That path
does not read Id's allocation records, headers, Symbol classification or behavior
dictionaries. The legacy local constructor still uses its previous inventory
until the remaining aggregate constructor/package migration is removed.

The installed native-loading harness checks inspection with only the eighteen
graph predecessors present and repeats it after Runtime adoption and collection.
Its independent C fixture checks extents, identities, Symbol classification,
owner edges, inherited hooks and finalizer declarations, and verifies that no
hook/provider/finalizer runs. It also checks missing visitors, exact failure
propagation and inspection after Id fact-method replacement. The existing storage
observation image workload exercises the production host's admitted view in
source, compiled, mapped, rebuilt and baked execution.
