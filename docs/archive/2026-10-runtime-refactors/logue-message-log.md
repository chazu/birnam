# Bounded Logue messages and post-fix profiling

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Logue said it kept six messages, but its append path copied all entries through
an identity `collect:` and an always-true `select:`. Its end-of-turn trim removed
only one entry, so a turn with several attacks grew the log. The original
[baseline](../../design/logue-performance.md) reached 105 messages after 64 crowded-arena
turns, and successive 16-turn batches became progressively more expensive.

`msg` now appends and, when needed, sends `copyFrom:to:` for the newest six
entries. `msgTrim` also repairs an arbitrarily oversized restored/edited log in
one slice; an already bounded log is left alone. This intentionally fixes the
application's visible log contents. It is not a claim of equivalence with the
old unbounded behavior.

The change uses existing, ordinary late-bound sends. No monkeypatch guards,
method cache, compiler specialization, native primitive or GC policy was added.
The Python oracle now caps messages on each append. The headless regression
checks empty and six-entry Vector identity, append order, bursts without an
intervening turn, restoration of oversized logs, and retained entry identity
through GC. Existing gameplay state, RNG, generation and replacement scenarios
remain checked separately.

## Qualification

- All 34 game assertions and six Python oracle/checker tests pass.
- All 75 benchmark checkpoints and the live Vector-method replacement check pass
  on the same installed runtime used for the earlier baseline.
- Lean proves the canonical suffix and size bound, unchanged short lists and
  append preservation of the bound. The mutation that retains seven entries is
  rejected. The complete gate checks 196 theorems, rejects 39 mutations, and
  checks all 118 TLA+ configurations, including required counterexamples.
- The native sample finishes a 640-turn run checked against the Python model.
  A separate untimed source census checks the same game rules after 64 turns.

The proof is about the intended list policy under canonical operations, not
arbitrary replacements or all generated machine code. Runtime checks cover
object identity and real collection; no bootstrap code changed.

## Profile of the corrected application

A five-second native sample begins after loading, fixture preparation and warmup.
It samples repeated actual turns with normal automatic GC. The completed game
state is checked against the oracle. The sample has 4,186 main-thread samples;
leaf totals from the sampler's collapsed list include:

| Leaf category | Samples | Share |
| --- | ---: | ---: |
| Hashing (`Hasher` / `hash_one`) | 972 | 23.2% |
| Id Vtables lookup | 539 | 12.9% |
| Symbol name lookup | 104 | 2.5% |
| Memory copying (`memmove`) | 161 | 3.8% |

These categories are disjoint leaf classifications, not overlapping inclusive
stack sums. The collapsed list omits entries below five samples, so category
totals are lower bounds. Generated bodies are incompletely symbolized. This was
a diagnostic sample, separate from the quiet paired timings; other validation
work was running during sampling. It cannot establish a proposed speedup or
attribute all hashing to a particular language operation.

The source census adds counters only in a disposable core copy. Over 64 crowded
turns it sees 357 chase actions, 155 attacks and 155 log appends. More tellingly:

| Operation | Calls | Vector rows visited |
| --- | ---: | ---: |
| `monsterAt` | 931 | 7,448 |
| `updateMonster` | 88 | 704 |
| `symEq` enum comparisons | 3,370 | — |

These counts are game-level callbacks/visits, not Id send totals or CPU
percentages. The checked census output preserves the actual game state despite
instrumentation.

## Next target

Investigate **first-match collection traversal** before changing Vector update
storage. `monsterAt` constructs a Vector of every match, then takes the first.
The existing `detect:` avoids further predicate calls after a match but still
traverses the entire collection through `inject:into:`. First compare that
existing API in the application, then measure whether genuinely terminating
search can improve the shared collection driver.

A shared-driver change needs explicit checks for matching nil values, callback
order, live method/state changes, GC, suspension and resumption. Ordinary authored
search code still does not need guards to emulate its former internal sends.
Any runtime specialization bypassing source sends retains its separate recovery
obligation. The hashing/dispatch sample also identifies broad runtime costs, but
neither the sample nor this application justifies weakening late binding.

## Paired result

The [evidence record](../../measurements/logue-message-log-20261012.json) retains the
source/runtime hashes, both oracle versions, comparison driver, raw phase
samples, corrected acceptance summary, formal receipt binding, sampled source,
collapsed native leaves and checked source-census code/results.

The quiet comparison uses the same runtime and harness in **ABBA** order: two
fresh processes per variant and case, three independently reset samples per
process, four consecutive batches per sample. Baseline states use the original
oracle; corrected states use the bounded-log oracle. This deliberately accepts
the intended log-content change while still checking gameplay and RNG results.

| Crowded batch (16 turns each) | Before | After | CPU reduction |
| --- | ---: | ---: | ---: |
| 1 — approach | 1,406.35 ms | 1,367.77 ms | 2.7% |
| 2 | 2,184.07 ms | 1,248.86 ms | 42.8% |
| 3 | 3,263.67 ms | 1,230.06 ms | 62.3% |
| 4 | 4,839.60 ms | 1,222.48 ms | 74.7% |
| All batches | 2,715.76 ms | 1,244.31 ms | 54.2% |

The quiet control changed from 27.99 to 28.09 ms per 16 turns (0.35% slower,
within the small process variation here). The first crowded batch changes little
because few messages have accumulated yet. The later batches become much
cheaper and remain roughly flat with the bound enforced.

The allocation-only first crowded batch charges 7,061,467 bytes and allocates
319,971 objects, versus the earlier baseline's 7,212,035 bytes and 326,918 objects.
That is only about 2.1% less charge in the approach batch; it does not represent
allocation savings in later batches. No claim of whole-language speedup follows
from this application fix.
