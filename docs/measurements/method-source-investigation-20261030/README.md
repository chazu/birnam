# Authored traversal costs and direct synchronous entry

This investigation follows the [inventory feasibility experiment](../method-source-inventory-20261030/README.md)
at baseline `58fddef260750bc6a0a07e1b40e6d7f311f9e94f`. It separates traversal,
arithmetic, root handling, and collection costs, then removes one unnecessary
setup allocation pair from Id. It does not complete the compiler migration.

## Findings

The reduced workload is the same 930-node CST containing 32 method declarations.
The [authored cases](walks.birnam) distinguish indexed traversal, bulk traversal
with counting, bulk visitation without counting, and rule reads. Counts returned
by both counting algorithms are checked against the independently projected tree.
The original inventory still returns its 32 exact declaration texts.

Uninstrumented baseline medians, with a precise collection before each sample:

| Operation | Automatic GC enabled | Automatic GC disabled diagnostically | Allocations per operation |
| --- | ---: | ---: | ---: |
| Indexed traversal with counting | 180.59 ms | 176.79 ms | 368,958 |
| `children` and `inject:into:` with counting | 61.40 ms | 60.50 ms | 120,777 |
| `children` and `do:` without counting | 6.92 ms | 6.99 ms | 13,011 |
| Bulk visitation plus `ruleName` | 8.72 ms | 8.83 ms | 14,871 |
| Bulk visitation plus `rule` | 7.14 ms | 7.77 ms | 13,011 |
| Original inventory | 480.14 ms | 207.50 ms | 436,284 |

These algorithms perform different work. In particular, the 7 ms visitor does
not produce a count or recognize declarations. The comparison identifies costs;
it is not a claim that one can substitute it for the complete inventory. Neither
disabling collection nor substituting `rule` for replaced `ruleName` behavior is
a proposed production fix.

The full inventory allocates about 17.1 MB of Id storage per operation. Diagnostic
counters observe one automatic collection in the enabled case, explaining the
large timing difference. The allocation total excludes separate native buffers
and is not an estimate of live memory. Explicit collection between samples is
outside the timer; the earlier feasibility benchmark instead retained the normal
inter-sample collection schedule.

## Where the arithmetic work goes

A wrapper around one addition of existing Integers, including the Host call and
result protection, measures 115 allocations, about 612 Id sends, and 117 root
acquisitions followed by 117 releases. The equivalent equality wrapper allocates
one object and performs about 20 sends. Counters include the small measurement
boundary overhead; these are not claims about an isolated machine instruction.

The addition enters 14 synchronous continuation boundaries and constructs 37
deferred work items. It creates 56 bootstrap Sequences and 51 bootstrap Methods:
107 of its 115 allocations. It also reads captures 44 times. The authored Integer
policy expands addition across bit access, operation dispatch, overflow policy,
and several Block invocations; the control implementation magnifies that work.

Across the full inventory, with automatic GC disabled for this count probe,
there are roughly 2.3 million Id sends and 418,000 root acquisitions/releases.
The same probe counts roughly 11.6 million allocation-record reads. Counts alone
do not establish that these reads or roots can be removed. The initial plain-walk
hypothesis was incomplete: arithmetic and repeated continuation construction
account for much of the difference between a cheap visitor and this prototype.

The temporary [instrumentation](instrument.py) adds counters to an isolated
checkout's Id, Roots, and Collect libraries. It adds diagnostic native exports
only to those copied libraries, not to the language protocol or production source.
Instrumented timings must not be compared with uninstrumented timing samples.

## Implemented simplification

Previously a synchronous native caller entering an already selected continuation
Method constructed an answer context and Method, then constructed another
Sequence and Method representing the deferred initial entry:

```text
selected Method -> answer state -> deferred entry object -> drive -> enter Method
```

The changed [Id entry path](../../../rust/birnam-id/src/id/continuation.rs) instead
starts the same synchronous drive and enters the selected Method immediately:

```text
selected Method -> answer state -> synchronous drive -> enter Method
```

Returned work continues through the existing loop. The shared entry helper serves
both this immediate call and explicit deferred entry, avoiding duplicate behavior.
The drive setup and loop are factored so the synchronous suspension barrier exists
before the initial Method runs. A native call-root scope retains the input values;
the heap answer context and Method remain valid for retained or late replies.

This removes two private allocations per synchronous boundary. It adds no object
family, registry, cache, public message, native ABI field, or alternate lookup path.
It does not special-case Integer or inspect another object's payload. Method
selection, error/answer delivery, explicit prepared work, GC protection, and
rejection of suspension across a synchronous boundary retain their behavior.

