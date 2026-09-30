# 74X — Fresh literal results in repeated closure regions

Status: implementation in progress. This broadens closure bodies beyond entry
values and aliases, within the four-family obligation in 74T.

A closure returning an integer literal must allocate a fresh Integer on every
invocation. Equal bits never authorize sharing that result across calls. The
compiler will retain the exact ordered stream of canonical index boxes and
literal boxes, identifying the allocation whose identity is returned. Entry
recovery remains the original entry values/nil at PC zero, before those effects.

The shared small-callee plan describes this allocation stream. Runtime admission
continues to require exact current registered code, controls, API/constructor
context and sufficient allocation budget for the whole admitted interval. All
literal allocations occur at their original position, including unused literals;
only the private closure allocation can be deferred by the existing journal.

Repeated calls need distinct result references in caller dataflow. A host vector
stores the produced Oops during the bounded nonobserving interval, and each
continuation publishes the appropriate identities into the original Frame slots.
Forced exits restore the closure identity and leave earlier literal results
intact. Future-result arguments initially end the admitted sequence, while the
ordinary continuation executes them normally. No selector gains intrinsic meaning,
and observing operations or noninteger literal bodies remain refused.

Required checks include actual canonical allocation traces, equal-but-distinct
results across repeated calls, aliases of one result, escaped closures, forced
exits at every supported invocation, GC budget refusal, lookup/API replacement,
control/retirement, and installed source under both GC modes. Full verification
and measured profitability remain prerequisites for promotion. This extension
does not complete the four-family objective by itself.

## Implementation checkpoint

The compiler now records literal/index allocation order and distinguishes an
entry result from a fresh allocation. Repeated closure plans retain invocation
result identities separately, including aliases of earlier results used after
later calls. A call argument requiring a preceding produced result ends the
admitted region; its ordinary continuation executes with the published result.

`BIRNAM_DEV_LITERAL_CALLEES=1` enables staging of these additional callee plans.
The switch defaults off. Closure elimination additionally requires its existing
`BIRNAM_DEV_CLOSURE_ELISION=1` switch. Original alias-only plans avoid allocating
the new literal metadata vector. Literal values, including unused values, are
physically boxed in their canonical order. Budget admission includes those boxes;
only the private closure identity is eligible for deferred allocation here.

The real compiled-callee differential test covers five Integer values including
both signed extremes, unused literals, aliases, distinct equal-valued returns,
exact canonical allocation traces/charges, overridden Return and collection
inside the original boxing sequence. It passes in debug and release. The
workspace passes 544 tests with 19 diagnostic tests ignored.

The installed fixture checks three fresh results, aliases of the first result
after subsequent calls, an escaping closure invoked after collection/eviction,
and changed current method effects. Its matrix includes both GC modes, disabled
staging, ordinary execution, enabled execution and forced recovery before each
of the three calls. The production recovery gate now requires these matrices in
both profiles, with matching control-floor hashes and complete coordinates.
Full production verification, literal-specific capture/suspension and lifecycle
coverage, further body forms and performance acceptance remain open. This
checkpoint does not promote the consumer or complete the closure family.

The [focused correctness receipt](../../measurements/74x-closure-literals-correctness.json)
retains the workspace/release test output and the independently audited
[debug](../../measurements/74x-closure-literals-source-debug.json) and
[release](../../measurements/74x-closure-literals-source-release.json) source matrices.
All 24 processes pass with matching source, selected-floor and installed-prefix
hashes. The initial attempt with the older installed Codegen correctly produced
no optimization hit; rebuilding the isolated compiler installation supplied the
new constructor hook and made the actual consumer testable.

## Full production verification of literal callees

The [production receipt](../../measurements/74x-closure-literals-production-correctness.json)
passes all 71 commands, with all 265 source hashes matching the tested checkpoint.
It retains 1,320 closure source cases and 1,604 numeric/loop cases (2,924 total),
including all 24 literal-body cases. Every matrix matches the independently
built debug or release control floor. Production installation, native execution,
image, cold-archive and relocation checks pass.

This verifies the initial default-off literal-callee implementation. The separate
literal-result capture/suspension extension has its own checkpoint and gate.
Profitability, preparation/resource/application acceptance and closure promotion
remain open; this receipt does not complete the full closure family.

## Literal results through V2 capture and suspension

The extended test-floor control bridge now carries three equal-valued but
identity-distinct literal results, an alias of the first, and the recovered
closure through actual host-owned Machine capture/suspension. The literal body
also retains the original captured Cell. The probe's saved identities live in
untraced host vectors. It collects with native-stack scanning omitted, so only
the real Frame or Continuation can retain the recovered results and closure.

The observer checks all three values and exact Frame aliases before and after
collection. While paused, the bridge evicts Codegen materializations and collects
again, then supplies a fresh Integer with equal bits. On resumption the source
must preserve every earlier result identity and alias and distinguish the
supplied identity. Wrong-Machine restore, repeated restore and repeated resume
are rejected; observer effects occur exactly twice and Frame/Continuation pins
return to their original counts. A subsequent closure invocation returns another
fresh identity while retaining its original captured Cell.

The [debug](../../measurements/74x-literal-control-debug.json) and
[release](../../measurements/74x-literal-control-release.json) matrices each pass
24 fresh processes: both GC modes, capture/suspend, disabled literal staging,
ordinary execution, optimized execution and forced recovery before each of the
three calls. All source/floor/installed hashes match. The unchanged original
closure-control behavior also passes its [12-case debug regression matrix](../../measurements/74x-literal-control-legacy-debug.json).
The workspace still passes 544 tests with 19 diagnostic tests ignored. The full
production gate now requires both complete literal-control matrices alongside
the existing control coverage; that updated gate remains pending. These checks
complete the focused capture/suspension obligation for this body shape, not
performance acceptance or the broader closure family.

## Full production verification of literal-result control

The [literal-control production receipt](../../measurements/74x-literal-control-production-correctness.json)
passes all 71 commands, with all 267 source hashes matching the tested checkpoint.
It retains 2,972 source cases, including all 48 literal-result capture/suspension
cases. Every source matrix matches its independently verified control floor.
Installed execution, image, native lifetime, cold-archive and relocation gates
pass. The literal-specific checks preserve distinct result identities, earlier
aliases, supplied-result identity and shared captured Cells across paused GC and
resumption. This completes that production control checkpoint; preparation,
profitability and wider closure acceptance remain open.
