# 74W — Reuse invariant arithmetic across actual tail-loop iterations

Status: implementation in progress; default-off experiment for the remaining
loop requirement in [74T](74t-representation-and-allocation-optimizations.md).

The existing tail-entry plan removes entry-layout work. This consumer additionally
removes arithmetic whose raw inputs remain unchanged across successive iterations
of an actual registered Id tail loop. It does not infer a loop from selector
spelling or replace Id control with a native loop.

## Analysis and admission

A numeric plan derives a 64-bit input-dependency mask for every operation. A result
depends on the union of its operands' masks. At most 64 inputs and 32 operations
remain supported. The ordinary kernel still performs the first evaluation. A
second private kernel skips operations whose transitive input bits did not change;
all other operations retain the original checked arithmetic and first-error order.
No public ABI or canonical descriptor/image representation changes.

Reuse requires the numeric invocation's immediate caller to have traversed an
actual self-tail transition since the preceding evaluation. The execution arena
records this private fact only after current lookup, acquisition and successful
Frame replacement. A stamp retains the arena, Frame index/generation and exact
parent descriptor owner. Fresh calls and continuation restoration clear the fact;
stamps must be consecutive generations of the same parent implementation. Both
ordinary and planned tail-entry paths record the same fact. Thus independent
calls, merely equal input values and stale Frame identities do not establish a
loop. Mutual recursion and non-tail loops initially refuse this admission.

A compiled numeric owner retains one bounded previous-input/result buffer and
stamp. Values are raw bits in untraced host vectors; no result Oop or shared Cell
is cached. Each entry rereads actual numeric inputs, including payload mutations,
and the established consumer still validates current lookup, native methods,
declarations, API context, storage and control before executing any optimized
result. The cache cannot authorize an operation or bypass those guards. Failed
predictions discard the cache and fall back before performing Id effects.

## Observable behavior

Only successful arithmetic computation is reused. Ordinary source-PC entries,
lookup/observation refusal, allocation charges, fresh object identities, effects,
return values and stackless transitions remain the responsibility of the existing
consumer. With boxed fusion every original box remains; with separately admitted
74V unboxing, its original identity/materialization obligations still apply.
Capture, suspension and restoration neither serialize nor restore the cache.
Retirement destroys caches with their compiled owners; retained stamps are
metadata owners and grant no right to execute old code.

## Required evidence

Compiler/native differential checks cover transitive dependency masks, partially
changed inputs, integer extremes, division by zero, overflow, and earliest-error
order. Arena checks distinguish actual tail recurrence from pop/push, a changed
target, capture/restore, exhausted generations and a different arena.

Installed source loops must demonstrate removed arithmetic, equal answers and
fresh identities with varying loop inputs and invariant subexpressions. Mutation
of inputs, operators, lookup and declarations must invalidate or refuse reuse.
Effects occur exactly once in order; suspension/capture stops before later loop
work. Both tail-entry configurations and boxed/unboxed arithmetic need coverage.
Full production verification precedes paired cold/warm/resource measurements,
including changing-input controls. No promotion or full-family completion follows
from compiler metadata or a synthetic kernel alone.

## Initial implementation evidence

The compiler emits both ordinary and dependency-masked kernels, and the runtime
retains one previous raw-input/result vector per enabled compiled owner. Production
builds require `BIRNAM_DEV_LOOP_INVARIANTS=1` before kernel preparation. No reuse
kernel is emitted in the default configuration. The counter exposed only by the
test floor counts skipped kernel prediction computations; it does not by itself
prove that the surrounding fusion or unboxing admission succeeded.

The initial workspace run passes 541 tests with 19 ignored. Focused debug/release
checks pass the native arithmetic differential, arena stamp lifecycle, existing
tail-entry behavior and registered-loop consumer tests. The consumer matrix has
56 baseline/enabled configurations across ordinary/planned tail entry,
boxed/unboxed execution, unchanged and mutated inputs, operator replacement and
restoration, observing lookup, declaration withdrawal/restoration and collection.
It compares effect/value traces, lookup counts, fresh result identities and
logical allocation charges (the explicitly collecting case compares traces).

