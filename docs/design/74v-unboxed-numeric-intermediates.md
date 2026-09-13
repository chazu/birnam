# 74V — Virtual numeric intermediates with original Frame recovery

Status: the default-off runtime consumer is implemented, production-verified
and integrated on main. Further recovered-Frame control/captured-input
verification and performance/resource acceptance remain pending.
This continues the unboxed-arithmetic requirement in
[74T](74t-representation-and-allocation-optimizations.md), whose enabled numeric
fusion still allocates every original result box.

## Required behavior

Carry successful numeric intermediate results as raw bits until an observation
requires their original object identities. Derive operations from current
admitted native implementations, preserving exact overflow/error behavior and
ordinary Id malleability. Keep public ABI, canonical descriptors, V2 transitions
and image encoding unchanged. Equal numeric values from distinct allocations
remain distinct identities; repeated aliases of one result recover one object.

The initial supported bodies are those of the existing numeric Template:
bounded straight-line integer chains with entry values, local binds, literals,
Call/Continue edges and a final Return or TailCall. Unsupported operands,
errors, mutation, native declarations, storage or allocation budgets must take
the established path. Selector names do not confer numeric authority.

## Compiler maps

`numeric_chain::RecoveryMaps` is derived separately from the existing Template,
so the boxed consumer does not pay its preparation or metadata cost. Derivation
requires the numeric Template and exact canonical coloring. Every PC records
all physical aliases of prior numeric results at original action publication,
including the send operand window, and after its original continuation.

An alias names the numeric operation's result identity, never its numeric bits.
Absent aliases leave ordinary Oops in place, including input values and distinct
literal boxes. Local binds propagate result identities. V2 parallel copies read
their sources before destinations change; clears and call-result initialization
produce the successor map. The current pending operation has no result identity
at publication. A final TailCall creates a returned result but no local successor.

Initial tests cover shared local/send aliases, equal computations producing
distinct identities, final Return and TailCall, two-/three-/16-/32-operation
chains, slot reuse/dead-value clearing, and rejection of changed layouts and
observing operations. These are compiler checks, not runtime allocation savings.

The compiler checkpoint passes all 85 `birnam-codegen` debug tests and all five
recovery-map release tests. The added Continue case propagates one result
identity through 110 aliases across a canonical split without inventing another
numeric result. Verification commands, with an explicit isolated Cargo target:

```sh
cargo test --locked --offline -p birnam-codegen
cargo test --locked --offline --release -p birnam-codegen numeric_chain::recovery_maps
```

The compiler and runtime changes have not been integrated into the main
checkout; the full source/production gates below remain required.

## Runtime integration contract

1. Extend private allocation-journal preparation to exact raw numeric payloads.
   Establish current representation and native constructor authority explicitly;
   do not disguise raw numeric bits as Oop payloads. Keep the original allocation
   charge, fresh identity and collector/finalizer behavior. Preparation is not an
   Id allocation; materialization must not charge again.
2. Use the existing raw numeric ticket and current binding/context guards to
   admit a bounded interval before any effect. Include all baseline allocation
   charges and journal capacity in admission. No observing callback, collection
   or native retirement may intervene while a live result remains virtual.
3. Execute original canonical PC entries and V2 transitions with private
   placeholders for admitted virtual results. Those entries only move/stage
   values in the supported Template; the raw ticket supplies numeric results.
   Existing literal/index/PC-token boxes remain ordinary allocations initially.
4. At every supported forced exit, materialize all published aliases of already
   charged results before ordinary dispatch or observation. Restore pending
   receiver/argument identities as well as local Frame aliases. Future
   predictions are not executed results and must not allocate or materialize.
5. Materialize results required at the final observation boundary, preserving
   return identity and any still-observable Frame aliases. Prove dead values
   before leaving them journaled. Do not assume that returning a value by itself
   proves the Frame is already retired.

## Completion evidence

Require real-source baseline/enabled/forced comparisons at every supported exit,
with actual physical allocation savings and unchanged logical allocation charges.
Exercise equal/distinct results, repeated aliases, captured inputs, original
numeric literals, extremal arithmetic, zero division, mixed/error refusal,
changed operators/API/layout, collecting observation, V2 capture/suspension,
native eviction/retirement and journal/automatic-collection limits.

Run debug/release correctness and the production source/install/image/archive/
relocation gates. Commit correctness before paired execution and collection
timings. Record preparation, metadata, native code and retained resources, plus
refusal and cold/application costs. Keep qualification gates unchanged. This
work does not complete the loop, aggregate or closure families, nor establish
the full arithmetic family until runtime behavior and its acceptance are proved.

## Experimental runtime checkpoint

The isolated consumer is selected by `BIRNAM_DEV_UNBOXED_NUMERIC=1` before native
candidate preparation. Its optional recovery maps belong to the candidate and
are absent when disabled. `BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT` selects an
original source PC for recovery; this includes Continue and Return boundaries.
There are no public ABI or image-format additions.

Exact signed-integer payload snapshots live in the existing untraced host
journal. Preparing a snapshot has no logical allocation effect. Executing a
numeric Call charges its original allocation and assigns a fresh deferred
identity. Original PC code and V2 transitions use private placeholders until an
observation boundary. Recovery materializes each required identity once and
patches all mapped local and send-window aliases before ordinary dispatch.
Equal intermediate bits still denote separate allocations.

Normal completion materializes only the returned identity. The driver's
existing `finish_stackless_return` pops the private Frame and commits the caller
transition without allocation or foreign/generated calls. This retirement
boundary makes the other private results dead before an observer can run. They
retain original journal accounting and collector handling. Final TailCall
results use the ordinary numeric constructor at the original allocation point.
Literal, index and PC-token allocations remain unchanged.

