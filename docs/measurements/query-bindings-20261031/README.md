# Reuse Id's checked allocation-query bindings until mutation

Baseline `8d6771f0`, 2026-10-31. The arithmetic profile identified repeated checks
inside Id's fast allocation-fact queries. This change reduces those checks while
preserving current lookup and replacement. It does not change Integer policy or
complete the inventory migration.

## Required behavior and implementation

The fast query is available only while Id's lookup and its four relevant query
bindings retain their default meanings. A replaced query must cause it to decline
so callers take the existing ordered message path. Each input object's allocation
record and current vtable must still be checked; those are not retained answers.

Previously every query repeated the callback-free binding verification:

```text
query -> verify default lookup and four query Methods -> read allocation facts
```

Now Id retains a positive proof only when all four bindings refer to the exact
original bootstrap query Methods:

```text
query -> use original-Method proof, or check current Methods -> read allocation facts
dictionary/parent/association change or retirement -> invalidate binding fact
```

The same mutation paths that invalidate Id's existing default-lookup fact clear
this fact explicitly, including after epoch counters saturate. Checking the fact
cannot call user code or collect, so no callback can change the graph between
verification and storing its result. Method creation without installation does
not invalidate it or change binding epochs.

The added state is one Boolean plus references to four existing bootstrap Methods,
kept alive as witnesses through Id's existing permanent roots. Their private fields
have no mutation operation. No new Method is constructed. Equivalent copies and
substitute Methods remain supported through the existing per-call verification;
neither positive nor negative results for those Methods are memoized.

This distinction is necessary: a public-API probe constructs structurally equivalent
Method data in ObjectModel slots, then changes its context without a dictionary edit.
An initial unrestricted cache incorrectly kept accepting the changed Method. That
version was rejected. The regression fails against it and passes with the conservative
proof; restoring the mutable context also works without a dictionary edit.

No object layout, numeric value or allocation fact is stored in the proof. There is
no new service, ABI operation, collector format or language immutability requirement.
The change adds native bookkeeping to remove repeated runtime work; it is not a
Rust source-line reduction or a Birnam migration. The production change adds 32
Rust lines plus tests.

## Measurements

The existing [arithmetic probe](../arithmetic-layers-20261031/probe.rs) ran in
baseline/candidate/candidate/baseline order. Each process used five samples of
1000 calls per case and GC mode. Only the Id library differs between prefixes;
builds and tests were stopped during timing. Selectors, inputs and compilation are
outside the timed ordinary send and Host result-protection boundary.

| Automatic GC enabled | Baseline | Changed Id | Reduction |
| --- | ---: | ---: | ---: |
| Integer addition | 43.24 µs | 40.50 µs | 6.3% |
| Word wrapping addition | 0.99 µs | 0.56 µs | 43.1% |
| Integer equality | 0.44 µs | 0.16 µs | 63.4% |
| Signed addition/overflow helper, given Words | 12.14 µs | 9.97 µs | 17.9% |

Allocation counts are unchanged: 49 for full addition, one for Word addition and
14 for the signed helper. With automatic collection disabled diagnostically,
full addition measured 43.39 versus 40.63 µs. The comparison supports a speedup for
these paths, not application-wide performance or fast Integer arithmetic overall.

The existing complete metadata-analysis operation improved from **10.14 to 8.22 ms**
on the dense fixture and **30.09 to 23.78 ms** on the real module (19.0% and 21.0%).
Those timings include parsing, projection and metadata analysis of an existing
source String, not a whole compilation or application run.

The authored inventory itself, compiled resumably, did not improve meaningfully:
354.82 versus 356.06 ms for the dense fixture and 838.50 versus 829.29 ms for the
real module. Both variants matched all six fixtures in every process. These ABBA
runs used seven samples per fixture, after two warmups, with normal automatic GC.
This change does not qualify the migration.

## Validation and architectural review

The new regression checks warmed queries across replacement, restoration, removal,
changed inheritance and reassociation of Id's receiver. Detaching an ordinary input
allocation changes the answer even with unchanged bindings, confirming that object
facts are read afresh. Further checks cover retirement and saturated epochs. As a
negative control, removing invalidation makes the replacement regression fail.

The corrected implementation passes the independent Id continuation suite,
workspace/all-targets/all-features check and **322 library unit tests**. All **16
integration targets** pass: twelve numeric, replacement, conversion and root targets,
plus full image roundtrip/resave, slot roundtrip, native refusal and snapshot-mutation
targets. The mutable-Method regression is included. Full image checks exercised the
20 resave epochs against the corrected library. The initial cache's green results
were not used as qualification for this version.

Architectural review is separate from tests: the proof concerns Id's own existing
query implementations, not foreign payloads or a fixed language-type catalog.
Ordinary dispatch still performs current lookup and direct Method entry. Constructor
and explicit collection calls remain in place; no constructor, argument, temporary
result, suspension opportunity or failure order is removed by the change.

[result.json](result.json) records the final source/library hashes, pooled medians
and validation evidence. The existing arithmetic and inventory probes were reused;
no new benchmark API was added. Raw evidence is under `.cargo-target/measurements/query-bindings-20261031`.
No installed prefix or remote branch has been changed.
