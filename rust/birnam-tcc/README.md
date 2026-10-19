# TCC owns compiled Units

Bootstrap loads `libbirnam_tcc` with Namespace after Execution and parsing. The
module owns libtcc compilation and every relocated native mapping. The private
`birnam-native-compiler::Unit` serializes compiler operations; no compiler lock or
registry borrow survives a native initializer or language callback.

| Receiver / selector | Input | Result |
|---|---|---|
| TCC `compile:` | Trusted generated C String | Unit, or compileError. |
| TCC `compileObject:` | Trusted generated C String | Bytes containing a relocatable object, or compileError. |
| TCC `loadObject:` | Trusted relocatable ELF object Bytes | Unit, or compileError; no C compilation or temporary object files. |
| Unit `initialize` | none | Captured result of the unit's existing native Module initializer. |
| Unit `initializeWith:` | Implementation Object | Same initializer with an additional rooted operand. |
| Unit `retain:` | Object | Same Object, traced whenever the Unit is live. Used for generated Methods and native-context references. |
| TCC `checkpoint` | none | Unit; drain detached mappings after Execution's checkpoint and idle admission. |
| TCC `liveNativeUnits` | none | Native Word counting live and pending-retirement mappings. |
| TCC `liveNativeBytes` | none | Native Word from libtcc's relocated-allocation accounting; available with `resource-metrics`. |

Each compilation has its own initializer and static native context. It exports
`birnam_native_module_v1` using the existing Id ABI: Protocol plus Namespace and
the owning Unit as Object operands, with the optional implementation Object last.
No service addresses travel in Words/Bytes. A successful initializer's result and
implementation are retained by the Unit. Repeating initialization returns the
captured result; changing the optional implementation is rejected. Initializers
must report other Objects retained in native contexts through Unit `retain:`.

Object loading uses libtcc's bounded memory input, sharing section merging and
relocation with its file loader. The current `bytes` message supplies a borrowed
view; it is consumed synchronously before any further Id send. Retained code,
data and symbols belong to the new Unit, independently of the input buffer and
of other Units loaded from identical bytes. This changes neither artifact bytes
nor the Id-facing protocol. General library/archive file loading remains a
libtcc facility; `loadObject:` is the single relocatable-object route.

`compileObject:` still uses libtcc's file-output API. Its output directory is
needed because the writer unlinks/recreates its destination. Loading existing
compiled code no longer takes that detour. See the
[startup/runtime audit](../../docs/design/startup-runtime-audit-20261019.md).

Generated code constructs Execution Methods with its Unit as opaque code owner.
Binding → Method → Unit and entered/suspended/recovery frame edges keep actual code
alive. Unit → generated Methods cycles remain collectable because the native Unit
registry is weak. GC removes metadata and detaches the native Unit; it never calls
language cleanup or unmaps code from a finalizer. An idle checkpoint drops detached
private Units after Execution retires their registrations.

The old function-address owner registration, floor-symbol retirement cache and
Unit-address-derived vtable cache are deleted. Source-free materialization and
[Image/Store orchestration](../birnam-image/README.md) use these same owners;
allocation-elision is not restored here.

`mise run native-messages` enables resource metrics and proves binding-only,
captured, entered, suspended and recovery lifetimes with real TCC mappings. Both
Unit count and libtcc relocated bytes return to zero after final release. The C
recovery candidate uses an independent ABI fixture; it tests compiled mapping
lifetime, not the deferred optimizer emitters.

On macOS, each relocated unit owns a dedicated anonymous VM mapping. The mapping
starts read/write; the existing relocation pass applies the usual code/data page
protections before execution. The existing last-owner/checkpoint path releases
it with `munmap`. This avoids placing protected executable pages inside malloc's
shared regions and removes the extra malloc-alignment page per unit. Empty units
still receive one page. Owner edges, initializers, dispatch and recovery are
unchanged. Other platforms retain their existing allocation paths.

`allocation-metrics` reports TinyCC's requested malloc storage. On macOS that
excludes the dedicated relocation mappings; `liveNativeBytes` still includes
those mappings. RSS, physical footprint, allocator capacity and these live-owner
counters measure different things. Use `scripts/retained-memory.py` for a macOS
post-GC census. Kernel-backed regression checks cover RX code, RW data, execution
after cache-owner release and unmapping after the final owner goes away.
