# JSON value conversion — 2026-10-12

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

After the unescaping rewrite, the 50-record application still spent about 265 ms
of its 403 ms CPU time parsing JSON. Native PEG matching was only about 14 ms.
The next work therefore targeted authored conversion rather than the PEG engine.

## Changes

`PegCstVt jsonValue:` now stops at the matching kind. Previously it tested string,
number, object, array and literal on every call, keeping the selected result
through subsequent tests. Canonical kinds are disjoint. The fixture has 50 string
values, 150 numbers and 50 objects, reducing kind comparisons from 1,250 to 500.
Array/literal/unknown handling remains explicit, and all comparisons and handlers
remain ordinary late-bound sends.

Profiling that change then identified redundant quoted-string work.
`jsonDecodeString` already makes a fresh String when stripping the quotes. It
now returns that body directly when it has no backslash, calling the current
`jsonUnescape` only for escaped bodies. Empty quoted strings still produce fresh
results. The separate `jsonUnescape` operation retains its own identity contract:
empty input returns itself, nonempty input produces a fresh result.

These are authored algorithm changes. They introduce no method-replacement
guards, native primitives, compiler changes or bootstrap changes. Runtime
deoptimization is unchanged. The new algorithm determines which sends occur;
every send it makes observes the current definitions.

An integer-first conversion prototype was also measured. On top of short-circuit
dispatch, its exploratory full-workload run was about 355 ms versus 368–371 ms
for dispatch alone. It passed numeric boundaries and nested-value checks in the
earlier investigation. It remains deferred: this slice retains the redundant-copy
removal as the next target, leaving numeric conversion order unchanged.

## Validation and measurement

The JSON suite covers all five kinds, unknown kinds, nested values, a comparison
counter showing dispatch stops at the match, and handler replacement between
calls. Quoted decoding tests check empty/plain/UTF-8/escaped contents, fresh
results across calls, live slicing/scanning with collection, and replaced escape
handlers. The existing whole-application oracle supplies independent expected
reports and error cases.

Lean verifies canonical kind selection, the bound on kind checks, the fixture's
750 omitted checks, and freshness when a quoted decoder returns its already-fresh
body. These are scoped models of ordinary contents and identity contracts, not
equivalence of old internal sends under replacement. A wrong-kind-handler mutation
must fail. The existing TLA+ recovery/dispatch models remain unchanged.

The [accepted evidence](../../measurements/json-value-conversion-20261012.json)
records the source/runtime hashes, probes, phase oracles, formal results and
application/image receipts.

## Results

The final comparison uses committed `5c55d42d` versus both retained changes on
one runtime. Medians of six thread-CPU samples per variant, in ABBA process order:

| Workload | Before | After | CPU change |
|---|---:|---:|---:|
| Full 50-record application | 410.4 ms | 352.5 ms | 14.1% less |
| Decode its 250 quoted string tokens | 128.8 ms | 108.7 ms | 15.6% less |
| Decode 250 short escaped tokens | 934.1 ms | 944.1 ms | 1.1% more |

A separate paired comparison isolates the second change on top of dispatch:
372.3 ms to 359.8 ms for the full application, or 3.4% less CPU. These comparisons
are local measurements; their percentages should not be added. The extra scan
has a small cost on escaped tokens, where the existing unescaper still runs.

Separate allocation runs with automatic GC disabled show full-pass additional
live objects falling from 118,554 before these changes to 102,604 after dispatch
and 95,604 after both changes. Allocation charge falls from 2.95 MiB to 2.64 MiB
and then 2.50 MiB. These counters are not peak memory or cumulative normal-GC
allocation totals. The default GC threshold remains fixed at 16 MiB.

Acceptance passed 600 Birnam module assertions (65 JSON assertions), all profiling
phase oracles, the application fixtures and 50/500-record runs, the scripted edit
cycle, and mapped/rebuilt image execution with entry source removed. The formal
gate passed 181 Lean theorems, rejected 35 proof mutations, and passed all 118
TLA+ configurations including required counterexamples. Five theorems and one
proof mutation are new; no new TLA+ model was needed for these authored changes.

No user installation or release was made.

## Reproduction

To compare the committed predecessor with this source on one runtime:

```sh
git show 5c55d42d:modules/json.birnam > /tmp/json-before.birnam
python3 scripts/json-unescape-bench.py \
  --prefix /path/to/compatible/birnam-prefix \
  --baseline-json /tmp/json-before.birnam \
  --work /tmp/json-value-comparison \
  --phases full,decode_strings,quoted_escaped_short
```

The driver checks results against Python, uses separate ABBA processes and three
warmed iterations per process, and keeps normal GC. Phase measurements are
independent workloads, not additive attribution. The short-escape case measures
the cost of the quoted decoder's additional scan when decoding is necessary.
