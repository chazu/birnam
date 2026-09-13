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
