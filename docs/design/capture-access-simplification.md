# Capture access simplification

The [receiver-query slice](capture-receiver-query.md) subsequently removes the
factory lookup from ordinary compiled capture loading. The costs and API below
describe this earlier checkpoint.

Follow-up: [Boolean selection simplification](boolean-selection-simplification.md)
reduces addition further to 1,083.375 sends. The measurements below describe the
preceding capture-access slice.

Implemented 2026-10-07, following [closure construction](closure-construction-simplification.md).
The successful Integer addition census falls from **1,728.375 to 1,367.375 sends**,
20.9% fewer. Capture access falls from **544 to 181** sends/addition, 66.7% fewer.
The original pre-construction census was 3,846.375; both slices together remove 64.5%.

## What changed

1. Execution's Word-indexed capture readers no longer call `prepareAllocation`.
   They inspect their own records and copy an Oop without allocation or mutation.
   Input scoping/thread checks remain; allocation/mutation admission elsewhere is
   unchanged. The boxed inspection operations that construct Integers retain it.
2. A synchronous compiled body resolves the current Block owner once, instead of
   once per capture. Actual reads use `captureWordAt:` on the Block through Id.
   This is a separate native execution-reader API; changing the old boxed
   `captureAt:` inspection accessor does not redefine the captured environment.
   Replacing the new reader does affect subsequent real sends. No foreign payload,
   context or stored executable is used to fetch a capture.
3. The owner answers `captureReadModeOf:` before the reads. Default reads borrow
   references covered by the live, initialized Block's immutable capture edges.
   Replaced/custom readers use ordinary per-result rooting, including fresh values
   that are not captured by that Block.

The mode query accepts one Block Object and returns a native Word:

| Reply | Compiled-body action |
| --- | --- |
| 0 | Send the current direct reader and protect every returned Object normally |
| 1 | Send the known pure direct reader; the live Block covers the resulting references |
| 2 | Receiver is not an initialized portable Block; use current owner inspection |

Mode 2 preserves the public `Method invokeOn:` behavior for non-Block receivers,
including nil captures in the synchronous case. A pending image shell cannot
execute. The older factory `captureWord:of:` operation remains for real inspection
callers and this type fallback, not as an old-artifact constructor shim.

## Why borrowing is conditional

A compiler cannot safely treat arbitrary projection results as captures: a replaced
reader can allocate a fresh Object, then another reader can collect it. The new
regression creates exactly that situation with precise GC and verifies the first
result's ordinary root survives the second callback.

The positive mode is admitted only by a callback-free Id-owned binding check. One
request verifies the reader on the Block, the mode query on the current factory,
and the canonical lookup/query services, **including the binding-check query's own
entry**. Expected NativeEntry contexts are opaque equality witnesses, not invoked
or dereferenced across owners. There is no remembered executable dispatch.

If any checked implementation is replaced, absent, or uses custom lookup, borrowing
declines. Borrowing never relies on a sequence of potentially reentrant separate
lookup queries. The existing diagnostic binding-proof toggle still controls ordinary
ABI guard comparisons; this lifetime rule always requires the canonical atomic check.
Word-helper plans now check/read the direct capture operation as well.

All selector identities needed by the loop are resolved before admission. Once
admitted, default projections have no dependency callbacks and owner-thread
execution cannot interleave another task inside the native boundary. A Block's
capture slots are initialized once; supported restoration mutates only pending
shells, which cannot execute. Shared captured Cells remain shared mutable Objects.
The Block and entered Method/Unit keep their existing scopes and lifetime edges.
Subsequent body callbacks still see current dispatch.

Forwarding wrappers on the mode method or the Id binding-check method can change
bindings after calling the canonical implementation. The check includes their own
current bindings, so the canonical implementation declines inside such a wrapper;
fresh replacement results then receive ordinary protection. Providers of native
permission/query operations must honor their result contracts, like other native
ABI providers; this is not a sandbox against native code lying or corrupting memory.

## Costs and measurements

| Capture work per addition | Before | After |
| --- | ---: | ---: |
| Actual capture reads | 69 | 69 |
| Namespace owner lookup, name decoding and owner roots | 345 | 80 |
| Allocation admission before reads | 69 | 0 |
| Per-result capture roots | 61 | 0 on the default path |
| Mode query and current-binding check | 0 | 32 |
| **Capture category** | **544** | **181** |

There are sixteen owner/mode resolutions on this path; a guarded Word helper also
performs direct projection within its existing owner scope. Ordinary cleanup grows
by two sends/addition with the new retained factory references, so the total saves
361 sends, rather than the capture-category difference of 363.

[Receipt](../measurements/capture-access-simplification-20261007.json) records the
hashed before/after runs and the complete disjoint partition. Inputs/libraries
were stable during the final census and timing run; tracked root tokens end at zero.
Physical allocations match the previous construction slice: thirty Id Objects per
addition, including eight branch Blocks. No unused Block virtualization is included.

Separate two-second CPU probes observed median approximately 139 microseconds per
addition before and 126 after, a roughly 9.5% reduction. This is a directional signal
on an active host, not a paired statistical estimate or startup claim. Count changes
are deterministic for this case; counts do not measure CPU shares.

## Validation

Passed affected Execution, native-backend and Id Rust tests, workspace all-target
compilation, and production regressions for:

- default capture reads even when allocation admission is replaced with a failure;
- live Block edges after external capture roots are removed and precise GC runs;
- fresh replacement results surviving a subsequent collecting projection;
- forwarding mode and Id-proof wrappers that change the reader after querying;
- replacement-reader failure propagation and current Word-helper fallback;
- non-Block receiver behavior, source-less synchronous/resumable materialization,
  suspension/resumption and native Unit lifetime;
- capture order/aliases, pending image shell restoration and invocation-input lifetime;
- captured Cells across wait/GC/resume and cancellation/cleanup at task quanta 128 and 1.

The existing full 171-check task suite is recorded for the preceding construction
slice; this slice uses the bounded task probe and does not claim a new full-suite
run or subprocess-worker image acceptance. No full image/startup benchmark was run.
The diagnostic prefix's early authored modules were explicitly recompiled with the
retained Rust seed, then production Bootstrap was restored and the seed removed.
No installed prefix or image is published by this implementation turn.

## Lean and TLA+

[Capture.lean](../../formal/rooting/lean/Capture.lean) proves that separate roots
are unnecessary when each returned result is already reachable from the covering
roots; it proves capture-edge coverage and needed-value survival, and rejects an
uncovered fresh result. Coverage and stable scope/edges are explicit premises.
It is not a proof of arbitrary native replacement methods or emitted C.

[CaptureReads.tla](../../formal/rooting/tla/CaptureReads.tla) checks bounded read,
replacement, collection and cleanup transitions. The safe model passes; omitting
fresh-result roots, granting borrowing incorrectly, ending the scope early and
breaking a capture edge each produce the required counterexample. Pure admitted
reads have no callback transition; ordinary result acquisition retains its existing
ABI lifetime obligation before subsequent callbacks. Result handoff after native
return is the existing runtime contract, not newly proved by this model.

The integrated formal checker passed: 97 Lean theorems, 44 TLC configurations,
and required Lean mutation rejections. These establish abstract rules and negative
controls; the runtime correspondence is supplied by the source audit and focused
production tests above.

The remaining largest send categories are Boolean selection (293/addition), ABI
bridges (281) and Block construction (272). Further changes should target their
causes rather than deleting Id sends solely because a histogram counts them.
