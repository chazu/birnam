# Reuse Execution's private address index

Baseline `59337e33`, 2026-10-31. This changes native metadata lookup, independently
of the pending ordinary-result adapter proposal.

## Required behavior and sequence

Execution must continue finding the same code, Machine run and return records by
object identity. Captured Methods, references reported to GC, retirement, live
replacement, suspension and recovery must retain their existing behavior.

Seven private maps now use Execution's existing `WordIndex`: native-code instances
and Method associations, plus resumable runs, returns, return-Method associations,
registered records and Method associations. Their keys come from object addresses.
The existing address hasher replaces the general-purpose hasher; full `usize`
equality still determines the key. No cache, new index implementation, public Map
policy, object family or Id operation is introduced. These maps were already
unordered; ordered language collections and scheduling sequences are unchanged.

The native-code metadata getter also returns its selected field directly:

```text
Before: find record -> clone record and Rc code reference -> read field -> drop clone
After:  find record -> read field
```

That getter makes no callbacks, allocations or collections after finding its
record. Keeping the native table access open for that read is an implementation
detail, not a Birnam borrowing rule. Paths that retain records across callbacks
still copy and retain the required data.

## Measurements

The final comparison uses copied source-built prefixes with only Execution
changed. Both variants use the existing generated code and authored modules.
Arithmetic and resumable inventory run sequentially in
baseline/candidate/candidate/baseline order, with builds and tests stopped.
Arithmetic has ten samples of 1000 calls per variant and GC mode; inventory has
fourteen samples per fixture and variant. Inputs, compilation and explicit
pre-sample collection remain outside arithmetic timing.

| Workload | Baseline | Changed Execution |
| --- | ---: | ---: |
| Dense authored inventory | 339.70 ms | 316.67 ms |
| Real-module authored inventory | 791.49 ms | 746.17 ms |
| Integer addition, automatic GC enabled | 28.28 µs | 27.82 µs |

Inventory takes about 6.8% and 5.7% less time respectively; all six fixtures match
in every process. Arithmetic changes little, and addition still allocates 40
managed objects. No substantial arithmetic speedup is established. The earlier
index-only comparison also showed about 6% less inventory time; the final figures
above include removal of the metadata clone.

Final measurements are recorded in [result.json](result.json). This is not a new
whole-source build, application benchmark or qualification of the inventory port.

## Validation and architectural review

Workspace all-target/all-feature checking and 320 default library tests pass.
Fourteen integration targets pass across construction purity, captured code and
capture access, native conversions, Integer operation replacement, Machine
execution, candidate retirement, recovery without replay, root lifetime, image
slot restoration and image recovery conflicts.

The native construction test initially stopped because the copied production
prefix intentionally lacked the build-only Lang seed. The seed was built and
supplied for that test; it then passed. The remaining tests ran successfully, and
the temporary seed was removed from the measurement prefix. The initial setup
failure is retained in the evidence. This run does not repeat the full image
roundtrip/resave suite or the three feature-specific compiler memory-metric tests.

The code review checks map-key provenance and unchanged full-key equality,
reference reporting and removal of dead records. It separately checks that the
field getter has no callback while accessing the record. There is no new guard
that freezes method meaning, payload access across owners, dictionary edit during
Method construction, or language ownership rule.

## Rejected local root-buffer experiment

A preceding experiment put sixteen Oops in each generated native call frame and
used heap storage after that capacity. It passed the generated-root lifetime and
native-provider token tests, including buffer growth. Matched Boolean/Integer
rebuilds measured addition at 28.00 versus 27.72 microseconds, with 40 managed
allocations in both variants. That roughly 1% timing difference did not justify
another storage path and 128 extra stack bytes per call frame. It was discarded;
the native backend source and shared build artifact were restored before this
index experiment. It is not an outstanding implementation task.

The receipt records source/library hashes, commands, measurements, test scope and
the rejected experiment's evidence. Raw files are in
`.cargo-target/measurements/execution-address-index-20261031` and
`.cargo-target/measurements/call-root-buffer-20261031`. No user-prefix installation,
push or completion of the overall runtime goal is claimed.
