# Oop-only Id: allocation and interning probes

> **Historical scope:** These are focused representation/calling-convention probes, not whole-runtime or application qualification. See the
> [complete work summary and current checkpoint](logue-runtime-work-summary.md).

Historical pre-cutover probe. Its source is preserved in the linked evidence; the
commands below describe that earlier tree. The current branch is qualified by
[scripts/oop-id-slice-test.sh](../../scripts/oop-id-slice-test.sh) instead.

2026-10-22. Result: the focused measurements support continuing with the strict
boundary. They do not justify raw allocation-size or C-string exceptions. They
also reject creating a fresh name wrapper around each interning request as the
default design. Full language/loader/collector integration remains outstanding.

[Evidence, sources and raw observations](../measurements/oop-id-design-probes-20261022.json)
complement the earlier [C/Rust calling-convention probe](../measurements/oop-id-call-feasibility-20261022.json).

## What was tested

The test-only fixture lives in
`rust/birnam-id/src/id/native_id/oop_boundary_probe.rs`. It uses the production
`ObjectMemory`, `Symbols`, initial Id graph, dictionaries and dispatcher. A
per-context `cfg(test)` extension recognizes odd size immediates as receivers;
there is no global tagged-value state or second dispatcher. Ordinary builds do
not contain this experimental representation or its methods.

- **Size object:** an unsigned tagged immediate. All sizes accepted by the current
  allocator fit; values that cannot fit or would exceed allocation geometry are
  rejected. The allocation experiment compares the same real allocator and
  object initialization with a raw size versus a decoded size Oop. It does not
  allocate a wrapper object.
- **Name object:** an ordinary Id allocation with a behavior and an immutable
  length/inline-byte payload. The same owner creates and interprets that payload.
  There is no borrowed-pointer carrier or separate byte-buffer allocation. The
  existing production symbol table performs canonicalization and owns its own
  copy of the canonical bytes.
- **Raw controls:** byte pointer plus known length, and a C-string pointer whose
  length is scanned. Both reach the same symbol-table algorithm. Prepared-name
  cases exclude construction; fresh-name cases deliberately include it.

Semantic sends in this fixture still use the old ABI's entry glue. Timings isolate
value representation and the real primitive algorithms; they do not include Id
lookup, carrier adaptation, roots or automatic GC. This separation prevents a
partially converted test harness from being called a completed Oop-only runtime.

## Measurements

Two final independent release test processes, ten rounds per case per process,
with alternating/rotated lane order. Measurements use the current thread's CPU
clock (`CLOCK_THREAD_CPUTIME_ID`, Darwin clock 16). Other desktop processes were
active; no concurrent build was launched during the timed loops. No statistical
confidence interval is claimed. The initial exploratory process is retained in
the evidence but excluded from the tables below.

Allocation bursts contain 16,384 objects of sizes 0, 8, 24, 64, 256, 1,024 and
4,096 bytes. Timers include allocation, normal behavior assignment and writes to
a preallocated result vector; disposal occurs afterward.

| Allocation run | Raw size | Size Oop |
| --- | ---: | ---: |
| Process 2 median | 75.65 ns/object | 74.76 ns/object |
| Process 3 median | 57.08 ns/object | 57.06 ns/object |

No consistent tagged-size penalty appeared. The large between-process movement
is a reason not to call the slightly lower candidate medians a speedup. The first
exploratory run was 63.38 versus 64.51 ns/object. These results support cheap size
representation, not faster allocation or completed immediate-object integration.

Interning hit cases cycle through 128 prepared names, 250,000 calls per sample.
The following are medians over both final processes, in ns/operation:

| Existing-name hit | Raw bytes with length | C string | Name Oop |
| --- | ---: | ---: | ---: |
| 8-byte name | 17.15 | 20.97 | 21.46 |
| 32-byte name | 22.62 | 25.70 | 24.97 |
| 128-byte name | 42.39 | 47.03 | 44.51 |
| 1,024-byte name | 318.01 | 338.81 | 329.75 |

The Name path validates allocation membership, behavior and payload extent before
reading bytes; it does not assume any arbitrary object is a valid byte buffer.
It is close to the C-string control, including at the short-name end. Against
the stronger raw-span control, the extra cost is roughly 2–4 ns for short names
and about 10 ns for the 1,024-byte case. No hash or comparison was silently omitted.

