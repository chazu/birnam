# Where the remaining Integer addition cost goes

Investigation at `83ae600c`, 2026-10-31. This measures the current implementation;
it does not apply the pending ordinary-return proposal or change arithmetic policy.

The previous [ordinary-return experiment](../native-result-20261031/README.md)
reduced wrapped addition from 50 to 40 managed allocations, with only about a 3%
exploratory time improvement. That is insufficient to make Integer arithmetic fast.
The new measurements locate the larger cost within the existing authored path.

## Results

Release libraries, automatic collection enabled. Each number is the median of
ten samples, five in each of two processes with opposite case order. Each sample
averages 1000 calls. Inputs are existing objects representing 4 and 3.

| Operation | Microseconds per call | Managed allocations |
| --- | ---: | ---: |
| Compiled method returning its argument | 0.29 | 0 |
| Integer equality | 0.48 | 0 |
| Word wrapping addition | 1.01 | 1 |
| Integer to Word conversion | 2.54 | 1 |
| Integer construction from Word | 2.12 | 1 |
| Signed addition and overflow helper, given Words | 12.37 | 14 |
| Exact Error check on an Integer operand | 8.50 | 6 |
| Shared Block returning nil | 0.84 | 3 |
| Full Integer addition | 44.21 | 49 |
| Compiled wrapper calling Integer addition | 45.35 | 49 |

The signed helper includes result construction. Full addition also includes the
operand checks, conversion, Boolean selection, shared operation dispatch and
explicit Block calls in [integer.birnam](../../../modules/integer.birnam).
The intermediate probes return the same numeric answer while entering at progressively
later points in that path. These are separate timings, not additive CPU profiles.

The standalone signed helper accounts for much less time than the complete call;
even the Error check is expensive relative to Word addition. The nil-returning
Block confirms that control construction itself allocates without doing arithmetic.
Disabling automatic collection diagnostically leaves full addition at approximately
the same cost; GC alone does not explain this gap. All detailed cases and both GC
settings are in [result.json](result.json).

Word addition does not implement Integer's signed overflow, mixed-number and Error
behavior. Its roughly 44-fold timing difference is **not a demonstrated achievable
speedup** for Integer. It establishes a useful reference for how much work the
current higher-level path adds. Equality likewise has different semantics.

## Measurement boundary and replay

The [probe](probe.rs) interns selectors, creates inputs, compiles its two small
wrapper methods and checks results outside timing. Timed work is an ordinary Id
send followed by Host result protection and scope cleanup. It does not bypass
method lookup, replace arithmetic methods, inspect foreign payloads or time a
bare CPU instruction. Correctness is checked before and after each case.

Unlike the earlier 50-allocation wrapper benchmark, this probe interns the call
selector outside each timed call. It records 49 allocations for the addition and
wrapper; this is a measurement-boundary difference, not another runtime improvement.
Allocation counts exclude Rust buffers. Explicit collection before each sample is
outside timing. These small-operand microbenchmarks do not establish performance
for overflow, mixed arithmetic, replaced methods, suspended code or applications.

```sh
python3 docs/measurements/arithmetic-layers-20261031/run.py --prefix .cargo-target/measurements/image-refusal-20261031/final-prefix --output .cargo-target/measurements/arithmetic-layers-20261031/replay
```

The [runner](run.py) builds the probe before measurement, then runs forward and
reverse case order sequentially. It records the executable, source, dependency lock,
library/image and raw-result hashes. The recorded prefix combines the previously
validated runtime changes through `83ae600c`; it is not a new clean-source install.
Raw evidence is under `.cargo-target/measurements/arithmetic-layers-20261031/final`.

## Next implementation target

Prioritize the complete authored arithmetic path over further small buffer changes.
Trace and reduce the repeated control construction around ordinary handler selection
and explicit Block entry, and the messages used by the Error check. The data supports
investigating those costs; it does not justify deleting an observable check.

Keep signed policy in Birnam. Any optimization must preserve current method lookup,
replacement effects, constructor effects, error order and real suspension/reentry
behavior. If guarded optimization is needed, derive it from the selected methods
and preserve recovery without replaying effects. Do not recognize a selector spelling
as permission to run hidden native Integer arithmetic. Existing native-operation
recognition is not proof that an authored replacement has the same behavior.

Judge a candidate on full addition and the affected inventory workload, including
replacement and collection checks. Reducing allocation counts alone is insufficient.
The pending independent-root-drive decision remains separate; these measurements
neither answer it nor authorize applying that proposal.

## CPU profile and rejected entry refactor

A subsequent three-second macOS `sample` run observed 2290 stacks during repeated
addition with automatic GC disabled. The largest leaf counts were Id's `behavior`
(370), method selection (228), Method entry (181), `payload` (138), and send (101).
Those five helpers account for about 44% of samples. This short profile points at
the cost of many small sends and repeated allocation-record checks; it is not an
application-wide CPU breakdown. Generated code did not have symbolic names.

An experiment shared already checked Method data within a single entry to remove
duplicate reads in the continuation path. It passed the independent continuation
checks but regressed addition from 44.38 to 47.29 microseconds in an ABBA comparison.
Forcing the new helper inline removed most of that regression, but a second ABBA
comparison measured 43.98 versus 44.39 microseconds. Allocation counts did not change.
Both variants were discarded; production source was restored unchanged. Fewer
source-level reads did not establish a faster or sufficiently simpler runtime path.

[profile-followup.json](profile-followup.json) records the results and local evidence
hashes. The saved rejected patch is experimental evidence, not a recommended change.
The investigation then returned to the stalled migration's algorithm: the
[bulk inventory experiment](../inventory-bulk-20261031/README.md) removes child-index
arithmetic and uses a bounded Word counter for traversal depth.
