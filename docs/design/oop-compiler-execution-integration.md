# Oop compiler and Execution integration

Latest checkpoint: the explicit resumable compiler is now connected to the retained
Machine engine. Source-level bounded pause/resume, GC, entered-code replacement,
current later sends, native/custom dispatch, and final code lifetime pass the native
gate. See the [resumable milestone](oop-resumable-milestone.md) and
[current receipt](../measurements/oop-resumable-20261023.json). The synchronous
checkpoint below remains part of that gate. Full language/control/image/worker
integration remains unfinished.

The Oop-only branch is the committed development direction. The work preserves
explicit language features and the Current Invariants; removing carrier machinery
must simplify responsibilities rather than add a replacement value system.

Status: the connected synchronous source-to-native path passes the native gate
in a separate build target. [Current receipt](../measurements/oop-connected-compiler-20261023.json).
TCC also retains its independent generated-code fixture.
[Receipt](../measurements/oop-tcc-integration-20261023.json). This is not full
compiler/Execution acceptance. The remaining integration is in progress.

## Existing contract and operation sequence

TCC owns compilation, relocation and eventual unmapping. Captured and entered
Methods and suspended/recovery frames retain their exact code owners. Execution
owns logical frames, scheduling, handlers, suspension, recovery and checkpoint
admission. Codegen owns lowering and generated source. Ordinary sends select the
current method and enter it without another lookup on the selected Method.

Before: generated initializer sends a NativeEntry plus Unit to Execution; Execution
constructs a Method and supplies a native entry to Id's dictionary. Invocation
adapts Operand/Outcome values. TCC detaches dead Units at GC and unmaps them only
after Execution's checkpoint and idle admission.

After: a compiled module declares Oop entrypoints in immutable object-file
metadata. TCC validates that descriptor before initializer entry, constructs
ordinary Methods, and supplies them to the initializer through its Unit's
`declaredMethodAt:` data operation. Construction does not pass code addresses in
messages. A dictionary contains a Method Oop. Id enters its declared native
implementation directly; TCC's private implementation enters the generated code
using its own checked entry context. No selector is looked up on the selected
Method and no application/forwarding protocol is installed.

The generated entry context contains a Unit edge and a private code address in
TCC-owned bookkeeping. Id sees an opaque context Oop and traces it through the
registered owner. TCC's weak context records are removed before their addresses
can be reused. Generated initializers receive the Unit as their owner; generated
entrypoints receive that same Unit as their context. These are explicit compiled
code contracts, not arbitrary foreign payload interpretation.

The new construction operation is on Unit, not on every send. The native entry
wrapper is required because Id's existing bootstrap Method owner admits static
native declarations, while TCC owns reclaimable mappings. It adds a private
function call and context-record read, not another language method lookup. Its
cost needs measurement once the full generated-code path works. No global code
address registry or permanent rooting of generated mappings is added.

## Features to retain

- Current lookup, custom lookup, DNU, replacement, captured methods and reflection.
- Language Errors as data, failed returns and control transfers as distinct cases.
- Synchronous and resumable generated functions, arity/captures and local Cells.
- Logical frame descriptors, tail calls, bounded drives, suspension, continuations,
  cancellation, handlers, cleanup, checkpoints, recovery and active generations.
- Portable program/admission, native candidates and existing optimization refusal
  semantics; removal of old carrier signatures does not remove genuine FFI.
- Source-free object loading, partial initializer publication, exact captured code
  lifetime and eventual mapping reclamation after the existing idle handshake.

## Validation boundaries

The TCC fixture must compile Oop C using the independent ABI header, invoke it
through production Id, collect inside entered code, remove bindings while keeping
captured code callable, and reclaim mappings after the last reference/checkpoint.
It must reject incompatible descriptors before initializer entry and prove no
lookup on the selected Method is required. An independent checkpoint fixture is
not proof that real Execution has been migrated.

That initial checkpoint has since been extended by the connected synchronous and
bounded-resumable milestones. Complete language/control and default compiler
acceptance remain separate; the native closure gate is not a working-language
qualification.

## Frame engine checkpoint

The production frame/continuation engine now carries opaque Method references
with allocation identities instead of native function/context pairs. Its pending
registration map uses the identity, preserving distinction when an address is
reused before checkpoint. Native failures carry ordinary caught error references.
The transition, scheduling, tail-entry, recovery and descriptor algorithms remain
unchanged. Their existing assertions are retained; inert test entries are opaque
identities because the pure engine never enters them.

At historical checkpoint `4b5e8a00`, `scripts/oop-execution-engine-test.sh`
compiled these production modules in a standalone harness while the message-facing
Execution module still used the removed ABI. The connected milestone builds and
tests the actual Execution crate; that temporary harness has been removed. The message-facing owner must obtain and retain the exact Method
identity and trace its step/implementation Methods when constructing registrations.
It must not manufacture keys from addresses alone or revive raw native entries.

