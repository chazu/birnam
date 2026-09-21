# Runtime construction v15

`birnam_runtime_init_v15` is the temporary upper constructor used by the
native entry of `libbirnam_runtime.dylib` while Design 83's Execution migration continues.
Version 15 adds completed Integer, Float, Word and String operations as an explicit input.
The host no longer fills Runtime dependency globals before construction. The Id native carrier remains version 1;
version 14 of this constructor is no longer exported.

The arguments, in order, are pointers to:

1. `id_protocol::Protocol`: borrowed Id carrier, retained by the caller for
   Runtime's lifetime; its context is opaque to Runtime.
2. `root_registry::OwnedV1`: transferred root registry owner.
3. `lifetime::OwnedV2`: transferred Lifetime owner, including complete
   [native reference records](fixtures/lifetime-v2/README.md).
4. `base_values::ValuesV1`: borrowed completed Nil/Boolean identities.
5. `namespace_owner::OwnedV1`: transferred Namespace owner.
6. `object_model::OwnedV1`: transferred Object Model owner.
7. `trace_graph::GraphV2`: borrowed completed Trace graph.
8. `collection_state::OwnedV2`: transferred Collection owner.
9. `id_primitives::PrimitivesV1`: borrowed completed original Id entries,
   selected immediately after loading Id and before loading upper modules. See
   [retained entries](id-primitives-v1.md) for layout and semantics.
10. `runtime_support::FactoriesV1`: copied complete method-observation,
    storage-observation, deferred-identity, deferred-index, dispatch-cache and
    guard-verdict factories. See [factory loading](runtime-support-v1.md).

11. `runtime_values::ApisV1`: borrowed live value tables, validated before
    adoption and attached before Runtime publishes Machine/Block or its collector
    extension. The 48-byte descriptor has `version: u32`, `byte_size: usize`,
    followed by Integer, Float, Word and String API pointers in that order.
    Existing value module entries later reuse those same tables.

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

The loaded Bootstrap module invokes Runtime through Id's native module loader. The loader supplies
argument 1 as the native carrier; the other ten pointers are explicit `Word`
initializer operands in the order above. The native entry rejects wrong counts,
operand kinds and invalid descriptors before adopting owners, and returns the
`IdApi` address as a `Word` on success.

Id is now independently packaged with only `birnam-abi` as a Cargo dependency.
Base Runtime constructs native Execution and Block with their completed value
inputs. It also constructs owned Method reflection and publishes the completed
[checked language-send entry](language-send-v1.md) before language executors load.
Compiler admission/catalogs and numeric/native optimization registries
remain absent until the explicit [compiler extension](runtime-compiler-v1.md)
loads. Both entries share the Runtime image; the base does not require Codegen
or its module initialization. All six supporting factories are loaded through Id
before Runtime itself is opened.

After construction, the same native descriptor accepts two-operand requests:
`Bytes("namespacePresence"), Word(runtime_api)` and
`Bytes("namespaceImport"), Word(runtime_api)`. Each returns a Word pointing to
Runtime's already completed namespace view, using the existing presence/import
ABI. A null or different Runtime pointer, or a mismatched Id carrier, returns
InvalidRequest. These queries neither install compiler services nor change
ownership. Bootstrap supplies these live views to later control constructors.

The same completed-service request form also supports `namespaceExport`,
`vtables` and `methodObservation`, returning the existing namespace-export v1,
vtable-enumeration v2 and method-observation v1 descriptors respectively. These
are inputs to collection/Store construction; they neither initialize a module
nor introduce a later-filled provider slot.

`namespaceNames` is another completed service selected by the common native
request `(Bytes("namespaceNames"), Word(Runtime API))`. It returns the existing
namespace-name API v1 used by Portable Code and Codegen, with the same Runtime/Id
identity checks as the other namespace views.
