# TCC owns compiled Units

TCC accepts source/byte objects and returns Unit, Bytes or ordinary compileError
objects. Byte data is transferred through [bulk messages](../../docs/contracts/bytes.md).
Compilation and relocation are private libtcc operations. Mapping executable code,
creating Methods and installing them are distinct actions; see the
[runtime reference](../../docs/contracts/runtime.md#methods-and-definitions).

Compiled code exports a v2 or v3 Module descriptor. Unit `initialize` and
`initializeWith:` validate it before entry, construct its literal Symbols and
Methods, and pass Namespace plus the optional implementation as Oop arguments.
Unit `declaredMethodAt:` returns a declared Method by immediate index. Its native
implementation enters generated code directly, using a TCC-owned context that
keeps the Unit reachable. No Method application protocol is added.

A Unit can register a synchronous native reference entry by declaration index
using `traceRootsWithEntry:`. The declarations must be ready; invalid indexes and
continuation entries are rejected. Registration updates the existing Unit record,
without constructing a new behavior or changing binding epochs. GC invokes the
entry as implementation code for that Unit's native storage, with the visitor as
its argument. The entry reports Oops through `visit:`; native failures propagate.
The existing entry-depth guard keeps code mapped through visitor reentry.

Generated code uses this entry to report active call buffers. TCC does not interpret
their C representation, and the collector continues to obtain references through
the Unit owner's ordinary `referencesOf:do:` operation.

Existing compile/object-load, retained references, weak mapping identities,
repeated-initialization checks and checkpoint operations remain. Scoped references
replace per-temporary Roots tokens. Unit and generated entry context registries
are weak. GC removes their records and detaches mappings; actual unmapping waits
for Execution's checkpoint/retirementReady handshake and no active TCC entry.
Full integration checks exercise that handshake with Execution; isolated fixtures
alone do not establish execution or suspension coverage.

The notes below describe retained private compiler/mapping mechanisms. References
to the old v1 message ABI are superseded by the contract above.

On native macOS, the private compiler owner configures its fresh memory-output
states to use the running process's runtime when all undefined symbols are already
resolvable. It then avoids repeated SDK discovery, libc library searches and
support-archive reads. The check and actual relocation use the same live dynamic
linker lookup; no process-wide symbol/address cache is added.

A reserved runtime/compiler name, unresolved import, explicit library pragma, or runtime instrumentation/linking
option falls back to the existing SDK and library setup before relocation.
Compiler helpers supplied by `libtcc1` therefore remain supported, and a missing
requested library still fails even when none of its symbols are needed. Ordinary
libtcc clients keep their eager configuration and library-path ordering; this
optimization is private to Birnam's fresh states with default search paths.
Unit ownership, page protections and retirement still follow the same path.

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
