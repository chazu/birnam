# 74V — Virtual numeric intermediates with original Frame recovery

Status: the default-off runtime consumer is production-verified and integrated
on main. The subsequent captured-input and recovered-Frame control tests pass
in both profiles; their extended production gate and all performance/resource
acceptance remain pending.
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

## Recovered numeric Frames through V2 control

A separate test-floor observation hook stops at an original pending Call only
after generated code and all unboxed optimizer scratch have unwound. The normal
production floor has no hook. The baseline for this comparison also exposes
the original action boundary by declining the older boxed fusion while that
specific test Machine is under observation. Enabled cases force unboxed recovery
at the requested source PC. This is an explicit test/debugger stop, not a new
language-visible poll or a claim that opaque observation is allowed inside an
admitted raw region.

The hook distinguishes the source PC governing the current root state from the
already-published successor PC. It checks every live numeric slot and its alias
identity before and after collection, then publishes ordinary capture or
suspension using the pending Call's existing result slot and successor PC.
After control returns, the bridge checks Frame/Continuation pin transfer,
evicts installed Codegen materializations, collects again, and confirms the
published numeric identities remain alive. Supplying a fresh numeric result
resumes the original continuation. The source checks its value and identity,
requires pins to return to their starting counts, and rejects repeated restore
or resume as already consumed/not suspended.

The focused debug cases pass for the last Call of three-operation chains,
equal-valued distinct results, and a chain crossing a Continue split. The full
[release control matrix](../measurements/74v-unboxed-control-release.json)
passes 448 fresh processes: baseline and forced unboxed recovery, capture and
suspension, both GC modes, and every Call in three-/16-/32-operation chains plus
the equal-result and Continue cases. The equal-result case explicitly requires
two different recovered identities with the same bits. The [debug control matrix](../measurements/74v-unboxed-control-debug.json) also
passes all 448 rows. Both profiles retain matching source/floor/installed hashes.

Source verification also has a `--captured-inputs` mode. It materializes each
numeric Block with an actual captured Integer and uses that value in the raw
chain; ordinary arguments retain their separate identities. The full production
gate now requires separate ordinary-input, captured-input and control matrices
for each floor profile, with matching floor hashes. Full control/captured-input
verification and performance acceptance remain separate gates.

The captured-input matrices pass all 146 cases in
[debug](../measurements/74v-unboxed-captured-debug.json) and
[release](../measurements/74v-unboxed-captured-release.json), with actual raw
completion, materialization and box-savings counters. Together with the control
matrices, this checkpoint adds 1,188 fresh source-process comparisons. The
feature-enabled runtime test suite also passes 195 tests, with 15 ignored. Full
production verification of this extended checkpoint remains required before
integration or a performance/promotion claim.

### Extended production recovery verification

The [extended production receipt](../measurements/74v-unboxed-control-production-correctness.json)
passes all 63 commands with all 252 recorded source hashes matching the tested
checkout. It retains 1,248 closure cases and 1,480 numeric cases: ordinary and
captured-input matrices each contribute 146 processes per profile, and actual
V2 capture/suspension contributes 448 per profile. All matrices for each profile
match the independently tested control floor. Native-prefix, image, cold-archive
and relocation verification also pass. This completes the extended recovery
checkpoint, not numeric cost acceptance or the full four-family objective.

### Cost measurement protocol

The numeric cost probe now measures real source Blocks with three, sixteen and
32 arithmetic operations. Each size has five paired baseline/enabled cases:
a tail result, a warmed operator replaced with subtraction, a mixed Float input,
a compiler-refused Cell result, and an early intermediate returned after the
remaining arithmetic. The mutation case restores the original method and requires
immediate readmission. Every invocation checks the expected answer; the paired
checks require equal logical allocation charges and exact completed-operation,
materialization and saved-box counts.

Preparation is timed separately from the first 128 calls and from steady batches
of 128 calls. Each batch ends with separately timed collection. This keeps the
cost of preserving original collector observations visible alongside execution.
The probe checks that steady execution adds no recovery maps or native owners.
Map storage includes owned vector capacity and the Rc header, excluding allocator
bookkeeping and other native-tier structures. Journal storage includes retained
capacity. Native-unit counts measure owners, not executable bytes. Darwin
`time -l` additionally records whole-process peak RSS in bytes, including startup
and collection, without attributing that entire footprint to recovery metadata.

`scripts/unboxed-numeric-measurement-smoke.py` verifies all 30 source processes
per profile before timing. Full `recovery-graphs.py --unboxed --verify` retains
both matrices beside the ordinary, captured-input and V2 control matrices and
requires their runtime hashes to match. The timing runner,
`scripts/unboxed-numeric-timing.py`, requires that complete production receipt,
matching source and floor hashes, and the requested fixture's counter evidence.
It then builds a fresh release installation and runs four alternating pairs by
default, with 500 batches per process. Preparation, cold execution, steady
execution, execution including collection, process wall time and peak RSS are
reported separately. Existing AC/load admission and interval gates remain intact;
`--exploratory` retains explicitly unqualified results when those gates fail.

These measurements cover bounded source fixtures. A passed counter matrix does
not establish profitability, application acceptance, or promotion of the
experimental consumer. Full arithmetic, loop, aggregate and closure acceptance
remain open.

The initial [debug](../measurements/74v-unboxed-measurement-debug.json) and
[release](../measurements/74v-unboxed-measurement-release.json) matrices each pass
all 30 processes with matching source/floor hashes and positive RSS readings.
For the two measured 32-operation successful cases, the release probe retains
5,112/5,792 bytes of recovery maps and 98,304 bytes of journal capacity after
collection. Native-unit counts equal the baseline. These short counter checks
supply no qualified timing claim; the extended production gate and paired cost
measurements follow this checkpoint.