The [debug](../measurements/74w-loop-invariant-source-debug.json) and
[release](../measurements/74w-loop-invariant-source-release.json) installed-source
matrices each pass 32 processes with matching recorded source and floor hashes.
They cover both GC modes and boxed/unboxed configurations. Across 128 actual Id
tail-loop iterations, a fully invariant three-operation chain skips 381 kernel
operations, a chain with a varying second operand skips 127, and all-changing
inputs skip none. Independent repeated-call controls skip none. These source
loops use current Codegen and ordinary Id conditionals, returning the same answers
and preserving the preceding result's distinct identity.

Full `recovery-graphs.py --unboxed --verify` now includes the three focused loop
checks in both profiles and retains both source matrices with the independently
verified floor hashes. That full gate, actual loop capture/suspend/retirement
coverage, bounded cache/resource acceptance and qualified profitability remain
required. This checkpoint neither enables the consumer by default nor completes
the loop family or any of the other three families.

## Capture, suspension and retirement checkpoint

The [focused control receipt](../measurements/74w-loop-control-correctness.json)
passes 542 workspace tests (19 ignored), the release loop checks, and 64
registered-loop configurations per profile. The matrix covers baseline/enabled
reuse, boxed/unboxed arithmetic, ordinary/planned tail entry, both GC modes,
capture/suspension and retirement while paused. After three iterations it requires
six skipped kernel operations in enabled mode and actual arithmetic-consumer
hits, then stops before any later loop effect. Resumption preserves the exact
supplied result identity, rejects repeated consumption, restores all code-pin
counts and reestablishes loop evidence before further arithmetic reuse.

This test initially found that successful generation retirement left native-tier
candidates alive while a descriptor owner remained. Descriptor lifetime alone is
insufficient to determine registration lifetime. Successful retirement now removes
matching native and closure records explicitly, releasing detached native/loader
owners outside the registry borrow. Rejected retirement leaves those records
intact. Weak-owner checks prove that the retired numeric kernel is released while
the paused caller's native candidate remains owned until execution completes;
retirement of that caller then releases its candidate too.

The fresh [debug](../measurements/74w-loop-control-source-debug.json) and
[release](../measurements/74w-loop-control-source-release.json) source matrices
retain all 32 cases per profile with matching source and floor hashes. Full
production verification of this lifecycle fix, broader installed-source control
coverage, bounded resource acceptance and profitability remain required. The
consumer stays disabled by default and the four-family objective remains open.

## Full production verification

The [corrected production receipt](../measurements/74w-loop-control-production-correctness.json)
passes all 69 commands, with all 262 source hashes matching the tested checkout.
It retains 1,248 closure cases and 1,604 numeric/loop cases (2,852 total),
including all 64 installed loop cases. Every matrix matches the independently
verified debug or release control floor. The focused control tests cover actual
capture/suspension, resumption, rejection of repeated use and native-generation
retirement, including release of the retired arithmetic cache. Native-prefix,
image, cold-archive and relocation checks pass.

This verifies integration of the default-off loop consumer and retirement fix.
It does not establish profitability, non-tail or mutual-recursive loop coverage,
application/resource acceptance, or completion of the full loop family.

## Loop cost and retained-cache protocol

`scripts/loop-measurement-smoke.py` checks actual installed Id loops with three,
sixteen and 32 arithmetic operations per iteration. Each size has fixed inputs,
one changing input, all-changing inputs and independent-call controls. Both GC
modes and boxed/unboxed execution are paired with loop reuse disabled/enabled.
Every loop checks fresh result identity; the test floor also checks the final
answer before and after collection. Each pair requires identical allocation
charges, saved-box counts and actual fused/unboxed operation counts. Reuse counts
must be exact: 127 times the chain length for fully invariant inputs, 127 for the
partially invariant chain and zero for changing or independent-call controls.
These counts are per complete 128-iteration loop, with no reuse between separate
loop invocations.

