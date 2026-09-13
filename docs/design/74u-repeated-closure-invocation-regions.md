# 74U — Repeated invocation of one private closure

Status: initial compiler/runtime implementation verified; not promoted. The full numeric, loop, aggregate and closure/capture
objective remains owned by 74T and `birnam-sik`.

## Problem and intended behavior

The current source closure consumer handles construction and one immediate
invocation. If a successor still needs the closure, it materializes the Block
before returning to ordinary execution. Consequently, two immediate uses cannot
share an allocation-free region. Recent unqualified measurements also show
that admission costs exceed savings for the single-use fixtures.

Extend the existing compiler-derived plan to describe up to 32 consecutive
non-tail invocations of the same source-created closure, with ordinary local
aliases between calls. Keep its original CodeObject, captures and one virtual
allocation identity throughout the admitted region. Recover that identity once
before an observer or escaping continuation. Shared Cells remain original
objects, and calls still resolve through current Id meaning.

This extends the current entry/capture/argument-returning callee scope. It does
not assume that arbitrary closure bodies are pure, that names grant authority,
or that allocation removal is profitable merely because it is legal. Unknown
sends, new literals, writes, additional closures, tail calls and unsupported
control end the region before their execution. The ordinary continuation then
handles them with all necessary identities materialized.

## Compiler representation

Add a bounded sequence description alongside the current first-constructor
plan. Derive it from canonical portable lowering and slot coloring; never
infer it by parsing generated C or trusting producer assertions about native
methods. Retain the existing first-literal provenance and descriptor checks.

For each admitted call, record:

- The original caller PC, call-site identifier, selector, argument sources and
  the original sequence of local stores and integer index allocations.
- Symbolic values referring to original caller inputs or the single virtual
  closure. Propagate previous call results through logical value identities,
  independently of reused physical slots.
- Complete physical closure aliases at invocation, and the original V2 edge,
  parallel copies, result destination and clear bitmap at completion.
- The returned input source implied by the admitted callee plan, plus aliases
  requiring materialization if the region ends at that successor.

Bounds remain explicit: 128 slots, 128 caller blocks, 4096 logical values and
64 KiB of nested program bytes; at most 32 admitted calls. Follow only a bounded
acyclic successor chain. A refused extension retains the valid shorter region;
it cannot silently execute the unsupported operation. Stop before the ordinary
return/tail/observing PC, so its existing driver behavior remains authoritative.

Test symbolic substitution against independent canonical physical execution,
including call-result bindings, argument changes, capture aliases, colored slot
reuse, all V2 copies and clear masks. Compare index-allocation order with actual
baseline emission. Selector spelling and malformed layouts remain refusal cases.

## Runtime execution and recovery

The original C constructor hook remains the entry point. It preserves existing
CodeObject/vtable resolution and capture evaluation, then supplies the floor's
registered plan. The public IdApi, Method ABI, canonical descriptors and images
remain unchanged; sequence metadata is private and derived again when needed.

Before removing the constructor allocation, establish the full region's
allocation budget, including original caller index boxes, intermediate caller
PC tokens, each callee's driver and alias/return boxes, and the original Block
charge. Decline if any original automatic collection boundary would occur inside
the region. Retain allocation/finalizer accounting through the existing journal.
Host scratch containing Oops must remain untraced storage, never a native array
that extends conservative roots across a later collection.

Use the existing current-constructor declaration, current invocation lookup,
registered callee generation lease and exact small-callee plan checks. Every
admitted call must have a valid current target; metadata alone does not confer
authority. Acquire/release leases through existing owner-thread machinery, and
preserve canonical Frame/control guards. No semantic assumption survives an
unknown operation, collecting boundary or reentry.

Execute each original caller prefix and publish its original Call action. On
success, apply the existing exact callee return and V2 caller completion. Charge
each intermediate PC token at its original place before executing the next
prefix. Keep one journal identity and patch all aliases together if recovery
becomes necessary. A forced recovery at each supported call boundary must
return the already-published ordinary Call without repeating construction,
previous calls, argument evaluation or committed effects.

At the region's final successor, materialize the Block if required by live
aliases. Return through the existing Resume mechanism and unwind the original
C constructor/capture locals before the normal driver allocates the final
successor token or enters any observing PC. Intermediate calls occur only in
the admitted region without observers or collection. The driver must validate
the actual final Frame/PC/action tuple even though the original C entry began
at PC zero.

## Acceptance sequence

1. Compiler sequence derivation and differential physical-state tests, with
   one-, two-, three- and 32-call cases and bounded refusal beyond the limit.
2. Real installed source execution through the floor consumer, requiring one
   virtual Block identity, the expected number of substituted calls and exact
   logical allocation charges. Include changed arguments and shared captures.
3. Escape/observer successors, distinct closures from separate factories,
   shared Cell mutation after recovery, changed invocation methods and lookup,
   and exact effects from unsupported continuation calls. Restore original
   methods and require immediate subsequent admission.
4. Forced recovery at every admitted call boundary, collecting successors,
   native observation, precise/conservative GC, allocation-threshold/finalizer
   controls and native ownership/retirement. Extend the existing V2 capture and
   suspension matrix to a region with several successful substitutions.
