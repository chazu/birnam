# Retained Id entries v1

Bootstrap selects these entries through ordinary Id messages immediately after
Siphon returns the completed Id carrier, before loading any upper module. Runtime
receives the complete record in its temporary [v13 constructor](runtime-construction-v13.md).
This replaces the allocation-only descriptor; it adds no Id selector, export,
callback slot or knowledge of an upper service.

```c
struct IdPrimitivesV1 {
    uint32_t version;                    /* 1, offset 0 */
    size_t byte_size;                    /* 144, offset 8 */
    BirnamNativeEntry allocate;          /* offset 16 */
    BirnamNativeEntry intern;            /* offset 32 */
    BirnamNativeEntry lookup;            /* offset 48 */
    BirnamNativeEntry add_method;        /* offset 64 */
    BirnamNativeEntry remove_method;     /* offset 80 */
    BirnamNativeEntry allocation_size;   /* offset 96 */
    BirnamNativeEntry behavior_of;       /* offset 112 */
    BirnamNativeEntry symbol_bytes;      /* offset 128 */
};
```

Entries come from `allocate`, the carrier's interning selector, `lookup`,
`addMethod`, `removeMethod:`, `allocationSizeOf:`, `behaviorOf:` and
`symbolBytesOf:`, respectively. Each is a native function/context pair. The caller
retains Id and the code/contexts for Runtime's lifetime. Validation checks the
aligned header and exact version/extent before requiring all eight functions.

The capture point matters: sampling current bindings after other modules have
loaded cannot establish original primitive semantics. Trusted Bootstrap supplies
this record from the initial Id graph. Lookup and numeric admission compare
complete entries against it; a replacement remains observable even when installed
before Runtime construction. Removing and restoring bindings recovers the same
original entries. These checks do not seal methods or veto replacement.

Allocation and interning entries retain their non-collecting contract for
deferred materialization. Ordinary upper allocation still performs accounting,
rooting and collection around them. The other entries identify Id's native
dictionary operations and read-only object/Symbol facts without referring to
private Rust functions or casting Id's context.

The temporary legacy bridge uses these entries to recognize operand/result
conventions. Exact matches distinguish implementations sharing a function but
using different contexts. A saved entry from another Id may use an unambiguous
shared function convention, but invocation always preserves its own context.
Such recognition never authorizes a compiler optimization: those guards require
the complete expected entry. Arbitrary native results still require a checked
language wrapper; this descriptor does not complete owned Method reflection.
