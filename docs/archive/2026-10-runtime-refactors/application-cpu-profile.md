# Application CPU profile — 2026-10-11

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

The log summarizer spends most of its processing time decoding JSON strings.
The expensive work is the ordinary Birnam string loop and the runtime work it
triggers, rather than native PEG matching. No runtime optimization was made in
this investigation.

Measured after `52965d84` (selector-table sizing), using its rebuilt diagnostic
prefix on macOS arm64. The [evidence](../../measurements/application-cpu-20261011.json)
records runtime/source/fixture hashes, observations, census sources and raw
artifact locations. The normal installation was not changed.

## Phase measurements

The deterministic fixture contains 50 records, 250 string tokens containing
1,509 body bytes, and 150 numeric tokens. None of these strings contains an
escape. Repeated field names account for 1,300 of the string bytes (86%).

Each phase has prepared inputs and one warm-up, followed by three measurements
with normal automatic GC. Explicit GC precedes each measured invocation.
Preparation, startup and input I/O are excluded. The independent Python oracle
checked every phase's warm result and the complete report/rendered output.

| Workload | Median thread CPU | Additional live objects, GC off | Allocation charge, GC off |
|---|---:|---:|---:|
| Full parse/aggregate/render | 1,989 ms | 471,980 | 11.43 MiB |
| Split and trim lines | 25.8 ms | 6,226 | 0.16 MiB |
| JSON parse and lower | 1,839 ms | 434,737 | 10.53 MiB |
| Native PEG matching | 13.9 ms | 3,312 | 0.06 MiB |
| Lower prepared parse trees | 1,837 ms | 436,934 | 10.58 MiB |
| Validate prepared records | 43.7 ms | 12,511 | 0.25 MiB |
| Render prepared report | 27.8 ms | 8,130 | 0.18 MiB |
| Decode quoted string tokens | 1,707 ms | 385,093 | 9.11 MiB |
| Unescape prepared string bodies | 1,635 ms | 376,843 | 8.98 MiB |
| Convert prepared number nodes | 68.7 ms | 17,264 | 0.34 MiB |

These are independent workloads, **not additive attribution**. Their traversal
code differs, and prepared inputs stay live throughout. In particular, lowering
uses indexed Range traversal while parsing uses Vector traversal. The empty
measurement control takes 0.234 ms and adds 124 live objects.

Allocation columns come from separate single-pass runs with automatic GC off;
normal-GC counter deltas are not cumulative allocation totals. The full pass's
11.43 MiB charge is below the fixed 16 MiB budget. Disabling automatic collection
did not materially remove the processing cost. There is no evidence here for
raising the threshold or adding scaling.

## Why a short string costs so much

[`jsonUnescape`](../../../modules/json.birnam) loops over every byte, even when no
escapes occur. Each iteration indexes the String, checks state and the character,
concatenates output, and constructs a new state Vector. `jsonDecodeString` also
copies the body before entering this loop. On this fixture the standalone
unescape workload costs about 82% as much CPU as the full application.

Separate five-second native samples, begun after preparation and warm-up, agree:

| Leaf category | Full workload | Unescape workload |
|---|---:|---:|
| Hashing symbols | 986 / 4,189 (23.5%) | 1,005 / 4,251 (23.6%) |
| Id Vtables lookup | 516 / 4,189 (12.3%) | 580 / 4,251 (13.6%) |

These are leaf counts, not sums of overlapping inclusive stacks. The sampler's
collapsed list omits symbols with fewer than five samples, making the hash totals
lower bounds. Generated code is incompletely symbolized. The sampled loop also
includes explicit GC between passes. Samples establish where to investigate;
they do not establish a prospective optimization's speedup.

An **untimed**, separately instrumented Id dispatch census decoded the
11-character string `duration_ms` after four warm-ups:

| Census | Sends |
|---|---:|
| Harness returning the input directly | 256 |
| Decode empty string | 1,080 |
| Decode `x` | 14,006 |
| Decode `duration_ms` | 91,509 |

The long-string run therefore adds 91,253 sends over the harness control. Its
counts include 8,151 `at: #Integer` lookups, 8,936 `symbolBytesOf:` calls,
8,557 `retain:`, 7,112 `behaviorEpoch`, 6,004 `nativeBindingsMatch:`, and
5,986 `tryRepresentationIdentityOf:behavior:bytes:` calls. `fromWord:` and
`wordOf:` add 1,816 and 1,800 calls. Interning accounts for only 592 calls.
These counts overlap through nested execution and are not CPU percentages.
Prepared-call entry can bypass a named `jsonUnescape` send, so this is a bounded
measurement region, not a named method subtree.

Source inspection supports the census: Execution's guarded call preparation
checks current lookup/allocation permissions, namespace lookup identity, Integer
factory identity and indexing permissions. Namespace validates Symbol keys via
current `symbolBytesOf:`. Integer conversion and rooting add further traffic.
The guard machinery is doing substantial work inside an otherwise tiny loop.

## Recommended next investigations

1. **Repeated namespace and fast-path admission work.** Trace the dominant
   Integer lookups and guard groups back to their callers. Seek owner-certified
   batching or reuse within valid execution spans, with current-world checks and
   refusal before effects. This has the broadest potential benefit for ordinary
   loops. Epoch equality alone is not permission to skip replaceable callbacks;
   namespace rebinding, representation changes, allocation/indexing replacement,
   observers and recovery must remain correct. Specify the permission boundary
   with Lean/TLA+ and replacement tests before retaining an optimization.
2. **String-loop allocation and the no-escape case.** Investigate a Birnam
   algorithm improvement or generic guarded specialization. Returning the input
   unchanged is not automatically equivalent: nonempty decoding currently makes
   new objects, and String identity and current factories/operations can be
   observable. Preserve those contracts and a deoptimization path. Avoid a
   permanent Rust-only JSON special case.
3. **Measured native hashing overhead.** Inspect Execution's fixed-name and
   owner-local maps. Id's address maps already use a specialized address hasher;
   replacing those is not a new opportunity. Distinguish byte-name hashing from
   address indexing, retain required ordering and collision behavior, and use
   application before/after measurements to judge any change.

PEG migration, raw byte-copy tuning and further GC threshold changes are lower
priorities given this profile. The next implementation should target one of the
above costs, retain semantic replacement/recovery checks, and rerun the full
application as well as its targeted phase. Profiling alone changes no semantic
boundary, so this investigation did not require new formal proofs.

## Reproduce phase measurements

```sh
python3 scripts/log-summary-profile.py \
  --prefix /path/to/compatible/birnam-prefix \
  --work /tmp/birnam-log-profile
```

The runner creates a disposable project/source copy, generates the fixture,
builds the diagnostic project and validates both timing and allocation runs.
All subprocesses have deadlines; no human interaction is required. It writes
`results.json` in the work directory. This is a profiling harness, not a general
correctness or cold-bootstrap qualification suite.

For native sampling the project's `sample` mode warms the selected phase, prints
`ready`, and waits for a line on stdin. An external driver starts `/usr/bin/sample`
and releases it. The exact driver used here is embedded in the evidence record,
alongside the sampled source hash and the separate dispatch-census source.
Raw artifacts live under `/tmp/birnam-cpu-20261011/`; their hashes and extracted
measurements are retained in the repository evidence, but temporary files may
eventually be removed.

The [next investigation](execution-name-access.md) retained cheaper retrieval of
Execution's own selector Symbols and rejected an admission batch with no clear
CPU benefit. It leaves namespace reads and all recovery checks in place.
