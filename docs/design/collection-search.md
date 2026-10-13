# First-match collection search

Date: 2026-10-13. Baseline: `bac86ae3`.

The third agreed optimization target is shared collection traversal. The
installed `detect:` previously used `inject:into:`: after a non-nil result it
stopped invoking the predicate, but the fold still read and visited every
remaining element. This retained work grows with the collection even when the
first element is the answer.

## Implementation and contracts

`modules/stackless-control.birnam` now installs ordinary `detect:` methods on
Vector, Range and String. They call a shared, tail-recursive
`Collection>>stacklessDetect:from:` driver. It reads the current size, current
element and predicate through ordinary sends, and returns a non-nil result
without traversing the suffix. A false predicate continues directly, avoiding
the fold's intermediate nil accumulator and its next notNil test. A nil match
still continues. The current element
is retained across the predicate; it is not fetched again after a callback.

Map delegates to its current `values` projection's `detect:`. Building that
projection still traverses the map; this does not make Map first-match search
constant-time. Collection retains its original fold fallback so custom
collections need not implement indexing. `detect:ifNone:` keeps its existing
single search and lazy absent block. Neither quantifier (`anySatisfy:` or
`allSatisfy:`) changes: their callback contracts need separate consideration.

All new methods belong to the existing resumable control unit. No Rust code,
compiler special case, method cache, binding guard or native callback barrier is
added. Callbacks can park, resume, collect or be cancelled. Existing compiler
admission and recovery still apply to their ordinary sends. Public methods,
helpers, accessors and predicates remain replaceable. Family overrides use
normal method inheritance; the generic Collection method is the fallback.

This is an authored library algorithm change. It intentionally changes internal
send/allocation traces and avoids suffix accessor effects. It is not a claim
that a compiler may silently rewrite arbitrary user-authored folds, freeze
bindings or erase user effects. No guard attempts to preserve the former
algorithm after someone replaces one of its internal sends. Before a hit,
size and accessors remain live on each iteration; bounds are never snapshotted.
The bootstrap dependency order and the early stdlib fold are unchanged, and the
candidate was built through the production bootstrap process.

Logue's `monsterAt` and `itemAt` now use `detect:` instead of building a complete
`select:` result and reading its first entry. These operate on non-nil records
with side-effect-free coordinate predicates under the application's definitions.
The shared library improvement is measured separately from this application
adoption.

## Verification

The new Lean model proves equal results, predicate state effects and ordered
predicate invocations for a fixed element sequence under canonical
conditional/nil contracts. The abstract predicate may change state used by its
later invocations. Its result preserves non-nil identities, including ordinary
Error values. It does not prove Rust/compiler code, arbitrary accessor overrides
or unchanged allocation observations.

The new bounded TLA+ model exercises parked predicates, replacement, GC,
cancellation and resumption. Required counterexamples reject callback replay,
stale element reads, missing suspended roots and premature termination on nil.
The complete formal gate passes 207 theorems, rejects 43 mutations and checks
123 TLA+ configurations, including expected violations.

Executable coverage includes empty/first/last/absent searches, nil matches,
result identity, predicate order, actual suffix reads, Error values, nonboolean
predicates, fold-only custom collections, all four families, current size and
accessor data, replacement of predicates/helpers/public methods, GC while parked,
cancellation cleanup and one-step Machine budgets. The control smoke gate also
searches 100,000-element ranges to the last match and to exhaustion.

The source-free image test uses precompiled-method replacement through the Id
method table, as existing image replacement tests do. Ordinary Reflect
publication has a separate quiescence contract when an image generation is
running; the initial fixture incorrectly ignored that publication result. This
was reproduced on the baseline and corrected in the fixture, without weakening
the runtime contract.

## Reproduction

Build the baseline and candidate from isolated snapshots at the same source
location with the pinned toolchain. Reuse a Cargo build cache only explicitly.
The source delta between those builds is the authored control module; its host
embedded copy and generated images consequently change too.

```
python3 scripts/collection-search-bench.py \
  --baseline /path/to/baseline --candidate /path/to/candidate \
  --output /tmp/collection-search.json
BIRNAM_TEST_PREFIX=/path/to/candidate bash scripts/birnam-test.sh
bash scripts/stackless-control-smoke.sh /path/to/candidate
python3 scripts/collection-search-image-test.py /path/to/candidate /tmp/search-images
python3 scripts/check-rooting-model.py
```

The microbenchmark checks every search result, warms each case once and retains
all three timed samples in each fresh process. Setup and explicit pre-sample GC
are outside timers; normal automatic GC remains enabled with the fixed 16 MiB
minimum. Process CPU timing includes the loop and result check. Default order is
baseline/candidate/candidate/baseline. The map cases derive target positions from
its actual values order, rather than assuming insertion order.

## Results

The paired runs use two fresh processes per variant, one warm-up per case and
three retained samples per process. No validation workloads run alongside these
timings, and no samples are excluded. Every search result is checked. The host
and all library files are byte-identical between prefixes except
`libbirnam_stacklessControl.dylib` and `libbirnam_host_entry.dylib` (the embedded
source copy); bootstrap/tool images also differ. The controlled source delta is
the search driver.

Median process CPU reductions per search, including repetition/check overhead:

| Family / elements | First match | Middle match | Last match | Absent |
| --- | ---: | ---: | ---: | ---: |
| Vector / 8 | 78.9% | 50.4% | 18.6% | 14.5% |
| Vector / 128 | 98.3% | 54.6% | 17.6% | 15.2% |
| Vector / 1024 | 99.8% | 55.3% | 17.5% | 15.9% |
| Range / 128 | 98.9% | 51.2% | 6.0% | 4.8% |
| String / 128 | 98.3% | 54.1% | 14.0% | 12.5% |
| Map / 128 | 97.0% | 53.4% | 15.8% | 14.8% |

These are local workload measurements, not universal speedups. Range misses show a smaller gain; authored Range size/access operations
remain part of each step. Map still constructs its complete values projection. The direct false
branch improves full scans as well as early matches; no tested case regresses.

Final qualification passes **718 module assertions**, including **46 focused
search assertions**, the 100k control/search gate, mapped/rebuilt source-free
images, Logue's **34 game checks** and **6 checkpoints** plus live replacement,
and JSON acceptance on 50/500-record data, invalid inputs and edit/rebuild/run.
The formal counts above apply to this final source. These changes are in the
working tree; the normal installed prefix was not modified.

### Application separation

The Logue comparison runs original/fold/search/search/fold/original, with two
fresh processes per configuration and three samples of four successive crowded
16-turn batches per process (24 timed batches per configuration). Every state
checkpoint matches the independent Python oracle.

| Configuration | Median CPU per 16-turn batch |
| --- | ---: |
| Original select-first application, baseline library | 985.39 ms |
| Application uses detect:, baseline fold driver | 964.86 ms |
| Same detect: application, new shared driver | 709.01 ms |

Adopting `detect:` alone reduces this workload's median CPU by
**2.1%**. Holding that application fixed, the new shared driver reduces it
by **26.5%**. The combined reduction from the original application and library
is **28.0%**. This distinguishes the language-library improvement from the
application's API choice.

The [evidence record](../measurements/collection-search-20261013.json) retains
all samples, workload sources, comparison driver, runtime/source hashes,
qualification receipts and the formal result. Exploratory timings using a
different build location are retained separately and support no final claim.
