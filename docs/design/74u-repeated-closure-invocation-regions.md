# 74U — Repeated invocation of one private closure

Status: implementation contract for the next 74T scope extension. Not yet
implemented or promoted. The full numeric, loop, aggregate and closure/capture
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