Each miss/fresh-name lane starts from a fresh Id floor. There are 8,192 distinct
32-byte source names per sample. Prepared-miss lanes both construct the same
source Name objects before timing, so existing heap population is comparable.

| Case | Raw span | Name Oop |
| --- | ---: | ---: |
| Miss with prepared input | 86.88 ns | 89.75 ns |
| Hit, constructing/discarding a fresh Name each time | 25.23 ns | 55.39 ns |
| Miss, constructing/discarding a fresh Name each time | 79.98 ns | 110.24 ns |

Fresh wrapping adds about 30 ns here: more than double the hit cost and roughly
38% of the miss cost. That is a real penalty, not something to hide by reporting
only the prepared-input benchmark. It argues for actual retained name objects
and direct production of those objects at ingress, rather than reconstructing
names around an unchanged raw-pointer API at every send. The benchmark does not
prove every existing producer can already supply that representation efficiently.

## Semantic results

Debug and release Id suites both pass: **20 tests passed, one explicit timing
experiment ignored**. Five new tests exercise:

1. Size range/allocator-boundary validation, no size-wrapper allocation, exact
   allocated extents, real Id sends to tagged receivers and method replacement.
2. Empty, UTF-8 and long names; canonical identity across distinct input objects;
   changed/dropped original source buffers; canonical bytes surviving input-object
   disposal; embedded-NUL and malformed representation rejection.
3. Replacement during an entered Name method and a nested send observing the new
   method while the entered implementation finishes correctly.
4. Custom metacircular lookup overriding Name interning and ordinary DNU handling
   for an immediate receiver, through the actual production dispatcher.
5. Symbol index retirement and allocation release followed by reinterning from a
   surviving Name object, with a fresh allocation identity even if an address is
   reused.

The initial size-limit test exposed an incorrect expectation in the probe: the
allocator's 16-byte alignment also counts against Rust Layout's signed-size bound.
The corrected boundary test validates the largest accepted extent and rejects the
next size, without attempting a huge allocation. No production limit was relaxed.

The ordinary non-test Id release build also succeeds. It was not byte-identical
to the preexisting target artifact, which was not reconstructed as a matched
baseline. No installation occurred, and this is not an installed-closure receipt.

## Remaining integration work

These results are sufficient to retain the strict design as the next experiment,
not to claim preservation of every runtime feature:

- Production Oop representation, roots, GC tracing, reflection, serialized values
  and images do not yet recognize these immediates. Only the probe's explicit
  dispatch contexts do.
- The Name owner is deliberately colocated with Id for this experiment. Its
  validation can query the local allocation registry directly. If the final owner
  lives elsewhere, use its own metadata or current Id messages and measure the
  additional cost; do not inspect foreign payloads to reproduce this result.
- Direct seed construction is exercised with upper modules absent, but the strict
  native module-loader handoff, full value-owner bootstrap and failure protocol
  have not been implemented or tested.
- Source-buffer and explicit retirement lifetimes were tested. Automatic GC,
  collector reentry, finalization, image restore, suspension, cancellation and
  process workers were not tested with the new representation.
- Existing boxed Integer identity and full-width Word semantics are unchanged.
  The experiment does not authorize retagging those values without their own
  semantic analysis.

The practical conclusion is to keep Oop-only allocation and interning in the
migration design, avoid per-call name wrappers, and qualify the complete ownership
and lifetime path before accepting the production cutover. No raw exception has
been shown necessary by these measurements.

## Reproduce

```sh
MISE_TRUSTED_CONFIG_PATHS="$PWD" CARGO_TARGET_DIR=.cargo-target/simplifications \
  mise exec -- cargo test --offline --locked -p birnam-id --lib

MISE_TRUSTED_CONFIG_PATHS="$PWD" CARGO_TARGET_DIR=.cargo-target/simplifications \
  mise exec -- cargo test --offline --locked --release -p birnam-id --lib

MISE_TRUSTED_CONFIG_PATHS="$PWD" CARGO_TARGET_DIR=.cargo-target/simplifications \
  mise exec -- cargo test --offline --locked --release -p birnam-id --lib \
  timed_strict_boundary_designs -- --ignored --nocapture --test-threads=1
```

Run the timing command serially in fresh processes. Read `OOP_PROBE=` rows as JSON.
Do not compare these core-only nanosecond timings directly with Logue phase timings
or treat the old-ABI semantic fixture as independent qualification of a new ABI.
