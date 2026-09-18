# Optional native Block construction declaration, v1

> Historical contract; [v2](native-block-construction-v2.md) replaces it in 81K.

This optional Portable Code native export describes the successful warm path of
an exact constructor. It changes no IdApi field, Method signature, canonical
descriptor or image encoding. Unknown constructors remain ordinary operations.

```c
unsigned int birnam_native_block_construction_v1(
    const void *constructor, void *nil, const uintptr_t *context,
    void *block_vtable, void *code_object, size_t capture_count,
    void *materialization_owner, const unsigned char *program, size_t length);
```

`context` contains ten current floor values, in order: `alloc`, `import`,
`objectPayloadSize`, `vtableOf`, `vtableParent`, `vtableSelectorAt`, `symbolName`,
`vtableOwner`, `symbol_vt`, and `exportedNameOf`. The producer compares them and `nil` with its
current initialized IdApi before calling any API entry. Wrappers or another
floor cause refusal, including a changed allocator. The caller supplies valid,
live objects and canonical program bytes, owns the constructor's native image,
and permits no concurrent mutation while querying or using the result.

Zero declines. One identifies the ordinary Portable Block constructor, requiring
a null materialization owner. Two identifies the materialized constructor. Both
codes attest the following current state:

- The supplied CodeObject has the exact supplied program and capture count.
- The vtable inherits from the current Block family and has the invocation
  arity required by the constructor. This does not attest the invoked Method.
- The existing vtable owner is the current two-Oop Block carrier, whose
  CodeObject has the same program and whose materialization owner is exactly
  the supplied owner. A cold or missing carrier declines.
- The successful operation allocates one fresh object of `(capture_count + 1)`
  Oop words with that vtable, storing the original supplied CodeObject followed
  by the original captures in order. It does not install methods or ownership.

The query is synchronous and has no Id allocation, mutation, sends, callbacks or
publication. Reverse export inspection avoids interning even absent names;
multiply named exports may conservatively decline if another alias is returned
first. It
does not claim that the allocation is removable: the consumer must separately
prove layout, GC/finalizer and accounting eligibility, complete roots, current
invocation/Frame control meaning, and native generation lifetime. It must also
preserve the original CodeObject cache lookup and vtable resolution before
querying, and reconstruct the fresh Block before any identity observation.

The consumer discovers this declaration in the actual resident image containing
the exact constructor and retains an image pin; a similarly named export in a
dependency supplies no authority. Requery current state at each admitted use.
Producer metadata is an unsafe native implementation contract, not a semantic
assertion that arbitrary Id data or selector spelling can grant.

The floor provides the private compiler-facing query
`birnam_id_warm_block_construction_v1(constructor, block_vtable, code_object,
capture_count, materialization_owner, program, length, materialized)`. Its last
argument is zero for the ordinary constructor and one for the materialized
constructor; other values decline. It supplies its own current floor context,
discovers and pins the actual constructor image, and returns one only for a
matching declaration, otherwise zero. This is a native export, not an IdApi
field. The result supplies no authority to skip allocation or invocation.
It declines outside an existing owner-checked runtime entry; a generated PC or
ordinary native Method supplies that dynamic scope.
