# Optional native Block construction declaration, v5

Design 81BH moves default selector enumeration out of IdApi. The current optional
producer export is:

```c
unsigned int birnam_native_block_construction_v5(
    const void *constructor, void *nil, const uintptr_t *context,
    void *block_vtable, void *code_object, size_t capture_count,
    void *materialization_owner, const unsigned char *program, size_t length,
    void *behavior_owner);
```

The caller must first prove, without effects, that the current Vtable receiver
uses the default lookup and exact default `ownerOf:` and `parentOf:` methods,
with no resumable replacements. It must prove the existing Block ancestry
condition and pass the current owner as `behavior_owner`. These are trusted
native caller obligations, not checks this producer delegates to user code.
The caller holds all operands live and permits no mutation between proving the
facts, querying and using the result. The floor's `BlockConstructionSummary`
performs this proof at every use; failure takes the ordinary constructor path.

`context` contains fourteen current floor values, in order: `alloc`, Namespace import `import`,
`objectPayloadSize`, `vtableOf`, `send`, Object Model `selector_at`, `symbolName`, `intern`,
`symbol_vt`, the canonical Namespace names `name_of` and `context`, `vtable_vt`, Namespace import context, and canonical enumeration context. Before calling any API entry, the
producer compares every value and nil with its initialized API and the live
selected Names, import and enumeration capabilities. The floor retains their original
operations and contexts separately; changing any of them must decline without
calling them. A wrapped entry
or different identity declines. Parent and owner functions are absent.

Zero declines; one identifies the ordinary constructor (null materialization
owner), and two the materialized constructor. The producer checks the supplied
CodeObject's exact program and capture count, the behavior's direct invocation
arity, and the existing two-Oop carrier's program and materialization owner.
A cold, missing or inconsistent carrier declines.

A successful warm operation allocates one fresh object with the supplied
behavior, storing the original supplied CodeObject followed by the original
captures. It installs no methods or owner association. The declaration remains
synchronous and has no Id allocation, mutation, send, user callback or publication.
Reverse export inspection does not intern names. The consumer still separately
proves allocation/GC eligibility, roots, current execution meaning and native
code lifetime; this declaration alone does not authorize allocation removal.

Discovery remains tied to the actual resident constructor image and its existing
pin. The private compiler-facing `birnam_id_warm_block_construction_v1` export
keeps its signature, supplies this new context and proof, and still declines
outside an active owner-thread entry. V1-v4 producer declarations are historical
and are not accepted as a fallback. Frame and image formats are unchanged.
