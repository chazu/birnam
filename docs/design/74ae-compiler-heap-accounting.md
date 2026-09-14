# 74AE — Account for compiler heap storage and complete release

Status: isolated implementation in progress. This addresses the four-family
resource contract; it does not complete that contract or promote an optimizer.

The existing counters measure relocated code/data allocations. They exclude the
TinyCC heap containing compiler state, sections, symbols and transient parsing
storage. Whole-process footprint samples cannot establish how much of that heap
remains owned or whether releasing the final Unit frees it.

## Implementation contract

Add an explicit diagnostic `allocation-metrics` feature to the private compiler
and baseline TCC module. Install one accounting allocator through the vendored
compiler's existing `tcc_set_realloc` hook before its first state is created.
Retain normal allocation alignment, contents across reallocation, zero-size
release and allocation-failure behavior. Count live requested bytes and blocks;
exclude bookkeeping headers and allocator rounding. Free/reallocation must use
the original allocation's recorded size, not a guessed state layout.

All Rust entry points that create a raw compiler state must pass through the
same once-only installation. Never swap the allocator while live states exist.
Snapshots cover this linked compiler instance and must be taken at quiescent
compiler boundaries for a coherent pair. Baseline module and floor-private
compiler instances remain separate. Existing owner pins, Unit release, language
GC and finalizers determine lifetime; accounting must not create extra owners or
reset counters to manufacture a zero result.

The optional baseline selectors expose only diagnostic heap totals. Normal
installations retain their existing protocol. The feature adds no public IdApi,
Method ABI, canonical descriptor or image-format fields. Existing resource-only
and normal builds keep their allocation path unchanged.

On the current Darwin configuration, relocated storage also passes through this
allocator. Heap and relocation totals therefore overlap and must not be added.
Other platform mapping paths can allocate relocation storage outside the hook.
These counters do not measure Rust-owned optimizer/compiler metadata, dynamic
loader allocations, allocator fragmentation or total process memory.

## Acceptance and follow-through

1. Test actual compiler states and native entries through successful compilation,
   compile/relocation failure, overlapping owners, cache-owner release and final
   owner release. Repeat complete lifecycles and require exact return to the
   pre-cycle live heap totals. Never treat a reset or process exit as teardown.
2. Exercise the baseline module through ordinary Id materialization, cache
   eviction, continued invocation while live and explicit collection after final
   release. Require heap and relocation observations separately, in both profiles;
   check that ordinary installations expose neither new selector.
3. Keep these diagnostics in the existing production ownership/source gate, and
   verify normal and diagnostic builds. Revalidate the complete composed source
   before integration; independent optimizer proofs do not transfer automatically.
4. Use the resulting observations for repeated full teardown of the actual four
   consumers. Account for Rust-owned plans, tickets and caches separately, and
   retain normal-installation application/cold/steady measurements. A zero TinyCC
   heap delta alone does not prove the full resource or application obligations.

## First source observation and corrected comparison

The first 4,096-byte source case reached both native calls successfully and
returned exactly to its original heap totals after final release. It nevertheless
failed a new intermediate assertion: heap bytes fell from the initial 385,350
to 331,878 after cache eviction. That assertion had copied a lower bound specific
to the older relocation-only observation.

Six diagnostic processes distinguish accounting from cache effects. Both exact
replays retain the same values. Observing live Unit counts records 9 initially,
10 after materialization, 6 after eviction and 9 after final release; the REPL
queries themselves own compiler Units. Removing only the intermediate eviction
keeps heap bytes at 442,524 and blocks at 189 through final reference clearing,
because the materialization cache still owns the target. The original failed
receipt and all six diagnostic inputs/outputs are retained.

The corrected check retains all original relocation assertions, successful use
after eviction, positive materialization growth and exact final-baseline equality
for every metric. Intermediate compiler heap can shrink below the initial total
when other cache/evaluator Units retire. Counters, roots, finalizers and allocator
behavior are unchanged by this correction. Complete source repetition and normal
build verification remain required.


## Completed focused verification and production queue

Both profiles pass all three private-compiler tests and both baseline TCC tests
with allocation accounting enabled. Sixteen complete private-owner cycles each
exercise successful native code, compile failure, relocation failure, selected
owner retention and exact final heap release. The baseline test also covers
multiple Units and idempotent payload release. Normal and resource-only feature
configurations pass their original suites in both profiles.

All 12 heap-accounting source cases and all 12 original relocation-only cases
pass, across both profiles, two payload sizes and three repetitions. Original
default fixture bodies remain byte-identical. An independent audit reparses all
24 raw outputs, verifies source/module/install hashes and rejects eight malformed
row variants. A newly built ordinary release TCC module separately passes three
normal-protocol/materialization cases; all diagnostic selectors are absent.
These overlays use the existing published host and are not a fresh full install.

Review caught a missing Python import in the new gate integration before its
production job ran. The waiting watcher was intentionally interrupted and its
terminal receipt retained. The repair exposes the exact storage admission helper
for direct checking: ten checks now accept the complete actual matrix and reject
failed/incomplete modes, duplicate/missing cases, different bodies, altered raw
or parsed observations and unreleased heap bytes. Only `recovery-graphs.py`
changed from the focused source checkpoint; the repaired queue verifies that
exact difference and rechecks the unchanged runtime/module/source evidence.

`74ae-production-acceptance-start.json` binds the 297 current inputs and the live
repaired watcher. It waits for the existing counted-closure production process
as a build scheduling dependency, then runs its own 75-command/3,188-case gate
with mandatory heap accounting. A fresh normal release installation and 24
installed storage cases follow successful production verification. Integration,
full four-consumer teardown, Rust metadata accounting and application/resource
acceptance remain pending. No timing or optimizer default changes are claimed.