5. Full debug/release, installed-prefix, image, cold-archive and relocation
   verification. Commit correctness before performance acceptance measurements.
6. Paired fresh-release measurements for single use, repeated use, escape and
   refusal. Record actual hits, physical savings, execution/GC, cold preparation,
   metadata and journal storage. Retain negative results and unchanged host
   admission requirements. Application/resource acceptance remains mandatory.

The implementation must supply executable evidence for these requirements.
Passing the compiler tests alone does not complete this extension, and passing
the extension does not complete the full four-family roadmap.

## Initial implementation evidence

The compiler now derives a bounded `following` sequence alongside the original
constructor plan. Its differential tests cover 1, 2, 3, 32 and 33 uses, capture
and argument results, escaping aliases, prior call results, colored slot reuse,
physical stores and exact baseline-emitter index sequences. Observing operations
and changed call shapes terminate the extension; selector spelling grants no
authority. The 33-use case retains an ordinary continuation after call 32.

The runtime admits a prefix using current lookup, registered callee leases,
receiver capture-layout checks and exact leaf plans. It budgets original Block,
caller/callee index and intermediate PC-token charges before recording one
virtual identity. An inadmissible later callee or insufficient budget retains
a shorter prefix. Each call publishes its original action and applies canonical
V2 completion. The final continuation recovers all remaining aliases before
unwinding through Resume. `BIRNAM_DEV_CLOSURE_FORCE_AT` selects a zero-based
call boundary for forced recovery; existing forced-first-call behavior remains.

A test-floor source runner checks allocation charges, physical savings, completed
substitutions and materialization across both GC modes, escaping/non-escaping
results and every admitted forced-exit index. The existing collecting-successor
fixture now performs three outer substitutions and three nested substitutions.
The explicit capture/suspension fixture also performs three substitutions and
requires that exact count before pausing, in addition to its prior alias, shared
Cell, supplied-result, collection and native-pin checks.

The initial verification attempt caught a diagnostic-filter mistake: accepting
every `closure-elision:` line as a diagnostic also removed the fixture's ordinary
result line. The filter now recognizes only hit and completed-call diagnostics.
The [failed receipt](../measurements/74u-sequence-initial-verification.json) is
retained separately. The [corrected full verification](../measurements/74u-sequence-correctness.json)
passes all 61 commands: 528 workspace tests (19 ignored), 40 sequence comparison
groups covering 640 baseline/optimized/forced process executions, eight existing
measurement groups, eight strengthened capture/suspension groups, 32 live-API
groups, and installed-prefix, image, cold-archive and relocation verification.
The retained source hashes match the tested files. The earlier
[development receipt](../measurements/74u-sequence-development.json) was a focused
run with an older verified Codegen prefix and a newly built test floor; it is
not the production provenance claim.

The consumer remains experimental and disabled by default. Dedicated
allocation-threshold/finalizer coverage for long regions, resource acceptance
and profitability remain open.

## Later-target mutation and immediate readmission

The source mutation fixture uses three calls through one private closure and
aliases its registered `value:` method under an unrelated selector for the
second or third call. After warming, it replaces only that alternate method
with an observing native callback. This requires runtime target admission to
retain a prefix of exactly one or two substitutions and materialize the same
closure before executing the replacement. The callback checks the original
captured Cell and actual argument identity, optionally collects, and records
the receiver address in untraced thread-local storage. The factory must return
that exact receiver, with a fresh closure on each invocation. A root for the
previous result prevents address reuse from invalidating the freshness check;
the currently recovered receiver relies on its ordinary Call/Frame roots.

Eight measured invocations require eight replacement effects, eight optimized
prefix hits when enabled, and zero physical allocation savings for the escaping
results. Without callback collection, baseline and optimized logical allocation
charges must match exactly. Collecting cases deliberately omit that byte-delta
comparison because the collector resets the underlying counter. Restoring the
original registered method must immediately admit all three substitutions on
the very next invocation, without another native effect or a cooldown.

The [focused development receipt](../measurements/74u-mutation-development.json)
passes 16 source processes covering both GC modes, both mutation positions and
collecting/non-collecting callbacks, each with optimization disabled/enabled.
It uses an older verified Codegen prefix and the newly built debug test floor;
the fresh installed-prefix verification is recorded separately. The probe is
available only through the opt-in feature floor and does not change the public
ABI or enable the experimental consumer.

The [full verification receipt](../measurements/74u-mutation-correctness.json)
passes all 61 commands and production verification: 528 workspace tests
(19 ignored), 16 mutation comparison groups covering 32 processes across
debug/release and both GC modes, all 640 sequence executions, eight existing
measurement groups, eight capture/suspension groups, 32 live-API groups, and
installed-prefix, image, cold-archive and relocation gates. All 239 retained
source hashes match the tested files. The fresh-prefix debug matrix's
[individual counters](../measurements/74u-mutation-debug-matrix.json) are also
retained: each non-collecting eight-invocation row charges exactly 10,112 bytes
in either mode. Enabled prefixes complete eight or sixteen calls, with eight
materializations/effects and zero physical savings; the first restored
invocation completes three substitutions and one materialization. Release
matrix success is retained in the full verification output. No timing or
promotion claim is made.