## Measured change

Final comparisons used uninstrumented libraries in baseline/candidate/candidate/
baseline order, after builds and integration tests had stopped. Values below are
medians pooled across the two runs per variant. Each addition sample averages
1000 calls; the inventory samples time one operation each.

| Workload | Baseline | Changed Id | Measured reduction |
| --- | ---: | ---: | ---: |
| Wrapped addition, automatic GC enabled | 53.98 microseconds | 51.60 microseconds | 4.4% |
| Wrapped addition, automatic GC disabled diagnostically | 54.06 microseconds | 50.57 microseconds | 6.5% |
| Dense original inventory, normal inter-sample GC | 487.54 ms | 481.86 ms | 1.2% |
| Real source-module inventory, normal inter-sample GC | 1418.68 ms | 1145.23 ms | 19.3% |

The wrapped addition now allocates 87 objects rather than 115, a 24.3% reduction.
This is a useful control-path improvement but arithmetic remains far too costly
to call fast. Inventory timing depends on collection frequency, and the dense
case's small median difference should not be generalized into an application
speedup. All six inventory fixtures still matched the native result in each run.
The [compact receipt](result.json) records exact values, source/library hashes,
validation results, and raw-evidence locations.

## Validation

The independent C caller gained an implementation-cost regression at an actual
synchronous continuation call. It failed on the baseline at six allocations and
passes at four. That bound is an implementation regression check, not a language
allocation-count promise. `python3 scripts/id-continuation-test.py` passes the
full Id continuation checks and its unit/export checks, including custom lookup,
collection, captured contexts, late replies, interleaving, and native barriers.

Focused compiler/Execution/root/source checks and image checks were exercised in
an isolated prefix with the rebuilt Id. The integration fixture also requires
TCC's existing `resource-metrics` build feature; that feature is confined to the
test prefix and excluded from performance comparisons.

The full image test binary is **not green**: `image_owner_roundtrip` fails at the
unsupported-capability save check at `image.rs:1014`. The unchanged Id reproduces
the same failure at the same line with otherwise identical libraries. Ordinary
roundtrip children, repeated resaves, and a minimized save/load/transient-refusal
case passed. The baseline failure has not been weakened or hidden, and this slice
does not claim complete release qualification or repair of that image behavior.

## Reproduction and limits

[run.py](run.py) builds the [standalone probe](probe.rs) under an explicit output
directory. It records library and experiment hashes. Use the baseline prefix
from the preceding experiment and a separate candidate prefix whose only runtime
change is the rebuilt Id. Keep builds and tests out of timed comparisons.

```sh
python3 docs/measurements/method-source-investigation-20261030/run.py --prefix .cargo-target/method-source/baseline-prefix --output .cargo-target/measurements/method-source-investigation/replay
python3 docs/measurements/method-source-investigation-20261030/run.py --prefix .cargo-target/measurements/method-source-investigation/candidate-prefix --output .cargo-target/measurements/method-source-investigation/replay-add --case addPair:with: --repetitions 1000
```

Each variant performs a correctness check before five timed samples. With
`--repetitions 1000`, each sample reports per-operation averages and the raw
diagnostic counters still cover the whole batch. Allocations come from Id's
existing allocation epoch, not a synthetic charge. Both GC settings are included;
the disabled setting is a diagnostic counterfactual only.

To inspect counts, create a fresh detached checkout of the baseline, run
`python3 instrument.py /absolute/path/to/checkout`, and build its Id, Roots, and
Collect libraries in a separate Cargo target directory. Copy only those libraries
into a copied baseline prefix and pass that prefix to `run.py`. Counter arrays
are defined by the instrument script; watched selector names are in the probe.
Never install the instrumented libraries or use their times as performance claims.

Raw data and validation logs remain under
`.cargo-target/measurements/method-source-investigation`. The first candidate
inventory run overlapped integration compilation; it is exploratory evidence only
and is excluded from the final timing comparison. The user's installed prefix
and remote branch have not been changed by this investigation.

## Remaining work

This change addresses one cost in the control implementation. Integer addition
still expands into hundreds of sends and dozens of allocations, and the inventory
port remains unready. The next design question is how to make that arithmetic path
direct while preserving its required replacement and error behavior. The data
does not justify freezing helpers, reinstating a hidden native arithmetic fallback,
or switching implementation language wholesale.