The test floor records the first complete loop separately from steady execution
and collection. The first-loop interval includes hot kernel preparation but
excludes CodeObject materialization. It checks owner counts and retained cache
capacity after every measured loop and collection. Reuse storage includes inline
state, raw-vector capacity and the test-only dependency-mask vector; it excludes
shared descriptors, executable pages and allocator bookkeeping. An independent
call may retain its most recent stamp, but cannot reuse that stamp unless the
next call proves an actual consecutive self-tail iteration. Zero reuse, rather
than absence of cached state, is the required independent-call property.

`scripts/loop-timing.py` requires a complete, source-matching production receipt
containing all 192 measurement cases. It builds a fresh release installation and
uses the unchanged preflight and interval host gates. Four alternating pairs and
500 loops per process are the defaults. It reports execution, execution plus GC,
first-loop cost, process wall time and peak RSS separately. The test floor's
build is the same on both sides, but reuse counting runs only on the enabled
path; its overhead is not symmetric. These source-fixture
measurements cannot establish application performance or promotion by themselves.

The combined production checkpoint includes the compact snapshot representation
from 74V and this measurement bridge. Its full gate will replace the compact-only
queued gate, which was cancelled before it started. Focused validation precedes
that combined run; no full production or timing result is asserted here yet.

The [release source matrix](../measurements/74w-loop-measure-release.json) passes
all 96 cases with matching current source, floor and installed-prefix hashes.
Every enabled case retains one owner and one cached input/result set. Owned reuse
storage is 192, 400 and 656 bytes for three, sixteen and 32 operations respectively,
including the test masks. Capacity stays constant across measured loops and GC.
The independent-call controls retain the last stamp but reuse zero operations.
The [debug matrix](../measurements/74w-loop-measure-debug.json) also passes all
96 cases with matching hashes. The [focused receipt](../measurements/74w-loop-cost-focused-correctness.json)
retains both matrices and the 545-test workspace/201-test release runtime results.
The combined full gate remains queued behind literal-control verification.

## Combined production gate passed

The [combined production receipt](../measurements/74w-loop-cost-production-correctness.json)
passes all 71 commands, with all 271 source hashes matching. It retains 3,164
source cases and matching independently verified debug/release floors. Both
96-case loop measurement matrices contain every operation count, input shape,
GC mode, numeric representation and reuse setting. The closure measurement
matrices also cover every one-, three-, four-, eight-, sixteen- and 32-use
configuration. Installed execution, native lifetime, image, cold archive and
relocation checks pass. The receipt is committed as `370be3d9`; the verified
compact snapshots and loop measurement support are integrated on main at
`9eed9f0a`, whose production sources match the receipt exactly.

The loop timing matrix has started after passing the unchanged host preflight.
Its interval gate and paired results remain pending. Closure-threshold and
compact-numeric matrices are serialized behind it, then combined application
and process-resource acceptance. A rejected measurement stops that timing queue
and retains its evidence for reassessment. No profitability or promotion follows
from this correctness checkpoint alone.

## Completed diagnostic loop matrix and normal-floor follow-up

The [retained loop timing matrix](../measurements/74w-loop-exploratory-timing.json)
contains all 192 processes and passes all 96 counter differentials. Preflight
eventually passed, but the measured interval failed: median idle was 28.90%,
with 368 of 731 samples below 30%. The receipt is `not_qualified`. All negative
observations are retained, and the dependent closure/numeric timing queue stopped.
The new compact installation also passes all eight normal application checks in
the [updated application receipt](../measurements/74t-compact-application-comparison-correctness.json).

For fully invariant inputs, diagnostic execution/including-GC improvements are:

| Operations | Boxed execution / including GC | Unboxed execution / including GC |
| --- | --- | --- |
| 3 | +0.55% / -0.06% | +2.29% / +3.86% |
| 16 | +2.24% / +2.29% | -4.86% / -7.81% |
| 32 | -7.89% / -9.95% | +0.70% / +1.52% |

These noisy results do not determine an admission threshold. There is also an
instrumentation limitation: the test floor scans its dependency-mask vector to
count skipped predictions only on the enabled path. Production builds omit that
scan and vector. Thus even a host-qualified test-floor comparison would still
need a normal-floor comparison before small differences could guide promotion.

