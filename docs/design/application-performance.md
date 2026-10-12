# Application driven performance work

Improve Birnam by building and measuring an ordinary, unattended application:
a JSON Lines HTTP log summarizer. Relational migration remains on hold. Complete
Birnam bootstrapping remains the long-term goal; better usability and execution
costs should make future migrations practical.

Start with per-endpoint request/error counts and response-byte totals, producing
stable JSON output. Add latency statistics after establishing this baseline.
Use the normal project build/run/test workflow and existing language libraries.
A separate Python runner supplies fixtures, checks results and measures processes;
the application's parsing, validation, aggregation and reporting stay in Birnam.

Generate synthetic logs with a fixed seed, skewed endpoint popularity, varied
statuses, payload sizes and latency. Keep inputs free of personal data and
network dependencies. Preserve explicit tiny edge cases and malformed records;
generate larger files outside the source tree. Calculate expected reports with
Python's standard JSON parser and an independent accumulator, and record hashes
of inputs, source and runtime artifacts.

Measure fresh command startup, sustained parse/aggregate/render work in one
process, scaling, peak memory and scripted edit–rebuild–run behavior. Distinguish
build cost, process startup and application work. Every subprocess has a deadline;
a timeout is a recorded result, not a reason to wait indefinitely.

Fix a demonstrated usability problem, then profile the application and retain one
broadly useful optimization only with correctness and before/after evidence.
Preserve late binding, current method replacement, observable execution and valid
deoptimization. Use Lean/TLA+ when an optimization changes those boundaries.
Keep GC's default at 16 MiB without adaptive scaling. Do not infer application
speedups from isolated send reductions or from comparisons with different semantics.

Authored algorithm changes may change their internal sends. Late binding applies
to the messages the new algorithm actually sends; the algorithm does not need
guards that reproduce its former implementation under method replacement.
Compiler/runtime optimizations that bypass source-level sends still need the
appropriate guards and deoptimization. Keep these two obligations distinct.

## First cycle

The runnable [log-summary project](../../examples/log-summary/README.md) and
[sample-data generator](../../scripts/log_summary_data.py) implement the counts,
errors and byte-total baseline. [The runner](../../scripts/log-summary.py) checks
valid/invalid input, deterministic output, project tests, scaling and an automated
edit–rebuild–run cycle. It uses a disposable project copy and requires an explicit
runtime prefix. The source library now has a project manifest, making JSON and
other authored modules available as explicit local dependencies.

Profiling selected Id's dictionary lookup for the first runtime change. Validated
position hints for larger dictionaries reduced warm processing of 50 records
from 2.09 to 1.99 seconds in a separate-process ABBA comparison (about 4.8%).
Hints never store executable methods or parent resolution; each lookup checks
the live key and reads the current value. Failure to validate uses ordinary lookup.
Formal checks and runtime/image replacement checks passed.

The [evidence record](../measurements/log-summary-20261011.json) separates this
local result from remaining costs. Startup still peaks around 2.4 GiB, and the
5,000-record baseline exceeded its 30-second deadline. Latency statistics and
larger parser improvements remain later steps; no installation or release was made.

## Startup memory follow-up

The [startup investigation](../startup-analysis.md#october-11-startup-memory-investigation)
found overlapping image buffers and external Bytes capacity missing from GC
accounting. Shorter scratch lifetimes and capacity accounting reduced median
startup peak RSS from 2.37 GiB to 1.49 GiB, with the fixed 16 MiB budget unchanged.
Startup rose from 2.98 s to 3.46 s. This is a memory improvement with a measured
latency cost; the remaining roughly 1 GiB after collection still needs attribution.
New allocation-byte diagnostics include reported external capacity, so they are
not directly comparable to earlier managed-object-only counts.

A [second investigation](../startup-analysis.md#october-11-retained-footprint-and-executable-allocation)
found executable units fragmenting macOS malloc regions. Dedicated VM mappings
reduced median post-GC physical footprint from 685 MiB to 341 MiB, with startup
and warm processing essentially unchanged. Peak startup RSS remains about
1.47 GiB: retained physical footprint, peak RSS and live allocation counters must
be tracked separately. Duplicated generated scaffolding is a promising next target.

The [helper-emission follow-up](../startup-analysis.md#october-11-emit-only-referenced-private-helpers)
marks private C helpers `static inline`, allowing TinyCC to omit bodies with no
references. Paired rebuilt prefixes reduced bootstrap/tools images by roughly
15%, startup time by 4.8% and peak startup RSS by 14%; warm processing remained
unchanged. Helper bodies, language dispatch, recovery paths, unit ownership and
selector-table capacity are unchanged. Savings require rebuilding existing
artifacts and images.

The [selector-table follow-up](../startup-analysis.md#october-11-size-selector-tables-per-generated-unit)
selects 64..512 static entries per unit from an estimated name inventory, retaining
ordinary full-table fallback and current dispatch. Bootstrap BSS fell from
30.1 MiB to 8.0 MiB, but page-backed native storage fell only about 8.4 MiB.
Startup, warm processing, image size and peak RSS were essentially unchanged.
Record this as a modest storage saving; the next cycle should return to profiling
application CPU costs.

The [CPU profiling follow-up](application-cpu-profile.md) finds JSON string
unescaping dominant: 1.63 seconds for the fixture's strings versus 1.99 seconds
for the full 50-record application. Native samples and an untimed dispatch
census identify repeated namespace, guard, rooting and conversion work inside
the string loop. The phase harness checks independent results; this cycle adds
measurement evidence without changing runtime semantics.

The [Execution follow-up](execution-name-access.md) avoids repeated hashing when
retrieving eight retained selector Symbols. The final local comparison reduced
full application CPU by 2.3% and string-unescaping CPU by 1.9%, preserving current
namespace reads, admission and recovery. A separate admission-batching experiment
was removed because its send reduction did not produce a clear CPU improvement.

The [JSON algorithm rewrite](json-unescape-investigation.md) scans for backslashes
and copies unescaped nonempty bodies once. All operations use ordinary current
dispatch. Paired measurements improved the full application from 1.92 s to 405 ms
and string unescaping from 1.60 s to 86 ms; a short-escape microcase was 1.5% slower.
Full-pass allocation charge fell from 11.43 MiB to 2.95 MiB. The fixed GC budget,
runtime guards and bootstrap behavior are unchanged.

The [value-conversion follow-up](json-value-conversion.md) stops kind dispatch
after a match, then removes the second copy of unescaped quoted string bodies.
Together they reduced full application CPU from 410 ms to 352 ms (14.1%) in a
paired comparison. The second change independently saved 3.4% on top of dispatch;
short escaped tokens cost 1.1% more. The full pass now adds 95,604 live objects
and 2.50 MiB of allocation charge with automatic collection disabled for counting.
