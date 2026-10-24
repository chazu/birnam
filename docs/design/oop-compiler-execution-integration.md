# Oop compiler and Execution integration

The Oop-only branch is the committed development direction. The work preserves
explicit language features and the Current Invariants; removing carrier machinery
must simplify responsibilities rather than add a replacement value system.

Status: TCC and its independent generated-code fixture pass the native gate.
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

Next integrate generated entry declaration emission and real Execution together,
retaining the frame engine and changing its message-facing owners. Port the real
language seed and Codegen consumers, then run the existing language and stackless
preservation tests. Do not claim a working language from the native closure gate.

## Frame engine checkpoint

The production frame/continuation engine now carries opaque Method references
with allocation identities instead of native function/context pairs. Its pending
registration map uses the identity, preserving distinction when an address is
reused before checkpoint. Native failures carry ordinary caught error references.
The transition, scheduling, tail-entry, recovery and descriptor algorithms remain
unchanged. Their existing assertions are retained; inert test entries are opaque
identities because the pure engine never enters them.

`bash scripts/oop-execution-engine-test.sh` compiles these production modules in a
standalone test harness while the message-facing Execution module still uses the
removed ABI. This is an intermediate source migration, not a build of the entire
Execution crate. The message-facing owner must obtain and retain the exact Method
identity and trace its step/implementation Methods when constructing registrations.
It must not manufacture keys from addresses alone or revive raw native entries.

Remaining connected work: Execution's message entrypoints and constructors,
generated C and descriptors, portable code, Codegen and the language seed, followed
by the real language/stackless tests. The new TCC and pure engine gates do not
replace any of those acceptance requirements.
