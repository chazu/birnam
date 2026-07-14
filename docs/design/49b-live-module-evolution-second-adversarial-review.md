# 49b — Second Adversarial Review: Simplified Live Module Evolution

> **Status:** Review, 2026-07-14. Reviews rev. 3 of
> [49 — Live Module Evolution](49-live-module-evolution.md) after the design moved
> to one current dispatch generation per runtime and explicitly aligned activation
> with the reactor/isolate safe points in
> [48 — Concurrency](48-concurrency-reactors-and-isolates.md).

## Verdict

**Initial verdict: revise narrowly. Final disposition: accepted after the four
corrections below.** The simplified model resolves the shared-composition defect,
genuinely reduces feature scope, and preserves the concurrency direction. Four
contracts needed tightening before implementation; design 49 was updated in place
and the reviewer verified the corrected revision.

## Blocking findings

### 1. Published source authority was called immutable without a sealed map

The draft rehashed each `SourceRevision`, but did not prevent ordinary Store
mutation from replacing an entry in a published generation's source map.

Failure scenario:

1. Preparation validates ledger revision 9 and its successor map.
2. Commit publishes revision-9 dispatch and the successor generation.
3. A later Store write changes the generation's ledger entry to revision 10.
4. Dispatch remains revision 9 while `fileOut` and image save claim revision 10.

Rehashing the revision bytes alone does not detect a changed association.

**Required correction:** make the complete map a sealed, content-addressed value;
include its digest in generation identity; validate it during publication; and
treat any raw association mutation as generation drift.

### 2. The file workflow overstated the accepted module language

The implemented candidate subset accepts a module declaration plus declarative
method additions/removals. A general module may also execute arbitrary top-level
programs. Publishing its complete document after silently extracting only methods
would make active source claim effects that never occurred live.

**Required correction:** state that the first slice accepts only candidate-safe
module documents over already-established stable targets. Unsupported top-level
forms reject the whole revision.

### 3. Image execution order was no longer explicit

A source map does not supply semantic execution order. The current implementation
enumerates Store attributes, while the module design already defines a topological
dependency order and rejects cycles.

**Required correction:** compile and replay module programs in deterministic
dependency-topological order, with the module manager first. Missing dependencies
and cycles are errors; map or Store iteration order is never executable policy.

### 4. Expected generation identity was not transferable across isolates

A `DispatchGeneration` object belongs to one runtime. Sending its object identity
or local generation number to another isolate conflicts with the copied-value seam
and may either reject equivalent state or accept a numeric collision.

**Required correction:** distinguish runtime-local generation identity from a
value-like base digest. Rollout messages carry the digest; each worker compares it
with its own current generation, binds preparation locally, and returns a
value-like receipt.

## Non-blocking residual risks

- Full-image save uses temporary-file replacement, but is not yet proven durable
  across power loss because the current writer does not fsync both file and parent
  directory.
- The implementation still publishes per-name `activeCompiledUnit` cells and its
  native publication plan has no sealed source map. Design 49 correctly presents
  the one-generation model as a refactor rather than current behavior.
- `prepareDependencies` still discards the direct `Module load:` result; exact
  propagation remains an implementation prerequisite.
- The feature is one vertical contract but should land as end-green sub-slices:
  generation/source authority, file workflow, image recovery, and the scheduler
  seam.

## Explicit concurrency assessment

### Does one current generation resolve shared image composition?

**Yes.** A composed image becomes the predecessor of a whole-runtime successor.
The next module activation chains from that successor rather than independently
superseding a shared per-module base. Native effective-ledger state must follow the
same single predecessor chain.

### Is the first implementation genuinely smaller?

**Yes in feature scope.** Initialization, reflective source patching,
source-associated `adoptLive`, migration, append persistence, multi-module atomic
activation, and cross-isolate atomicity are deferred. The remaining vertical
contract is still substantial and should be implemented incrementally.

### Does the design preserve concurrency and asynchrony?

**Yes, subject to the base-digest correction.** Prepared and rollback capabilities
remain process-local; commits occur between reactor turns or isolate messages;
workers report independent outcomes; copied rollout data and rolling replacement
remain the pool-level policy.

## Resolution after review

Design 49 now incorporates all four required corrections:

1. `DispatchGeneration` uses a sealed, content-addressed source map whose digest is
   revalidated and participates in reproducible generation identity
   ([§3.3](49-live-module-evolution.md#33-dispatchgeneration)).
2. The file workflow explicitly accepts only candidate-safe documents and rejects
   unsupported top-level forms as a whole
   ([§5](49-live-module-evolution.md#5-file-backed-revision-workflow)).
3. Module compilation and replay use deterministic dependency-topological order,
   module manager first, and reject missing/cyclic graphs
   ([§4.2](49-live-module-evolution.md#42-module-metadata-follows-active-source)).
4. Cross-isolate rollout carries a value-like base digest and binds to the
   receiver's runtime-local generation
   ([§8.2](49-live-module-evolution.md#82-isolates-evolve-independently)).

The full-save language now distinguishes temporary-file atomic replacement from
unproven power-loss durability, and the initial work is expressed as four end-green
sub-slices rather than one undifferentiated implementation step.
