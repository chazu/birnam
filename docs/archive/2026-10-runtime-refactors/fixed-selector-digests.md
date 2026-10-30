# Fixed selector-name digests

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

This follow-up targets repeated name hashing left after the
[first selector-hashing changes](../../design/selector-hashing.md). A post-change native
sample still placed about 19% of samples in hashing, including roughly 6% beneath
Symbol lookup and representation checks. The experiment holds application code
and the prior runtime improvements fixed.

## Change and language invariants

Each Id Symbol inventory now computes eleven digests at construction, using that
inventory's existing randomized byte hasher. `existing` reuses the digest when
its byte argument equals one of those fixed internal query names. All other
names retain the original hashing path. The additional storage is **88 bytes per
Id instance**, not per Symbol, object or send, and needs no additional allocation.

The cache contains only integer digests. Every query still searches the current
weak inventory and compares full name bytes; it stores no Symbol, method entry,
lookup result or absence result. In particular:

- A query before interning still returns no Symbol and creates no object.
- Retiring a Symbol removes it from subsequent queries despite the retained hash.
- Re-interning the same bytes resolves to the new current Symbol.
- Hash collisions still require full byte equality.
- Current method lookup, custom lookup behavior, admission checks and fallback
  remain unchanged. No new monkeypatch checks or speculative assumptions are
  introduced, and existing deoptimization paths are untouched.

This is derived indexing data, like the digest already stored in each Symbol
record. It uses Id's private byte hasher, not a user-visible `hash` or equality
method. The hasher is immutable for that inventory's lifetime. A new Id builds
its own cache; saved-image loading does not carry cached digests between Ids.
The change adds no language operation or permanent Rust-only policy and can be
carried over when implementing the same owner in Birnam.

## Checks

The new unit test exercises every cached name in three fresh inventories. It
checks missing queries without interning, heap-allocated copies of names,
same-length nonmatching names through fallback, retirement and re-interning.
Earlier collision-chain, canonicalization and accounting tests remain in place.

Lean proves that an agreeing cached digest gives the same answer as full-name
lookup for any current inventory, and includes retirement/re-interning witnesses.
A mutation using another name's cached digest must fail. These are abstract
lookup guarantees, complemented by native tests rather than a proof of all Rust
code. The normal admission/recovery TLA+ cases remain regression checks.

Qualification uses the native replacement/rebinding/GC tests, full module suite,
fresh bootstrap build, mapped/rebuilt saved images and independent Logue/JSON
oracles. Timing prefixes use production libraries without compiler diagnostics;
fixtures requiring diagnostic TCC run in a separate prefix.

## Measurement contract

The comparison baseline already includes digest-index reuse and the five direct
owner selector fields. The candidate differs only in `libbirnam_id.dylib`.
Both applications, their fixtures and saved images are identical between the
compared prefixes. CPU timers exclude startup/preparation and retain normal GC
with the fixed 16 MiB default. All returned application states are checked.

A paired pilot determines whether the change merits qualification. The delivery
comparison runs after validation work finishes; both sets of samples are kept.
Gains are incremental over the previous runtime changes and should not be added
to their percentages.

## Retained result: 2026-10-12

The [measurement receipt](../../measurements/fixed-selector-digests-20261012.json)
contains both paired runs, the exact source delta, runtime/source hashes, driver
and validation bindings. The delivery run uses reversed **BAAB** order after the
ABBA pilot, with no concurrent validation workloads or excluded samples.

| Workload | Already-optimized baseline | With fixed digests | Additional CPU reduction |
| --- | ---: | ---: | ---: |
| Logue, crowded 16-turn batch | 1,095.37 ms | 1,034.28 ms | 5.6% |
| JSON, full 50-record report | 302.32 ms | 281.48 ms | 6.9% |

Each variant has two fresh processes per application and three warmed samples
per process. Logue's four consecutive batches per sample yield 24 batch timings
per variant; JSON has six full-report timings per variant. Both pilot and final
comparisons favored the candidate in both applications. These are local measured
results, not guaranteed gains for every program; noncached names still perform
normal hashing after the byte-name selection.

The measured Id library is identical to the fresh production build. Only that
library differs between timing prefixes; their images, host and remaining
libraries are identical. Independently rebuilt production images also pass
mapped and rebuilt loading, protocol withdrawal, collecting fallback and fresh
target lookup.

Validation passes **11 Id unit tests**, **16 native test binaries/tests**,
**672 module assertions**, Logue's **34 game checks and 24 checkpoints** plus
live replacement, and JSON valid/invalid fixtures, 50/500-record scaling and
edit–rebuild–run. The complete formal gate checks **200 theorems**, rejects **41
mutations** and checks **118 TLA+ configurations**, including counterexamples.
No user installation, commit or push was performed.

This completes the planned additional hashing experiment. Ordinary method
lookup is the next high-level target; no method-lookup optimization is included
in this slice.
