# Runtime construction v12

`birnam_runtime_init_v12` is the temporary upper constructor used by the
production host while Design 83's Execution and package migration continues.
Version 12 adds the completed Id carrier as the first argument. The Id native
carrier remains version 1; version 11 of this constructor is no longer exported.

The arguments, in order, are pointers to:

1. `id_protocol::Protocol`: borrowed Id carrier, retained by the caller for
   Runtime's lifetime; its context is opaque to Runtime.
2. `root_registry::OwnedV1`: transferred root registry owner.
3. `lifetime::OwnedV1`: transferred Lifetime owner.
4. `base_values::ValuesV1`: borrowed completed Nil/Boolean identities.
5. `namespace_owner::OwnedV1`: transferred Namespace owner.
6. `object_model::OwnedV1`: transferred Object Model owner.
7. `trace_graph::GraphV2`: borrowed completed Trace graph.
8. `collection_state::OwnedV2`: transferred Collection owner.
9. `primitive_allocation::PrimitivesV1`: borrowed completed allocation/intern
   entries selected before collecting replacements.

The result is the existing `IdApi` pointer. Header/extent validation happens
before adopting any transferred owner; invalid descriptors return null. A valid
caller supplies coherent, completed predecessors from the same construction and
retains the code/contexts of every borrowed descriptor. All calls use the owning
thread. Runtime owns its one-shot initialization guard and rejects a repeated
construction before querying or mutating Id. No descriptor has a future-filled
operation slot.

Runtime obtains Object/Symbol prototypes, their behaviors and dispatch selectors
through Id messages. Each returned object is retained in the completed root
registry before the next query. Current interning preserves collecting allocation
accounting. Runtime subsequently uses its existing construction roots through
collector-extension installation and releases temporary roots afterward.

Id keeps its own production state for process life, matching primitive native
image pinning. Runtime neither takes an `InitialObjects` Rust value nor owns an
Id `State` box. The legacy `id_init` compatibility entry obtains a carrier and
calls this same construction path; it is still scheduled for removal.

This declaration does not establish the final Execution entry or independent
Id package: Runtime/compiler code and the remaining legacy interfaces must still
leave `birnam-id` under Design 83.
