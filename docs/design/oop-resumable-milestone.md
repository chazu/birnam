# Connected Oop resumable execution

**Current hold:** the expanded source gate reproduces loss of tail-call reuse
under forwarding custom lookup: 8/24 calls retain 9/25 frames, versus 2 with
default lookup. The runtime is not fully qualified; dependent migration is held
until this is repaired without changing the Id contract or weakening invariants.
See the [regression](oop-custom-lookup-tail-contract.md) and [controlled diagnosis](oop-tail-routing-diagnosis.md).
Earlier passing receipts below retain only their recorded subset coverage.

Status: the corrected connected native gate passes in a separate build target.
[Qualification receipt](../measurements/oop-resumable-20261023.json). This milestone
connects generated resumable functions to the retained Machine engine; it does
not qualify cancellation, handlers, recovery, images or the complete language.

## Contract and operation sequence

Before: the synchronous Oop compiler creates declared TCC Methods and Execution
Blocks; the original resumable facade still expects Operand/Outcome/NativeEntry.
The retained frame engine and canonical descriptor validator already use Method
references plus allocation identities.

After: the existing portable-to-resumable lowering and slot coloring produce
Oop step functions and the existing canonical descriptor/selection bytes.
Generated initializers send those as ordinary byte objects with their declared
step Method and Unit to Execution. Execution admits the descriptors and returns
a declared native entry Method with an Execution-owned context. Ordinary Id sends
enter that selected implementation directly. No apply/forwarding protocol or
lookup on the selected Method is introduced.

The existing Machine, actions, capability checks, registration leases, frame
pins, tail transitions and runnable budget stops remain the state authority.
Generated steps use current Machine slot/action messages. The driver releases
all engine/storage borrows before lookup, native entry or any callback. It keeps
receiver, selector, arguments and the selected Method reachable through calls.
Frames retain their exact registered Methods; subsequent sends resolve current
bindings. A budget stop is allowed only after committing the preceding transition.

Registration and context records belong to Execution, never to generic Id or GC.
The context traces its Method, declared step and Unit; Machine roots trace actual
frame contents and executable owners. Dead Machines release frame pins before
registration retirement. TCC remains the mapping owner and unmaps only after the
existing Execution checkpoint/idle handshake. An abandoned runnable Machine must
release the same ownership as a completed computation.

New entry points `Codegen compileResumable:` and `NativeBackend emitResumable:source:`
make this acceptance path explicit while the rest of the default compiler remains
pending. The existing Machine `new`, `runBlock:budget:` and `continueWithBudget:` contract
is reconnected, along with the required slot/action and diagnostic operations.
No alternate interpreter, raw value interface or second scheduler is added.

## Acceptance

Compile source, enter a callee, stop with bounded execution, remove/replace its
binding and collect, then resume. Entered code must finish with its original
meaning; later dependency sends must observe replacement; completed effects must
not replay. Confirm a frame-only code owner survives and its mapping is reclaimed
after final completion or abandonment. Retain the direct-send negative contract,
synchronous source tests, pure compiler/engine tests, and descriptor rejection.


## Implemented acceptance and limits

The source gate exercises bounded runnable stops with zero and one-transition
budgets, nested captured values, entered-callee replacement, GC between drives,
current later sends, and exact effect counts. It verifies completion releases the
parent/old-callee mappings, abandonment destroys frame pins, and final checkpoint
returns the mapping count to its baseline. Replaced Block `value` is honored.
Captured registered Methods retain fresh identity and resumability. Both ordinary
and Machine calls work when every lookup on their selected Method is made to fail.
Malformed descriptor/selection data is refused, and changed top-level publication
results stop later effects under the existing contract.

The existing API names remain `executionState`, `runBlock:budget:` and
`continueWithBudget:`. Step entries keep their zero-or-one Integer PC argument
contract. Generated code reads that Integer through the current value owner.
Canonical descriptors, slot coloring and the frame/registration algorithms are
reused unchanged. Slot access, source calls, tail transitions and result delivery
are connected to those algorithms rather than another interpreter.

These are runnable budget stops, not result-bearing language suspension. The
suspension/continuation, handler/cancellation, recovery, image and worker message
interfaces still need reconnection. Full compiler parity also includes the
remaining shape-aware/portable Block constructors, named adapters, optimized
paths and default compiler orchestration. None of that is silently substituted
with synchronous execution or a v1 compatibility path. The native test's Error
provider remains an independent fixture; full authored-language acceptance is
still outstanding. No new whole-language performance claim is made.

Reproduce this milestone, including the previous native and synchronous gates:

```sh
BIRNAM_OOP_TARGET="$PWD/.cargo-target/oop-resumable-qualified" \
  bash scripts/oop-id-slice-test.sh
```

## Native fallback correction

The draft driver initially bound a native source call and then sent `invokeOn:`
to its selected Method. That was an invariant violation. A negative test poisoned
all lookup on that native Method: direct Id send succeeded but the draft Machine
call failed. This draft is not acceptance evidence.

The corrected fallback uses ordinary Id send, so selected native Methods are
entered without another lookup on them and Id owns DNU forwarding. Preliminary
binding for frame admission is used only after the existing canonical-query and
default-lookup checks establish a callback-free lookup. The retained query Methods
are equality witnesses, never cached executable calls. For native fallback this
can repeat a pure lookup; it cannot replay custom lookup. Custom lookup takes the
ordinary-send path once, within the existing synchronous-call boundary. Tests
count its collecting callback and its DNU lookup separately.

Budget stops do not interrupt a synchronous native boundary. A noncanonical
lookup path can therefore run a nested computation to that boundary before the
budget stop is observed. Full control/suspension acceptance must include this
fallback; this milestone does not claim result-bearing suspension through it.
