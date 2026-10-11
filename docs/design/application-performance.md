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
