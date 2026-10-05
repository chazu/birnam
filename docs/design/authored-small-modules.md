# Authored BlockLoop, Range and SystemGC

NativeLoader and Error are now authored too. Their contracts follow below.

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

These helpers are now authored in `native-values.birnam` over Execution's
[explicit ABI boundary](native-value-foundation.md).
Integer's `fromWordResultOf:selector:` sends a zero-argument query through Id and
boxes its Word result through current `fromWord:`. Boolean's
`sendWordOf:to:selector:` converts a canonical Boolean to Word 1/0 and sends the
caller-selected operation through Id. These are typed operand bridges, with no
collection or System policy. GC retains collection, accounting, precision,
automatic thresholds and finalizers.

Authored capture/argument construction and Word carriers can add real allocations
before or after a query or setting change. Memory diagnostics report that actual execution. They do not
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

## Native Loader

`modules/native-loader.birnam` owns exact-String path recognition, an owned path
snapshot before sequence callbacks, argument validation/order, Unit-versus-Object
results, and mapping/descriptor/initializer error translation. Error construction
still consults the live factory after constructing the diagnostic String.
Reload through Module preserves replacements.

Bootstrap's generic `loadResultAt:arguments:` accepts owned Bytes and a sequence
of Objects and sends Id's existing descriptor-load message. It returns a Vector
of status metadata Bytes (u32 status, u32 operand kind, u64 detail) and a language
Object or nil. It contains no System or error-mapping policy and does not retain
loaded Objects permanently. Id still owns native mapping, validation and pinning.
Bytes `fromByteResultOf:selector:` copies a native byte borrow immediately after
the caller-selected Id send, before further callbacks or result construction.

## Error

`modules/error.birnam` owns constructors, ordinary kind/message slots,
`isError`, propagation on unknown messages, and Object's missing-message
diagnostic. It has no Module-manager dependency: Bootstrap loads its compiled
artifact after the compiler floor and before Module/Lang. It preserves any
existing Namespace Error binding on reload, including nil.

ErrorPrototype is a canonical Namespace export. This preserves its identity when
an image restores captured constructors and methods on ErrorVt. Namespace's
language-facing includesKey: distinguishes an absent binding from a bound nil.

The existing Rust grammar seed can compile this same source into an empty build
prefix. Production initialization loads the artifact and never parses or compiles
Error source. Store and Execution resolve Error through live Namespace messages
when needed; PortableCode no longer requires it merely to initialize. Before
Error is installed, generated initializers and compiler guards do not send its
unavailable predicate. Failures still propagate as native protocol outcomes.

Execution adapts Id's variadic doesNotUnderstand convention to source handlers:
the declared selector is forwarded, extra Object operands remain rooted, and
Word/Bytes/Entry carriers are not inspected. Native implementations keep their
original convention. Native nullable Object inputs normalize to language nil at
the source Method boundary. Ordinary slot reads likewise map null references to
nil. The generic `Execution invalidRequest` operation preserves refusal of an
invalid missing-message selector.

Saved native entries do not root their callable contexts. Fixtures that replace
an authored binding across collection retain its Method/Unit before restoring it,
matching the existing ownership contract.

Acceptance for these two ports: 74 compiler/backend unit tests; production
compiler, loader, text/error, numbers, method observation and source-loader
integration; Store history/limit and local-cell checks in isolated processes;
workspace check; native export/install-name/dependency audit; fresh installation
with both startup images; exact installation receipt; source-free compiled loader
artifact; mapped/rebuilt image and baked executable; REPL arithmetic/Error
printing, FizzBuzz and collections. The REPL session completed in 6.23 seconds.

Store's deliberately tiny 256-byte fixture floor uses a fixture-only kind sink;
it cannot host compiler metadata. It checks Store's requested limit error and
unchanged basis/accounting, while production Error semantics use the separate
full-floor checks. Process isolation preserves Id's owning-thread invariant.

Evidence is under /tmp/birnam-loader-error/: install.log, final-tests.log,
store-final.log, compiler-units.log and image-smoke.log. The fresh prefix is
/tmp/birnam-loader-error/final-prefix, with opt-in TCC resource metrics for the
compiler checks. The installed user prefix was not replaced. Exhaustive Image GC
and the complete language suite were not rerun.
