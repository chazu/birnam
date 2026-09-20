# Vtable enumeration v1

Object Model owns native count and indexed selection. Id owns the ordered
registry, its mutation epoch and GC membership. Enumeration is live and unrooted:
count and each indexed read are separate observations, and an out-of-range index
returns null. A saved count does not freeze later reads. No send, allocation,
snapshot or additional root is introduced.

The [independent C mirror](fixtures/vtable-enumeration-v1/vtable_enumeration_v1.h)
describes four arm64 macOS layouts:

| Descriptor | Size | Fields after version/extent |
| --- | --- | --- |
| Factory | 32 | create at 16, destroy at 24 |
| Floor | 32 | context at 16, view at 24 |
| Capability | 40 | context at 16, count at 24, at at 32 |
| View | 16 | entries at 0, count at 8; no version/extent |

The host pins `libbirnam_object_model.dylib`, obtains its immutable
`birnam_vtable_enumeration_factory_v1`, and attaches it through Id's
`birnam_vtable_enumeration_dependency_v1` before `id_init`. Invalid factories or
retargeting reject without invocation; the same pointer may attach repeatedly.
This mounts only an inert factory. Object Model's ordinary method installer
still runs at its original construction point.

Creation copies a complete Floor descriptor without calling it. Missing floor,
wrong version/extent or absent view callback returns null. Context may be null
when the supplied callback permits it. Each Runtime owns its service allocation.
The floor view performs the original owner/observation admission and borrows
Id's current ordered array. Object Model copies its length or bounds-checked
identity immediately, without another callback. No array pointer is retained.
Roots callbacks retain their original ability to enumerate; storage-observation
callbacks retain their original refusal.

After initialization, Id publishes its service through
`birnam_vtable_enumeration_provider_v1`. Native initialization now receives
`(api, prior_state, presence, imports, exports, vtables)`. Store, Codegen, Tcc,
Alien and Image retain the matching live capability at their original API
selection points. Prelude retains it for Native Loader. Each operation copies
the selected function/context just before invocation; active calls do not
borrow the mutable descriptor across callbacks. Destroy the original owned
address, even if its public context or callbacks have been replaced. Descriptor
copies borrow the same allocation and do not extend its lifetime.

IdApi v12 has 28 fields / 224 bytes after removing `vtableCount` and `vtableAt`.
Default selector enumeration remains in Id. The [Native Loader v5 installer](native-loader-v5.md)
forwards the sixth argument; other preparation and installation contracts remain
unchanged. Native consumers must rebuild. Image encoding is unchanged.

See [81BG](../docs/design/81bg-production-vtable-enumeration.md) for the caller
audit. Run `mise run vtable-enumeration` and
`scripts/vtable-enumeration-image-smoke.sh PREFIX` for focused checks.
