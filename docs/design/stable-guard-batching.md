# Stable guard batching

Integer addition previously used 181 Id sends. Its Word expression admission
checks current allocation permission and stable lookup/binding facts separately.
This slice batches only the stable facts through existing Id `nativeBindingsMatch:`.

ObjectModel checks its `allocate:bytes:` binding in one message rather than two.
GC checks the behavior's `allocate` and GC's `prepareAllocation` together in one
message rather than four. Word's uncached primitive-meaning checks also use one
message rather than two. Its existing epoch-keyed meaning cache is retained.
The warmed addition census saves four sends: 181 to 177 (2.2%). It still allocates
three Words and one Integer; this does not eliminate those boxes.

## Authority and fallback

`birnam-abi::binding_guard` is shared marshalling code for these three owners.
It contains no owner storage inspection or module-specific policy. At construction,
it asks Id to declare the semantics of saved lookup, binding and matcher entries.
Those entries are opaque equality witnesses, never invoked by the utility.
The owners trace the selector Objects through their existing name inventories.

Every batch includes witnesses for all three query services, including the matcher
itself, followed by the target entries. Id's canonical matcher checks exact method
and context, ordinary lookup and its current dictionaries without a callback.
A forwarding matcher wrapper cannot acquire a positive canonical result because
its own current binding fails the included witness. Declined admission executes
the original ordered, short-circuit lookup/binding path. Missing matcher falls back;
other failures from this new ordinary Id query propagate. Replaced old guard
services keep their original callbacks and errors. Noncanonical service semantics
at owner construction conservatively disable that owner's optimization.

This depends on Id semantic declaration/query contracts; it cannot protect against
an arbitrary provider that deliberately lies about those contracts. No direct
function call, foreign payload read, new IdApi slot, image ABI change or saved
executable invocation is introduced. Existing Rust cold-bootstrap owners remain.

## Checks that remain necessary

Both `usesDefaultAllocation` queries remain. Allocation permission remains a live
ObjectModel-to-GC message on every expression. GC's phase and final epoch check
remain, as does Word's final epoch check after all input and permission callbacks.
A permission receiver can revoke and restore permission without rebinding any
method; matching epochs alone never admits that change. Representation validation,
input lifetimes, error precedence and final result allocation remain unchanged.

A successful one-target batch saves one send; a successful two-target batch saves
three. A declined batch adds one message before the original path. No universal
CPU improvement or theoretical minimum is claimed from message counts.

## Validation and evidence

`guard_batch.rs` checks exact context, replaced lookup callbacks, original binding
errors, forwarding matcher refusal, GC second-target replacement, conservative construction
and restoration.
The Word expression regression directly toggles a stateful permission at the same
method epoch and verifies refusal/restored evaluation. Existing tests cover
collecting admission, a primitive changed during its callback, custom allocators,
input lifetime and detached Map snapshots.

Lean extends the admission proof to two independent entries, equivalent fallback,
current permission and the post-callback epoch fence. A mutation dropping the
second entry must fail. The TLA+ admission model now has two independently mutable
entries; unsafe context, service, cached-permission and missing-fence variants
must still yield counterexamples. These are abstract contract checks, not full
verification of Rust, generated C or arbitrary providers.

The bounded census uses production Integer policy and records the complete warmed
message tree. Fresh-prefix saved-image startup and focused test results are recorded
in the [measurement receipt](../measurements/stable-guard-batching-20261009.json).

Accepted validation passed seven Rust checks, thirteen native checks across thirteen
binaries (with the extended guard fixture also rerun), fourteen task assertions
at quanta 128 and 1, 137 Lean theorems and all 84 TLA+ configurations. All 26
Lean mutations were rejected, including omission of the second batched entry.
The fresh release prefix passed its required saved-image startup gate. The final
census reproduced 177 sends with unchanged inputs during measurement. These are
focused checks, not an exhaustive workspace rerun. Changes remain uncommitted.
