# Admit closure code shape once per Method

Follow-up: [receiver capture query](capture-receiver-query.md) removes the ordinary
capture-loading factory lookup. The measurements below describe this admission slice.

Implemented 2026-10-07. Integer addition falls from **829.375 to 717.375 sends**,
13.5% fewer. Closure construction falls from **272 to 160** sends/addition. The
112 removed sends are precisely the eight repeated CodeObject owner/count queries.
Physical Id allocations remain twenty per addition.

## Registration and construction

`Method admitCodeObject:` receives one admitted CodeObject. Execution obtains its
parameter/capture counts through current CodeObject-owner Id messages and checks
resumable descriptor agreement when a descriptor exists. It publishes one immutable
binding containing the CodeObject identity and checked counts in its owned Method
record. This is an assertion about an admitted native Method's code association;
it is not a verifier that arbitrary native machine code implements those bytes.

Generated synchronous and resumable portable Methods invoke that admission during
compiled-unit initialization, before user bodies run. No foreign payload/context is
read; no selected foreign executable is cached. Named adapters with no standalone
portable program retain their existing construction route.

Generated Block literals now send `fromMethod:captureOperands:` with a Method and
ordered Object captures. The Method supplies its admitted code/counts; the Block
checks capture count and creates its existing owned representation. CodeObject is
not passed again, and no shape-owner query occurs during ordinary literal creation.
The old compiler-only `fromMethod:codeObject:captureOperands:` selector is retired.

Input scopes are established before outgoing messages. No Method-map borrow spans
an owner callback. Reentrant admission cannot overwrite a competing binding or
publish inconsistent checked facts. A same-CodeObject admission is idempotent and
returns the existing Method without another query; a different-CodeObject rebind
is rejected. Failed admission leaves an unbound Method available for a valid retry.

## Identity, authority and lifetime

CodeObject's semantic program state is admitted and immutable. Checked facts move
into Method registration instead of being re-requested for every Block. Replacing
shape-query methods affects subsequent admission or external validation; it does
not rewrite a Method's previously admitted counts. This is an explicit registration/
constructor contract change, not permission to bypass an unchanged owner's API.

Execution traces Method → admitted CodeObject. Capturing a Method alias copies the
binding along with its existing owner/code-generation lifetime. Block code and
capture references keep their existing GC representation. Methods/Blocks are still
ordinary Objects; constructors, admission and actual invocation use current Id.

The materialization cache keys complete canonical bytes and may reuse one Method
for distinct CodeObjects with the same program. The collection-based
`fromMethod:codeObject:captures:` and pending image-shell route remain for these
real external pairings. A pairing with the Method's bound CodeObject reuses its
facts. A different supplied CodeObject is queried through its current owner and
must match the Method's admitted shape/descriptor. The Block retains the supplied
CodeObject identity rather than silently substituting the Method's default code.
Methods without a portable binding retain descriptor/native construction semantics.

This deliberately preserves different-code identity and callback validation where
an external pairing actually requests them, without retaining the retired generated
constructor for old artifacts. Old compiled literals require explicit rebuilding;
image startup does not silently recompile supplied machine code.

## Measurements

[Receipt](../measurements/method-code-admission-20261007.json) contains hashed traces,
source/library identities and short timing observations.

| Per successful addition | Before | After |
| --- | ---: | ---: |
| Total sends including amortized fixture work | 829.375 | 717.375 |
| Closure construction with preparation/roots | 272 | 160 |
| Parameter/capture shape queries | 8 each | 0 |
| `admitCodeObject:` calls inside warmed addition | 0 | 0 |
| Constructed branch Blocks | 8 | 8 |
| Id Objects allocated | 20 | 20 |

The default constructor subtree falls from 28 to 14 sends; current Block factory
lookup and result roots make the per-literal cost 20 rather than 34. Admission is
initialization work and must be counted when measuring setup; this is not a startup
speed claim. Blocks still receive separate behaviors and `value` bindings.

Separate two-second CPU probes observed candidate medians approximately
85.8 microseconds/addition before and 76.4 after, about 11% lower. These are
active-host directional measurements, not a paired speedup confidence interval.
Inputs/libraries remained stable in the final census/timing and tracked Root tokens
end at zero. Capture access (174 sends) is now larger than construction (160).

## Validation

Passed workspace all-target compilation and Execution/native-backend tests, plus
production integration checks for:

- failed/idempotent/rebinding admission; no fast construction before admission;
- precise GC after an external CodeObject root is dropped during its admission
  query, then Method-only code retention and captured Method aliases;
- repeated construction without new shape queries, including readers replaced
  with failure replies after admission;
- current reader failures, malformed counts, descriptor mismatches and explicit
  external-code identity preservation;
- capture count/order/aliases, zero/twelve captures, collecting constructor
  admission and private Block clones;
- synchronous/resumable materialization, suspension/resumption and native Unit
  retention/release;
- pending image-shell restoration, Integer/Float/error oracle, Boolean and typed
  ABI behavior, replaced capture readers and scoped fresh results;
- captured Cells across wait/resume/GC, cancellation and cleanup at quanta 128/1.

The diagnostic prefix's early authored modules were explicitly rebuilt with the
retained Rust grammar seed, then production Bootstrap was restored and the seed
removed. No installed prefix/image is published by this turn. No new formal proof
of the implementation, full image/worker acceptance, full 171-check task rerun or
startup benchmark is claimed. Existing reachability rules and focused tests provide
this slice's lifetime evidence.

Sharing behaviors/method installation remains an investigation: behavior identity
and direct/per-instance mutation are observable. This slice changes only repeated
shape admission; it does not share behaviors or virtualize unselected Blocks.

```sh
python3 scripts/microbench.py --case public-integer-default --counts-only --trace \
  --output /tmp/birnam-method-shape-after/final-trace.json
python3 scripts/analyze-send-tree.py /tmp/birnam-method-shape-after/final-trace.json \
  --output /tmp/birnam-method-shape-after/analysis.json
```
