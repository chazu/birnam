# Control Methods with inline context references

Baseline: `72e38d80`, following the
[synchronous-entry simplification](../method-source-investigation-20261030/README.md).
This change removes the separate Sequence allocation used by Id's deferred calls
and answer continuations. It does not change arithmetic policy or complete the
authored inventory migration.

Validation completed on 2026-10-31; the directory names the investigation series
that began on 2026-10-30.

## Representation and operation

Before, each control value was a Method whose context referred to another object,
a Sequence holding its arguments or reply state:

```text
prepare control -> allocate Sequence -> allocate Method referring to Sequence
drive control   -> enter Method -> read Sequence context
```

Now the Method contains those references after its ordinary declaration, context,
and code-owner fields. Its context is the Method itself:

```text
prepare control -> allocate Method containing context references
drive control   -> enter Method -> read its own context
```

The bootstrap Method implementation validates the extent and interprets this
representation. Only its own control declarations can have trailing references.
Other Methods retain their existing representation. There is no interior Oop,
new vtable, registry, cache, public message, or native ABI field. Generic allocation
still records allocation facts, and the collector receives references through the
existing Method-owner protocol. Neither needs to interpret control payloads.

Captured Methods retain the original context object, so capture preserves shared
state. Consumed lookup and closed reply contexts still clear their no-longer-needed
references. Prepared calls remain inert until driven; returned continuation values
remain ordinary data unless an explicit control operation uses them. The existing
drive loop, suspension barriers, and code-retention responsibilities remain.

A consumed lookup clears its reference group in one owner-local operation, after
retaining the checked snapshot needed for the next call. It does not reread the
Method layout separately for every discarded argument.

This adds bounded Method-storage access code while removing an object, its length
field, and a separate allocation record from every control value. It reduces
runtime machinery and allocation; it is not a claim of net Rust source-line
reduction. The new helper code belongs to the same bootstrap representation owner
and creates no second object interface.

## Measurements

Apple M3, arm64 macOS 14.6, Rust release libraries. Only Id differs between the
baseline and candidate measurement prefixes. The existing
[probe](../method-source-investigation-20261030/run.py) performs five samples of
1000 wrapped additions per run. Baseline/candidate/candidate/baseline order gives
two runs per variant. Builds and integration tests are outside the timed runs.

| Workload | Baseline median | Candidate median | Reduction |
| --- | ---: | ---: | ---: |
| Wrapped addition, automatic collection enabled | 50.60 microseconds | 46.91 microseconds | 7.3% |
| Wrapped addition, collection disabled diagnostically | 50.52 microseconds | 45.54 microseconds | 9.9% |
| Dense inventory, ordinary collection schedule | 476.21 ms | 381.55 ms | 19.9% |
| Real source-module inventory, ordinary collection schedule | 1141.87 ms | 942.51 ms | 17.5% |

The addition allocates **50 objects instead of 87**, using 2824 instead of 3416
bytes of Id storage. That is a 42.5% object-count reduction relative to this
baseline. Before the preceding change it allocated 115 objects; the cumulative
object-count reduction is about 56.5%, without implying a matching time reduction.

Inventory comparisons use all six existing fixtures and seven samples per run;
all outputs still match the native collector. Collection frequency affects the
timings. Disabled collection is a diagnostic setting, not a production change.
These figures do not establish application-wide speedups. Addition still expands
into too much work, and the inventory port remains unqualified on performance.

[result.json](result.json) records the source/library hashes, exact summaries,
validation outcomes, and local raw-evidence hashes. The existing probes can replay
the comparison against explicitly selected prefixes; no new benchmark framework
was added. Raw results are under `.cargo-target/measurements/inline-control-20261030`.

## Validation and architectural review

The C synchronous-entry cost check failed on the baseline at four allocations and
passes at two. It is explicitly an implementation-cost regression, not a language
allocation-count promise. New unit cases check shared context after capture,
reference removal, marked references, bounds, and rejection of malformed extended
Methods. `python3 scripts/id-continuation-test.py` passes its unit, independent C,
and export checks, including precise-GC root handoff, custom lookup, interleaving,
native barriers, consumed continuations, and captured-code lifetime.

Twenty-one focused native integration targets passed, covering arithmetic bounds,
mixed numeric operations, collecting operation replacements, native adapters,
Method construction and binding purity, roots, memory, Execution, candidates, and
source inspection/loading. The constructor fixtures use a build-only Lang seed;
the integration prefix also supplies TCC resource metrics and a CLI executable.
Those test resources are not additions to the installed product or measurement
prefix. Initial setup failures for a missing executable and seed were corrected
before rerunning the affected checks.

The image roundtrip again passes its ordinary child cases, then fails the same
unsupported-capability save assertion at `image.rs:1014` previously reproduced
with unchanged Id. That assertion was not weakened. This is not a clean full-image
or release-qualification result. No installation or push is claimed.

Architectural review is separate from those results: object identity and shared
context are retained; Method creation performs no dictionary edits; current method
lookup and direct entry are unchanged; only the Method implementation reads its
payload; arguments remain Oops; and there are no language-level ownership or
borrowing requirements. The representation is an implementation choice that a
later Birnam implementation may replace.
