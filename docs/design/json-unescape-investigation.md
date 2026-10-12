# JSON string decoding — 2026-10-12

The decoder now scans for a backslash before entering its per-byte escape loop.
Empty input returns itself. A nonempty body without a backslash uses one
`copyFrom:to:` send, preserving a fresh result. Bodies containing escapes retain
the existing decoder and its documented `b`/`f`/`u` limitations.

## Language contract

This is an ordinary Birnam algorithm rewrite. An authored implementation may
change which messages it sends. Each operation in the new implementation uses
current late-bound dispatch; the decoder does not inspect bindings or check for
method replacement. The runtime's existing deoptimization machinery is unchanged.
Runtime optimizations that bypass source-level sends have separate obligations
and must still preserve those sends' meaning under replacement.

For example, overriding a plain input's `at:` no longer affects this decoder's
no-escape path, because that path uses scanning and slicing. Replacing its
`includesSubstring:` or `copyFrom:to:` does affect it immediately. This is the
explicitly agreed source-algorithm contract, not equivalence of the old and new
internal message traces.

No Rust JSON helper, native protocol or bootstrap change was introduced. The
algorithm uses existing String operations. Nonempty outputs remain fresh, even
though `jsonDecodeString` has already copied the body out of its quoted token.

## Validation

`modules/json-test.birnam` is included in the ordinary module test gate. Its
34 assertions cover UTF-8, contents, empty and nonempty identity, escaped quotes
and backslashes, existing unsupported-escape behavior, and keys/values together.
Private replacements of scanning, slicing and escape decoding verify current
ordinary sends, including a method replaced between successive calls.

The independent prototype corpus supplied 53 expected value cases and four
identity checks. `formal/semantics/lean/JsonStrings.lean` verifies canonical
no-backslash contents and the abstract identity rule. Its model totalizes the
invalid dangling-backslash case; that totalization is outside valid JSON token
semantics and is not a claim about the runtime's error result. Mutations that
skip the scan or reuse the nonempty receiver must fail. The proof does not claim
identical internal sends under method replacement.

The [prototype evidence](../measurements/json-unescape-prototype-20261012.json)
records the initial investigation and the replacement-behavior distinction.
The [final evidence](../measurements/json-unescape-20261012.json) records the
retained source, paired measurements and acceptance checks.

## Results

Both variants used the same runtime, including the earlier Execution selector
access change. Each phase ran in ABBA order, with three timed iterations per
process and six samples per variant. Median thread CPU:

| Workload | Before | After | Result |
|---|---:|---:|---:|
| Full 50-record application | 1,922 ms | 405 ms | 4.75× faster |
| Its 250 string bodies, 1,509 bytes | 1,602 ms | 86 ms | 18.65× faster |
| 250 short escaped bodies (`\\n`) | 878 ms | 892 ms | 1.5% slower |

The common no-escape case is the benefit. The extra scan adds work for escaped
bodies; the short-escape microcase shows a small measured cost rather than a
speedup. Existing escape handling remains intact.

With automatic GC disabled for separate allocation measurements, the full
application's additional live objects fell from 471,980 to 118,554 and its
allocation charge from 11.43 MiB to 2.95 MiB. Unescaping fell from 376,843 to
23,417 objects and from 8.98 MiB to 0.50 MiB. Baseline allocation counts come from
the preceding CPU profile of the same workload; these are allocation diagnostics,
not peak RSS or normal-GC cumulative counters.

Validation passed all 569 assertions in the Birnam module suite (including the
34 JSON assertions), all phase-result oracles, 23 application acceptance steps
including 50/500-record runs and edit–rebuild–run, and mapped/rebuilt saved-image
checks. The image replacement probe uses the existing low-level binding protocol:
ordinary Reflect publication correctly refuses while image main is running.
The formal gate passed 176 Lean theorems, rejected 34 proof mutations, and checked
all 118 TLA+ configurations, including required counterexamples. Four theorems
and two proof mutations are new; no new TLA+ model was needed for this algorithm
change.

No user installation or release was made.

## Reproduce the comparison

```sh
git show 52965d84:modules/json.birnam > /tmp/json-before.birnam
python3 scripts/json-unescape-bench.py \
  --prefix /path/to/compatible/birnam-prefix \
  --baseline-json /tmp/json-before.birnam \
  --work /tmp/json-decoder-comparison
```

The runner builds isolated copies using the same runtime, checks each result
against an independent Python oracle, and measures warmed phases in ABBA order.
It includes a workload of 250 short escaped bodies to expose the added scan's
cost when every string needs the old loop. These are independent phase workloads,
not additive attribution. Normal GC retains its fixed 16 MiB default.
