# Authored BlockLoop, Range and SystemGC

These replace the native Block Loop, Range and System GC crates. Bootstrap loads
compiled artifacts after Module and Lang. The build-only Rust grammar seed can
compile this floor without any of the three removed native modules. Every owner
interaction remains an Id send; private value representations stay with their
native owners.

## Block loops

`modules/block-loop-control.birnam` owns condition/body evaluation, canonical
Boolean checking, exact Error-family recognition and lazy error construction.
Its continuations execute outside native Boolean callbacks, allowing existing
Execution tail calls to keep constant stack depth.

`modules/block-loop.birnam` supplies synchronous `whileTrue:`/`whileFalse:` entry
methods. They preserve the bootstrap suspension barrier. Later stackless control
methods replace them normally. Module reload preserves replacements.

Execution's `evaluatePreservingFailures:` evaluates one Block with a scoped
native-failure propagation policy. It returns native status/detail unchanged,
instead of turning failed callbacks into language Errors. The prior policy is
restored on every return; frame driving, retention and suspension remain with
Execution. No loop algorithm or new scheduler is implemented there. Native Unit
results still normalize to language nil, following ordinary compiled sends.

## Range

`modules/range.birnam` owns inclusive endpoints, construction, size, membership,
indexing, callback iteration/folding, printing, reversal and concatenation.
Its two endpoint references use ordinary Object Model slots. Shallow cloning,
precise tracing and image slot transport therefore reuse the existing owner.

Object Model provides `newSlots:parent:`, `slot:in:` and `slot:in:put:` for language
callers. It creates private behavior and Oop slot metadata, obtains Vector values
and Integer indices through messages, and alone accesses ordinary object storage.
These operations contain no Range policy.

Integer's `integerValue:` normalizes a value through current `wordOf:`/`fromWord:`.
`compareInteger:with:` compares signed Integer bits exactly and returns -1/0/1,
avoiding the established floating-point conversion of ordinary ordered numeric
comparisons. Bounds, count overflow and out-of-range policy remain authored.
Integer iteration supplies inclusive endpoints without wrapping either i64 bound.
Range `do:` ignores body results; folds retain each result, including Errors.

## System GC

`modules/system-gc.birnam` supplies `gc`, `liveObjects`, `autoGc:` and `preciseGc:`.
Count queries execute before current Integer result construction, preserving all
native Word bits. Setters accept canonical Booleans and return their actual System
receiver, including aliases. Reload preserves replacement methods and settings.

Integer's `fromWordResultOf:selector:` sends a zero-argument query through Id and
boxes its Word result through current `fromWord:`. Boolean's
`sendWordOf:to:selector:` converts a canonical Boolean to Word 1/0 and sends the
caller-selected operation through Id. These are typed operand bridges, with no
collection or System policy. GC retains collection, accounting, precision,
automatic thresholds and finalizers.

Authored capture/argument construction can add real allocations before a query
or setting change. Memory diagnostics report that actual execution. They do not
reproduce the preceding Rust wrapper's allocation history or fabricate charges.

## Acceptance

The former native-loop/collection/SystemGC tests now exercise the authored
artifacts through production Bootstrap. They retain the 10,000-iteration check,
native failure status/detail, scoped failure-policy restoration, wrong arity,
exact-family errors, collecting/replaced callbacks, suspension rejection and
later resumable control, Range endpoint extremes/cloning/materialization, live
GC queries/constructors/setters, alias roots, precision, finalizers and reloads.
Count assertions describe the actual heap at the query point; enabling automatic
collection must preserve existing and newly constructed charges until the next
allocation checkpoint.

A fresh installer run generates and qualifies both startup images with no removed
native crates. A separate source-free saved-image check restores Range aliases,
clones and both i64 endpoints and invokes the authored System GC controls. See
the [roadmap checkpoint](native-module-decomposition-roadmap.md#small-rewrite-checkpoint-blockloop-range-and-systemgc-2026-10-04).