Admission includes the current native numeric declaration and constructor
context, current Machine/lookup controls, all original allocation charges,
journal capacity and the automatic-collection threshold. The unit-test bootstrap Integer vtables have no declared layout; their
existing conservative payload tracing is supported without changing that
layout. The public floor installs the ordinary raw layout, which is also
supported. The current native constructor
establishes the eight-byte representation. An explicitly incompatible layout or
an inherited trace hook refuses admission.

Focused tests now exercise 73 fresh runtime cases across three-operation
Return/TailCall chains, equal intermediate results, a Continue split with 110
aliases, and 16-/32-operation chains, with forced recovery at every source PC.
They check numeric results, original logical byte charges, actual physical box
savings, materialization counts, Frame retirement and released registration
pins. Normal 3-/16-/32-operation chains save 2/15/31 intermediate result boxes.
These are allocation observations, not timing claims.

A shared differential harness additionally exercises 52 fresh runtime cases
covering changed Machine controls, lookup, mutation epoch, operators, ErrorVt,
native declarations, incompatible layouts, trace hooks and a full 4,096-entry
journal. Ten collection-budget cases include an actual root-provider mutation
of an input at collection. Twelve arithmetic-fault cases preserve overflow and
zero-divisor effects at the original ordinary calls. The raw-journal unit test
checks signed extremal bits, distinct identities and exact once-only charges.

Installed-source observation, capture/suspension, native retirement, mixed-type
and resource/performance acceptance remain required; these targeted checks do
not complete the full arithmetic family or any of the other three families.

The runtime checkpoint passes all 538 debug workspace tests (19 ignored), and
all 281 runtime/codegen release tests (15 ignored). Both profiles execute the
raw-journal check and all four unboxed runtime tests. The
[checkpoint receipt](../measurements/74v-unboxed-runtime-correctness.json)
records commands, counts, log hashes and source hashes. Production gates and
qualified timings remain pending.

## Installed-source consumer and release-context repair

A feature-gated `UnboxedNumericProbe` measures actual source-created registered
Blocks through installed Codegen and the real Prelude. It holds explicit roots
for its native arguments while requesting collections without native stack
scanning. Its counters check eight measured invocations after warmup, logical
allocation charges, physical box savings and materialization at each forced PC.
A final rooted result also survives collection. The production floor does not
publish this probe, even when its activation environment variable is present.

The first full debug source matrix passed, but release returned ordinary results
with zero optimization hits. The retained
[release refusal receipt](../measurements/74v-unboxed-source-release-refusal.json)
and [preceding debug matrix](../measurements/74v-unboxed-source-debug-before-context-fix.json)
record the difference. Diagnostics found that the numeric declaration received
a different address for the private small-integer reader than the address
published in IdApi. The other five context pointers agreed. Admission correctly
refused the mismatching context; unit native fixtures did not expose it because
they do not compare the real Prelude API context.

Canonical numeric function pointers now have one immutable typed owner,
initialized once and shared by IdApi publication and declaration checks. This
avoids independently taking a private function address in separate release
codegen units. The Block-constructor and codegen guards use that same owner for
shared operations. The IdApi layout and callable signatures are unchanged.
Declarations still inspect each module's actual API pointers on every use;
replaced or copied API tables gain no new authority.

The corrected [release source matrix](../measurements/74v-unboxed-source-release.json)
passes 146 fresh processes across both GC modes and every supported PC for
three-operation Return/TailCall chains, equal results, a Continue split, and
16-/32-operation chains. Each case uses the installed compiler and actual
Prelude implementations. The source, floor and installed-artifact hashes match
before and after execution. The copied-API native fixture also passes for both
floor profiles: overriding boxing refuses, restoration readmits, and altering
each of the six context operands refuses. The existing numeric source fixture
passes its arithmetic, mixed-value, error, identity and eviction/retirement
assertions with the consumer enabled.

Full production control/capture/suspension and performance acceptance remain
pending. Source allocation savings do not imply an execution-time gain.

`python3 scripts/recovery-graphs.py --unboxed --verify` now includes the unboxed
runtime tests in both profiles and requires both installed-source matrices
alongside all existing closure/source, native, image, archive and relocation
gates. Each unboxed matrix must contain 146 rows, prove the probe absent from
the production floor, and match the floor used by the independent closure
control checks. The gate hashes the new script and runtime sources. Adding this
verification path does not itself establish a passing production receipt.

The corrected [debug matrix](../measurements/74v-unboxed-source-debug.json) also
passes all 146 rows, bringing the installed-source total to 292 fresh processes.
After the function-identity repair, the complete debug workspace again passes
538 tests with 19 ignored. Full production verification is the next gate.

## Production checkpoint at 63becebf

The [production receipt](../measurements/74v-unboxed-production-correctness.json)
passes all 63 commands, the 538-test workspace (19 ignored), and the production
native-prefix, image, cold-archive and relocation gates. Its 250 source hashes
match the tested checkout. Ten existing closure matrices retain 1,248 source
rows; the two numeric matrices add 292 rows, for 1,540 in total. All twelve
matrices use the matching per-profile feature floor also verified by the
independent control checks. Debug and release floor hashes are respectively
`89c51cf5d535d9b8ba06dbfffc391352bc68193ecc024c3ce38c4f49d593c6a0` and
`1c38516839896f30729aee1d8f637aabdcfa3a2cf4bb994d63d2294d4f7d2d7a`.

This verifies the default-off runtime consumer and canonical numeric-function
identity repair for integration. The later control/captured-input extension is
being verified separately. Arithmetic timing, preparation/metadata/resource and
application acceptance remain open; no full optimization family is complete.
