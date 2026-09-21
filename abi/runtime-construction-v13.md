# Runtime construction v13

`birnam_runtime_init_v13` is the temporary upper constructor used by the
native entry of `libbirnam_runtime.dylib` while Design 83's Execution migration continues.
Version 13 replaces the allocation-only descriptor with the original Id entries
retained before upper modules load. The Id native carrier remains version 1;
version 12 of this constructor is no longer exported.

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
9. `id_primitives::PrimitivesV1`: borrowed completed original Id entries,
   selected immediately after loading Id and before loading upper modules. See
   [retained entries](id-primitives-v1.md) for layout and semantics.

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
Id `State` box. Only explicit `ffi-test-hooks` builds of Runtime retain `id_init` for diagnostic
fixtures. Normal Id and Runtime builds do not export that entry.

The host invokes Runtime through Id's native module loader. The loader supplies
argument 1 as the native carrier; the other eight pointers are explicit `Word`
initializer operands in the order above. The native entry rejects wrong counts,
operand kinds and invalid descriptors before adopting owners, and returns the
`IdApi` address as a `Word` on success.

Id is now independently packaged with only `birnam-abi` as a Cargo dependency.
This constructor does not establish the final Execution base: compiler code and
remaining legacy factory preparation still reside in the upper Runtime.
