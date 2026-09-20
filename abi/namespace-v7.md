# Namespace factory v7

The selected floor requires `birnam_namespace_dependency_v7` before `id_init`.
The host pins `libbirnam_namespace.dylib` and attaches
`birnam_namespace_provider_v7`'s immutable 160-byte descriptor. All eighteen
operations are required. Invalid descriptors and retargeting reject; repeating
the same attachment succeeds. Historical factories are not fallbacks. The
[independent C mirror](fixtures/namespace-v7/namespace_v7.h) defines its layout.

The first sixteen operations retain [v6](namespace-v6.md)'s offsets and
contracts. `create_import` at offset 144 and `destroy_import` at 152 create and
destroy a runtime-owned [import service](namespace-import-v1.md):

```c
NamespaceImportV1 *create_import(const void *storage, FindV1 find,
                               LookupV1 lookup, const NamespaceImportFloorV1 *floor);
void destroy_import(NamespaceImportV1 *owned_service);
```

Creation copies the immutable 40-byte FloorV1 descriptor: version, size,
context, access and interner callbacks. It retains the selected storage,
find and lookup operations. Null storage, missing operations, missing floor,
wrong floor version/size or missing floor callbacks return null without invoking
callbacks. The opaque floor context may be null if its callbacks permit it.
The caller pins all selected dependencies until destruction.

Canonical import diagnoses null names, scans through the first NUL, and then
performs Id owner/observation admission through `access`. Its transient result
is supplied as interner context to the selected lookup. Missing names are
interned before raw find; empty/non-UTF8 names, original Symbol identity, nil
and raw-null bindings are preserved. Existing fatal diagnostics and their
precedence remain unchanged. Restricted interning and floor admission stay in
Id; no RuntimeState reference spans a callback.

Each Runtime owns separate live and canonical import selections. A live
operation/context replacement changes calls without changing ownership:
`destroy_import` requires the original owned service address, never a copied
public descriptor. Copied descriptors do not keep the allocation alive. The
floor publishes its live descriptor through
`birnam_namespace_import_provider_v1` after `id_init`; native consumers receive
that selection explicitly with their matching API. IdApi v10 removes `import`.
Native ABI consumers require rebuilding; language behavior and image encoding
are unchanged.
