# Log summary

An unattended Birnam application for the usability/performance work described in
[the plan](../../docs/design/application-performance.md).

Each nonblank input line is a JSON object with a nonempty `path`, integer `status`
(100–599), nonnegative integer `bytes` and nonnegative integer `duration_ms`.
The first version reports request counts, errors (status >= 400), response bytes
and endpoints sorted by path. Latency statistics are reserved for a later step.
Blank lines and CRLF are accepted. Diagnostics identify the file and physical
line; failures exit with status 2. Output is JSON on stdout; diagnostics use stderr.

From the repository root, with a compatible Birnam binary on PATH:

```sh
python3 scripts/log_summary_data.py /tmp/log-summary-data
birnam run examples/log-summary --build -- /tmp/log-summary-data/tiny.jsonl
birnam test examples/log-summary --build
```

`requires:` names providers in the project dependency graph. The manifest names
the repository's `modules/` source package explicitly; installed runtime modules
are not implicitly copied into a project publication. No network fetch is needed.

The nested `bench` project uses the same application module and measures repeated
parse/aggregate/render passes with input already in memory. Its thread CPU timer
uses the existing macOS Time module; GC runs before, not inside, each timed pass
unless normal allocation triggers collection during the pass.

```sh
birnam run examples/log-summary/bench --build -- /tmp/log-summary-data/small.jsonl 5
```

Synthetic fixtures use a fixed seed and Python's independent JSON/aggregation
oracle. Small/medium/large contain 50/500/5,000 records with skewed endpoint
popularity, varied statuses, sizes and latency. Tiny, empty, escaping, CRLF and
invalid fixtures exercise correctness separately. Generated data stays outside
the source tree; a manifest records its hashes and expected reports.

Full unattended acceptance and measurement:

```sh
python3 scripts/log-summary.py --prefix /path/to/birnam-prefix
```

The runner copies the app and source library into its owned work directory,
checks independent expected reports and error behavior, measures startup/scaling
and warm execution, then verifies an edit–rebuild–run cycle. Scaling timeouts are
recorded separately from correctness failures. Results default to
`.cargo-target/log-summary/results.json`.

The existing Json parser retains its documented escape limitations (`\uXXXX`,
`\b`, `\f`); generated records use raw UTF-8 and supported escapes. This cycle's
[results](../../docs/measurements/log-summary-20261011.json) include the remaining
startup-memory and throughput costs as well as the measured lookup improvement.

For warmed CPU phase measurements with an independent result oracle:

```sh
python3 scripts/log-summary-profile.py --prefix /path/to/birnam-prefix --work /tmp/log-summary-profile
```

The nested `profile` project separates parsing, PEG matching, tree lowering,
string decoding, validation and rendering. Timing runs retain normal GC;
separate allocation runs disable automatic GC for one pass. These phases are
independent workloads, not additive slices. See the
[CPU investigation](../../docs/design/application-cpu-profile.md) for findings
and sampling limitations.