`scripts/loop-production-timing.py` supplies that comparison. It keeps the
previously verified numeric, self-tail-loop and independent-call factories
byte-identical, replacing only the native probe driver with ordinary Id calls,
`Time monotonicMicros` and `System gc`. Each result is checked before and after
collection. Verification runs both the release test floor and the unchanged
normal release installation. The test variant checks exact reuse counts around
the Id-driven phase and then uses the established native probe to independently
check numeric-consumer admission. The normal variant contains no probe calls.

The runner requires its own complete 96-process verification receipt before
timing, along with matching full-production sources, test-floor identity,
fixture bytes and the complete normal installation manifest. Timing retains
all operation counts, input/control shapes and boxed/unboxed settings, with
alternating pairs and unchanged per-pair preflight/interval gates. Cold execution,
steady execution, collection, process wall time and peak RSS remain separate.
The ordinary Id driver and collection boundary differ from the native probe;
absolute durations between the two harnesses are not directly comparable.

### Normal-floor admission attempts

The [Id-driver correctness receipt](../measurements/74w-loop-production-driver-correctness.json)
passes all 96 combinations on the test and normal release floors, with current
source, fixture and installed-artifact hashes. Missing-case and wrong-floor
verification receipts are rejected before measurement. This checkpoint is
committed as `c285b205`.

The [first normal-floor attempt](../measurements/74w-loop-production-admission-attempt.json)
completed eight processes with passing per-pair host gates, then stopped when
the next preflight could not admit compilation activity. Process ancestry tied
that compilation to the `rk` daemon. A [second attempt](../measurements/74w-loop-production-paused-timing.json)
temporarily paused that exact daemon under the existing user authorization.
Its compiler children received no signals. Six processes completed before the
interval gate rejected median idle of 46.30%; the required median remains 50%.
Neither incomplete matrix establishes a production admission threshold.

The [process-control receipt](../measurements/74w-loop-production-process-control.json)
retains the guard source, daemon identity, pause/resume times and verified
post-resume state. The same daemon was resumed after the rejected attempt.
No benchmark process remains active from these attempts. A sufficiently quiet
host window is still required for the full production matrix and the subsequent
closure, unboxed and application/resource timing decisions.

### Continued work without quiet-host admission

The user subsequently waived the quiet-window requirement for now. The normal
loop driver accepts `--exploratory`: it skips preflight waits, continues through
rejected diagnostic intervals, and always records `qualified: false`. Ordinary
qualified mode retains the original gates. Execution, source and artifact
verification still fail normally on errors.

The [updated driver verification](../measurements/74w-loop-exploratory-driver-correctness.json)
passes all 96 test/normal-floor cases. A [48-process exploratory smoke matrix](../measurements/74w-loop-exploratory-mode-smoke.json)
covers every operation count, shape, representation and reuse setting. It records
24 diagnostic intervals, performs no host-admission waits and remains explicitly
unqualified even though those short intervals did not fail. Full exploratory
cost matrices can now proceed without treating host load as a work blocker.

### Avoid the reuse-kernel call when every input is unchanged

For an actual consecutive self-tail iteration, the runtime already compares
every raw numeric input with the preceding successful evaluation and copies its
outputs. When no input changed, it now skips calling the C reuse kernel, whose
only remaining work would be testing each operation's zero dependency mask.
Partial changes still use the dependency kernel; fresh calls and stale stamps
still use ordinary evaluation. Current numeric/lookup authority remains checked
by the surrounding consumer, and logical allocations, result identities and
V2 transitions are unchanged. Test counters still report all reused operations.

The [focused receipt](../measurements/74w-unchanged-reuse-focused-correctness.json)
passes the five relevant checks in both debug and release, including registered
mutation and GC differentials, capture/suspension/retirement, actual-loop stamps
and earliest arithmetic failures. The diagnostic timing test remains ignored.
Full production integration and before/after cost evidence follow this checkpoint;
no speedup is inferred merely from removing the C call.
