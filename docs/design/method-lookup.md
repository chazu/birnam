# Ordinary method lookup — 2026-10-12

The previous application, bounded-log and selector-hashing changes were committed
as `29cf6de6` before this investigation. The baseline here includes all of those
runtime improvements. Application code stays fixed.

## Retained change

Id's private `Vtables::lookup` dictionary walk is marked `#[inline(always)]`.
The ordinary inline hint did not help in the pilot, but forcing the private
helper into its callers did. The algorithm and its data structures are unchanged.

This exposes the callback-free walk to the Rust optimizer and removes the
standalone helper call/return path. The optimized library no longer contains an
out-of-line `Vtables::lookup` symbol. Its file size falls from 470,480 to 468,784
bytes on this build, rather than increasing. Symbol absence demonstrates the
code-generation change; application timing establishes whether it helps.

Birnam sends still perform current lookup. The helper still checks registered
vtables, validates selector-position hints against current keys, reads current
values and follows current parent links. Opaque nonmethod entries still shadow
inherited methods. The dispatcher's checks for replaced `lookup` implementations
and custom metabehavior remain untouched, as do all existing admission and
recovery paths. There is no cached binding, new guard, new state, object-layout
change or bootstrap policy. No Rust-only language operation is introduced.

## Alternatives tested

All pilot application runs check their outputs against the Python oracles.
The percentages below are exploratory median CPU reductions; negative values
mean a higher measured median, not a proven universal regression.

| Candidate | Logue | JSON | Decision |
| --- | ---: | ---: | --- |
| Dense registry records with validated position hints | -2.2% | -1.4% | Discarded |
| Ordinary `#[inline]` on lookup | -1.1% | -0.5% | Discarded |
| Half-sized nonzero-encoded selector-position hints | -1.4% | -1.4% | Discarded |
| Use existing slot hints for dictionaries larger than four entries | -1.2% | -2.2% | Discarded |
| Force inline lookup | 3.7% | 4.6% | Qualify independently |
| Force inline lookup and binding | 2.4% | 4.9% | No convincing extra gain; discarded |

The dense-record experiment stored records in a Vec with an address-to-index
map and used positions only, validating current membership on every hint hit.
It passed additional growth/compaction/address-reuse tests but did not improve
either application, so neither its representation change nor its extra cache
remains in the project. Its source delta and results are kept in the evidence.

The packed slot hints saved two KiB per Id on this host, but showed no CPU gain.
The larger binding-inline variant increased the Id library to 485,216 bytes
without a consistent additional application benefit. These experiments were
removed instead of accumulating speculative complexity.

## Validation and limits

The source algorithm is unchanged, so no new abstract lookup rule is introduced.
The existing Lean/TLA+ checks cover current key/value lookup, parent replacement,
stale positions and recovery. They do not prove Rust compiler transformations;
real native and application tests qualify the produced library.

Validation includes Id's custom-metabehavior and replaced-lookup tests, live
binding/parent changes and address reuse, native replacement/GC/fallback tests,
a fresh bootstrap build, source-free mapped/rebuilt images, the full module
suite and both application oracles. Native fixtures needing TCC resource counters
use a separate diagnostic prefix; timing libraries have those counters disabled.

Because the helper is now inlined, subsequent native samples will attribute its
instructions to its callers. A disappearance of the old profiler symbol does
not mean method lookup costs vanished. The remaining costs still merit measured
investigation; this change does not justify caching executable entries or
weakening current-world semantics.

## Delivery measurements

The [evidence record](../measurements/method-lookup-20261012.json) retains the
experimental sources/results, final source delta, toolchain, runtime hashes,
lookup-symbol inventory and qualification receipts. The final comparison runs
in BAAB order after validation workloads finish, with two fresh processes per
variant, one warm-up and three samples per process. Each Logue sample has four
consecutive 16-turn batches. All samples are retained and every application
result matches the independent oracle.

| Workload | Baseline | Inlined lookup | CPU reduction |
| --- | ---: | ---: | ---: |
| Logue, crowded 16-turn batch | 1,005.53 ms | 983.83 ms | 2.2% |
| JSON, full 50-record report | 281.41 ms | 270.43 ms | 3.9% |

These are modest local gains, incremental over the earlier hashing changes.
The compared host, images and application code are identical; only the Id
library differs. The measured candidate library matches the fresh production
build byte-for-byte. No application-specific behavior was introduced.

Qualification passes **11 Id unit tests**, **16 native test binaries/tests**,
**672 module assertions**, Logue's **34 game checks and 24 checkpoints** plus
live replacement, and JSON acceptance with 50/500-record inputs and the edit
cycle. Mapped/rebuilt images pass protocol withdrawal, collecting fallback and
fresh target lookup. The unchanged formal suite passes **200 theorems**, rejects
**41 mutations**, and checks **118 TLA+ configurations**, including expected
counterexamples. Id's exported ABI symbols are unchanged.

The earlier work is committed as `29cf6de6`; this follow-up remains separate.
The user's normal installation was not changed.