The core Method/Machine entrypoints and step emission are now connected for the
bounded milestone. Remaining work includes portable/named/optimized consumers,
full control/recovery and default compiler/bootstrap orchestration, followed by
the complete language/stackless preservation gates.

## Connected synchronous milestone

The explicit `Codegen compileSynchronous:` acceptance path parses real source with the retained grammar/PEG owner,
lowers its CST with the existing compiler, emits synchronous Oop C, compiles it
through real TCC, and invokes an Execution-owned Block. This is deliberately the
synchronous integration milestone; it does not qualify resumable code or replace
the remaining language gates.

Before, Execution wrapped native entry/context carriers in another Method and
adapted every call/result. For synchronous functions, TCC already supplies ordinary
Methods with the required mapping lifetime. Execution can retain those Methods in
its Block payloads directly. Block value sends enter Execution's native Block
implementation and use the existing explicit captured invocation operation. Id
itself still enters every selected Method without a lookup on that Method.
The existing Objects storage bookkeeping validates Execution-owned payloads; no
foreign payload interpretation or generic payload catalog is added.

Generated selector names are immutable module literals, interned once by the
current loader operations. Actual dependency sends still perform current lookup.
Generated temporary roots remain explicit. Cleanup changes from a byte buffer of
raw root tokens to `Roots releaseTokensProtecting` with a protected result/error
and Oop tokens as arguments. That owner operation calls current `release:` for
each token; the entered call protects the result through collecting replacements.
No managed argument container or per-value carrier is needed for cleanup.

The old Execution message facade is kept as pending migration source in
`pending_message_integration.rs`; it is not compiled as an alternate runtime or
compatibility path. The retained frame/continuation engine continues to compile
and its tests remain mandatory. At the synchronous checkpoint the Machine/control/recovery facade remained
pending. The resumable report now records the bounded Machine subset; full
control/recovery is still pending.

The synchronous source entry is explicit. Ordinary `compile:` orchestration
remains pending; the later `compileResumable:` entry retains logical frames.
Neither entry silently changes the other scheduling contract. NativeBackend likewise exposes `emitSynchronous:`
for this path. No existing suspension/continuation implementation is replaced by
the synchronous emitter. Full compiler promotion still requires reconnecting the
retained Machine/control/recovery facade and its existing acceptance tests.

### Acceptance exercised

The expanded native gate uses the actual Lang seed, PEG, compiler lowering,
NativeBackend, TCC and Execution. It checks arithmetic, full-width Integers,
lexical closures/captures, top-level let publication, collection inside generated
code, dependency replacement, ordinary Error data versus failed returns, and
collection from replaced temporary-root cleanup. A compiled Method remains
callable when all lookup on that Method fails. Another entered compiled Method
removes its own binding, collects and checkpoints, then finishes successfully.

An isolated lifetime check retains only the returned closure, verifies exactly
one additional live mapping, releases that closure and confirms the mapping count
returns to its baseline after real Execution/TCC checkpoint. The former generated
checkpoint stand-in has been removed from the TCC acceptance driver.

No full-language, resumable emission, authored Error/Lang, artifact/image, worker,
installed-candidate or Logue performance acceptance is claimed. The Error provider
in this native test remains an independent fixture. The ordinary compiler and
resumable feature surfaces are pending, not silently replaced by synchronous ones.

Explicit reflection retains fresh capture identity. `methodFor:` previously did
current lookup followed by construction of a captured Execution Method. It now
does current lookup followed by `captured` on the selected Method. The bootstrap
Method owner implements that explicit construction by copying its own declared
entry index, context and owner into a fresh Method. No payload crosses owners,
no extra per-Method table is introduced, and ordinary sends do not use this
operation. The negative direct-send contract remains mandatory. Behavior receivers
retain the existing `methodFor:` interpretation: capture their instance binding,
not a method on their metavtable.

The real Execution crate now builds, so its retained engine tests run through
ordinary `cargo test` in the native gate. The temporary standalone engine harness
is removed. The synchronous owner reads a single Block field for invocation and
capture access; only clone/tracing take complete field snapshots. Generated
helpers remain `static inline`, allowing TinyCC to omit unused helper code.

Reproduce the complete current native milestone gate from this worktree:

```sh
BIRNAM_OOP_TARGET="$PWD/.cargo-target/oop-connected-fresh" \
  bash scripts/oop-id-slice-test.sh
```

The gate includes 71 pure compiler tests and all 22 retained frame-engine tests,
in addition to the native owner tests and source-to-execution integration. Each
check retains its scope: the native gate is not full-workspace acceptance.
