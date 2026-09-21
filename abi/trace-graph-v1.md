# Native live graph v1

`libbirnam_trace.dylib:birnam_native_module_v1` constructs a usable live graph
before Runtime, Block or compiler construction. The initializer takes four Word
operands: the completed root-registry `OwnedV1`, Lifetime `OwnedV1`, Object Model
`OwnedV1`, and Native Mark `ApiV1`. Native Mark itself loads through Id with no
arguments. Trace returns a Word pointing to the completed graph descriptor.

The [independent C header](fixtures/trace-graph-v1/trace_graph_v1.h) mirrors
[`trace_graph::GraphV1`](../rust/birnam-abi/src/trace_graph.rs): 56 bytes, with
version 1, byte size, context, roots, edges, validate and trace fields. Calls run
synchronously on the constructing thread. The native context and its predecessor
owners must remain live; production pins the context and code until process exit.
There is no Runtime pointer or provider slot filled by a later module.

- `roots(context, sink)` emits retained roots and Id's primordial roots, then
  invokes a snapshot of registered native root providers in registration order.
  Borrowed registry views are copied before callbacks.
- `edges(context, object, flags, sink)` emits current Id edges and Object Model's
  behavior-owner edge. For ordinary objects it scans payload words and invokes
  the current inherited native trace hook. Symbols have Id-owned byte payloads;
  their bytes are never conservatively interpreted as references.
- `CORE_ONLY` (1) limits an edge query to Id edges and the behavior owner. It
  supports the current upper collector's permanent-object root policy.
  `PRECISE` (2) selects only Oop slots when an exact layout exists; absent layouts
  retain the existing conservative behavior. With neither flag, scan every
  padded payload word. Hook selection remains live and cycle-bounded.
- `validate(context)` checks the allocation inventory, identities, Id/owner edge
  membership and exact payload-description coverage of nonempty ordinary
  objects. It invokes no native trace hooks, changes no marks and reclaims
  nothing. It returns `INCOMPLETE` for missing descriptions and `INVALID` for
  inconsistent facts. It is one input to future activation, not proof that all
  native modules have declared every reference they own.
- `trace(source)` is the existing [live walk](trace-live-v1.md). The caller owns
  mark admission and any additional roots; the graph owns representation edges.

Trace selects `rootsDo:`, `allocationsDo:`, `referencesOf:do:` and
`allocationSizeOf:` through Id lookup during construction. Later graph queries
invoke those exact native entries; they cannot accidentally invoke an allocating
application replacement during marking. No Id storage borrow crosses an upper
sink or native hook. Direct sink failures propagate; rejected native marks retain
Native Mark's abort behavior. Neither failure authorizes a sweep.

Production constructs the graph in `InitialFloor::initialize_trace`, passes the
same descriptor through the temporary `birnam_runtime_init_v7` bridge, and uses
it for native roots, payloads, hooks and owner edges in actual collection.
Collect now owns the [mark records and controls](collection-state-v1.md).
Runtime still supplies Execution roots, native register capture, its collection
source adapter and retirement policy. Explicit collector activation before Runtime remains pending.

`scripts/id-native-loading-smoke.py PREFIX` builds independent C fixtures against
the installed production dylibs. The standalone graph case has only the eighteen
required libraries available. The adoption case repeats edge checks after actual
precise collection with the same graph and Lifetime records.
