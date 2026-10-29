# Boolean selection without comparison boxes

Measurement-tool note (2026-10-28): the Operand/Outcome profiling runner and its
disconnected dispatch counters are retired. Commands below using that runner are
historical; use each report's measured revision to reproduce its results. Current
Oop checks include `scripts/oop-id-slice-bench.py` and the
`integer_performance` test. This note does not change the reported measurements.

Follow-up: [fixed ABI call simplification](fixed-abi-simplification.md) reduces
addition further to 829.375 sends. The measurements below describe the preceding
Boolean slice.

Implemented 2026-10-07. Successful Integer addition falls from **1,367.375 to
1,083.375 sends**, 20.8% fewer. Its Boolean-selection category falls from **293 to
13 sends**, and allocated Id Objects fall from thirty to twenty per addition.

## Implementation

`Word selectIfSameObject:with:ifEqual:ifDifferent:` accepts four Object operands:
left identity, right identity, equal result and different result. It returns the
supplied equal result when the identities match, otherwise the different result.
It accepts opaque Object references, not just Words or Booleans. It reads no foreign
payload, sends no dependency messages, allocates nothing and invokes neither result.
All carriers, including the unselected result, must be nonnull Objects. Invalid
count/kind/null carriers produce native `INVALID_REQUEST`. Owner-thread and ordinary
Id ABI argument-lifetime rules remain.

The old Boolean implementation created a Word containing zero/one for each identity
comparison, then validated/unboxed it to select an Object. Addition executed ten
such pairs. Each comparison cost 21 sends; each subsequent selection cost seven.
Temporary allocation, allocator-output validation and root registration accounted
for most of that work.

The Birnam Boolean source now uses the combined operation for all sixteen source
occurrences of that pattern: `not`, `&`, `|`, conditionals, short-circuit combinators
and printing. True/false/nil identities, noncanonical receiver policy, selected
thunks and printing/conversion policy remain authored in Birnam. Execution's existing
deferred-result ABI still invokes the selected thunk through its current Id `value`
message, preserving failures and native result kinds. No Boolean policy moves into
Rust and no native branch execution path is introduced.

The new operation is an ordinary replaceable Id method. Generic older Word
operations remain available; Boolean no longer invokes them for this intermediate
round trip. This is an explicit source/API simplification, not an optimization
that promises to preserve interception of the superseded helper sequence. No
compiler option, permission chain, foreign dispatch cache, ABI field or new native
module is added. The required Rust cold-bootstrap seed remains.

## Measurements

[Receipt](../measurements/boolean-selection-simplification-20261007.json) records
hashed traces, library/source identity and short CPU runs.

| Per successful addition | Before | After |
| --- | ---: | ---: |
| Total sends, including amortized fixture work | 1,367.375 | 1,083.375 |
| Boolean selection sends, including surrounding result roots | 293 | 13 |
| Word Objects allocated | 13 | 3 |
| All Id Objects allocated | 30 | 20 |
| Branch Blocks constructed | 8 | 8 |
| Same-object comparison / unboxing-selector calls | 10 each | 0 |
| Combined selection calls | 0 | 10 |

Each default combined operation is one Id send. The other three sends in the
selection category protect returned Objects in the generated caller. Total sends
save 284: 280 from Boolean selection and four from ordinary cleanup. Against the
original 3,846.375-send checkpoint, the three simplification slices together remove
71.8% of sends. These are send counts, not CPU shares or a theoretical minimum.

Separate two-second CPU sessions observed candidate medians approximately
116 microseconds/addition before and 105 after, about 9.6% lower. This is directional
active-host evidence, not a paired confidence interval or a startup benchmark.
Inputs/libraries were stable during the final census and timing run; tracked
Root tokens ended at zero.

The largest remaining category is ABI bridges: 281 sends/addition (25.9%), followed
by closure construction at 272 (25.1%) and capture access at 181 (16.7%).

## Validation and limits

Passed:

- Boolean identity/truth tables, noncanonical receiver nil behavior, short-circuit
  selection, collecting selected branches and native Object/Unit/Word/Bytes/Entry/
  Protocol result carriers; failures and module reload stability.
- Generic operation identity versus numeric equality, alias/nil inputs, wrong
  argument count/kind/null values, including invalid unselected carriers.
- Pure selection while ObjectModel allocation and Root retention operations are
  replaced with failures: neither is requested.
- A collecting replacement of the new selector, a replacement returning a different
  choice, and replacement failure propagation. Subsequent calls see restored methods.
- Existing native-value/ABI ownership, fresh capture-reader replacement, Word-helper
  fallback, direct capture construction and captured Cells through wait/resume/GC,
  cancellation and cleanup at quanta 128 and 1.
- Word Rust tests and workspace all-target compilation.

The updated diagnostic bootstrap artifacts were explicitly compiled using the
retained Rust grammar seed, then production Bootstrap was restored and the seed
removed. No installed prefix or image is published by this turn.
The full 171-check task suite, full image suite and subprocess-worker image startup
were not repeated. Focused task checks passed; no full-startup speed claim is made.

[Selection.lean](../../formal/rooting/lean/Selection.lean) proves value equivalence
with compare-zero/one-then-select, that the result is a supplied argument, and
reachability/survival when caller inputs are protected. These are explicit premises
about default operation semantics and ABI lifetime, not a proof of arbitrary
replacement providers or emitted C. The operation has no persistent state or
callback transition; no new TLA+ model was needed. The existing forty-four bounded
configurations still pass their required positive/negative outcomes. The integrated
checker verified 101 Lean theorems and rejected a deliberately reversed selector.

Reproduce the census on a compatible rebuilt diagnostic prefix:

```sh
python3 scripts/microbench.py --case public-integer-bindings --counts-only --trace \
  --output /tmp/birnam-boolean-after/final-trace.json
python3 scripts/analyze-send-tree.py /tmp/birnam-boolean-after/final-trace.json \
  --output /tmp/birnam-boolean-after/final-analysis.json
```
