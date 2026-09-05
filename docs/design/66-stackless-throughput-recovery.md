# 66 — Stackless Throughput Recovery

> **Status:** Research and proposed implementation sequence, 2026-09-05.
> Research task: `birnam-v0j`. Publication closeout is committed as `a1a5a69`.
> No throughput optimization is implemented by this document.

## 1. Direction

Start with **indexed weak Symbol lookup**, then measure its effect on complete
workloads before selecting the next optimization. Current profiling identifies
linear symbol interning as the largest individual leaf cost in an empty
stackless loop. This is a small runtime-internal change with useful existing
identity and reclamation tests, and it preserves the language execution model.

The next investigation is allocation in generated control code, especially boxed
Machine operands. It needs a separate semantic analysis: replacing visible
Machine messages with native slot access or reusing observable boxed Integers
would not be a routine implementation optimization under today's contracts.

## 2. Evidence and its limits

### Historical release evidence

[Design 60HB](60hb-v0.14-performance-tradeoffs.md) records the accepted v0.14
tradeoff against the retained v0.11 Rust snapshot: `loops.to-do` 49,574 ns/op,
`query.single-clause` 701.7 ms/op, and `docgen.full` 54.36 s. Those were roughly
237×, 456×, and 30× the prior measurements. They are historical measurements,
not a current-checkout comparison. v0.15 returns to ordinary performance policy;
the v0.14 exception is not permission to relax future thresholds.

### Current quick micro run

After publication acceptance completed, a fresh release-runtime prefix ran the
unchanged micro suite with `BENCH_QUICK=1` (two samples per case). All cases
completed without a benchmark failure.

| Case | Net ns/op | Raw ns/op |
| --- | ---: | ---: |
| `loops.to-do` | 53,425 | 53,425 |
| `loops.whileTrue` | 49,242 | 49,242 |
| `loops.timesRepeat` | 56,609 | 56,609 |
| `sends.unary` | 1,505 | 8,035 |
| `sends.compiled-unary` | 860 | 10,189 |
| `string.scan-derived` | 3,508,443,000 | 3,508,662,000 |

This confirms that loop/control cost remains substantial after the publication
change. It does not establish a regression against v0.14: quick calibration,
two samples, machine noise and subtraction-sensitive send cases are not release
comparison evidence. No new relational or full-docgen timing was collected in
this research pass; the rollout validation task below owns that measurement.

### Isolated control profile

`bench/control-throughput-probe.birnam` loads the time module, disables automatic
GC, performs one collection, then times 100,000 empty `to:do:` iterations.
`scripts/control-throughput-probe.py` runs it in three fresh processes and
checks the loop's receiver answer. A fourth process emits a ready marker before
sampling, separating source compilation from the profile window. Sampling does
not contribute to the timing median.

- Uninstrumented elapsed times: **7,988,805; 7,915,867; 8,135,047 µs**.
- Median: **79,888 ns/iteration**.
- Separate sampled process: 8,387,081 µs; `sample` ran for 3 seconds at 1 ms.
- Main-thread samples: **2,484**.
- Sampled process peak physical footprint: **1.3 GiB**.

| Leaf function/group | Samples | Share of main-thread samples |
| --- | ---: | ---: |
| `Symbols::intern` | 659 | 26.5% |
| Heap address-map insertion | 233 | 9.4% |
| Heap address-map rehash | 218 | 8.8% |
| `Runtime::make_integer` | 152 | 6.1% |
| `memcmp` | 97 | 3.9% |
| Generated vtable cache lookup | 87 | 3.5% |
| Vtable lookup | 76 | 3.1% |
| `resolve_stackless_target` | 72 | 2.9% |

These are leaf samples, not call counts or predicted speedups. Allocation also
appears below generated StepMethods and Integer construction in the call tree.
The long no-GC probe intentionally accumulates objects: its footprint does not
prove a leak or represent default-GC application memory. Its different loaded
module set and long iteration window also make its timing incomparable to the
micro suite's 512-iteration calibrated case. Both harnesses must be compared
against themselves in future A/B runs.

## 3. Source-backed cost model

The current path is:

```text
to:do: -> source-authored stackless control methods and conditional blocks
       -> generated StepMethods
       -> late-bound Machine slot reads/writes and action messages
       -> symbol interning, boxed operands, dispatch and checked frame access
       -> driver Call/TailCall/Return transitions
```

Verified implementation facts:

1. [`src/id/stackless-control.birnam`](../../src/id/stackless-control.birnam)
   replaces production Integer iteration with `stacklessTo:do:answer:`,
   `stacklessToBody:do:answer:` and `stacklessToContinue:do:answer:`. The native
   `integer_to_do` implementation still exists in the Rust prelude, but treating
   it as the production benchmark path would misdiagnose the problem.
2. [`resumable_c.rs`](../../rust/birnam-codegen/src/resumable_c.rs),
   `emit_get`, `emit_store_prefix`, `emit_call_staging` and `emit_publish`,
   generates `birnam_intern`, `MKV` and ordinary `birnam_send` operations for
   frame transport. Slot numbers and action operands become boxed Integers.
3. [`symbol.rs`](../../rust/birnam-id/src/symbol.rs), `Symbols::intern`, scans
   `ordered` with `iter().find(...)`; `contains_bytes` also scans.
   `intern_in_state` can call both in compatibility mode. `by_address` indexes
   object addresses, not bytes, so it does not accelerate these name lookups.
4. The same Symbol table owns weak identities, stable C-name buffers, insertion
   order and post-GC compaction. An index must preserve all four properties.
5. [`runtime.rs`](../../rust/birnam-id/src/runtime.rs) and
   [`execution.rs`](../../rust/birnam-id/src/execution.rs) already share
   resolve-and-pin classification. Frame arenas reuse storage. Temporary
   argument, capture and initial-slot vectors still allocate; their existence
   alone does not make them the dominant cost in this profile.
6. Exact heap membership is already a hash map. Replacing an imagined linear
   heap-membership scan is not a supported optimization direction.

The old benchmark comments describing a native loop and a Zig counter are
historical. The benchmark's stable names and body shapes remain useful, but its
mechanistic explanation needs correction when updating benchmark documentation.

## 4. Bounded implementation sequence

Beads is authoritative for execution status. The dependency order is:

| Slice | Issue |
| --- | --- |
| R1: weak Symbol index | `birnam-symbol-index-oy6` |
| R2: workload validation | `birnam-throughput-validation-ay9` |
| R3: allocation investigation | `birnam-control-allocation-spi` |

### R1 — Index weak Symbol lookup

Scope: `rust/birnam-id/src/symbol.rs`, its caller in `runtime.rs`, focused tests
and a small Rust lookup benchmark. Keep `ordered` as enumeration/name ownership
authority and add a derived byte-lookup index. Prefer safe owned byte keys or
hash-to-index buckets with byte equality; choose based on measured lookup cost
and retained bytes. Do not introduce pointers into movable `Vec` entries.

Required behavior:

- Existing arbitrary non-NUL byte names still canonicalize to one live Oop;
  reject embedded NUL without changing state.
- Lookups allocate nothing on hits and do not scan the complete symbol table.
- Preserve enumeration order and stable `c_name` pointers while symbols live.
- Sweep removes dead index entries before reclaiming names; compacted indices
  are rebuilt correctly. The index does not root symbols or retain dead names.
- Reinterning after collection produces the same fresh-identity behavior as
  today. Independent runtimes and image recovery rebuild their own indexes.
- Compatibility limits and `owned_name_bytes` remain correct. Account separately
  for index overhead if the implementation duplicates key storage.

Tests: extend existing equal-byte, non-UTF8, enumeration/pointer stability and
sweep/reinterning cases with survivors in the middle of the table, all-dead
cycles, growth after compaction, and collision handling where applicable.
Measure hit/miss behavior at multiple table sizes (for example 1k/10k/100k),
including late-inserted names. Timing belongs in benchmarks, not flaky test
assertions. Then run the existing GC/image/lifecycle gates and the same control
probe. Keep the change only with a repeatable benefit or a clearly demonstrated
scaling improvement without significant macro regressions.

### R2 — Validate workload payoff and establish the next profile

Use independently built before/after release prefixes. Run full, non-quick
micro and relational suites with the existing fresh-process discipline, then
`docgen.full` through the canonical benchmark driver. Preserve case order,
loaded module sets, cardinality checks, raw timings and calibrated operation
counts. For small effects alternate A/B order on the same boot.

Report loop, query and docgen improvements separately. The query engine may have
additional costs; a faster empty loop is not evidence of a 456× query recovery.
Retain the tagged v0.14 baseline and publish a separate development comparison.
Reprofile the loop and a representative slow macro to choose the next slice.
An unchanged dominant cost is useful evidence; do not keep stacking optimizations
without measuring the first one.

### R3 — Investigate generated-control allocation

Use diagnostic counters or allocator instrumentation in an isolated build to
separate: boxed slot/action operands, user-visible Integers, literal Blocks,
temporary Rust vectors, and retained arena capacity. Vary iteration count and
run with default GC as well as no-GC to distinguish allocation rate from retained
memory. Include a workload with captures and callbacks, not just the empty loop.

Choose one measured source of allocation. Reusing private Rust scratch may be
possible while preserving the existing rooted-copy-before-reentry discipline;
it must survive nested callbacks without sharing mutable borrows or overwriting
outer operands. Boxed operand caching, direct slot access, fused control or
cross-send specialization need an explicit proof/design decision before code.
This task produces that bounded decision and acceptance criteria; it does not
authorize replacing the late-bound execution model.

## 5. Semantic and release gates

[60E](60e-resumable-registration-and-entry.md) requires protected lookup through
classification and acquisition, including lookup-triggered mutation. No new
per-resolution allocation, mutex, atomic RMW or reference-count work belongs in
that fast path. [60F §4](60f-resumable-compiler-lowering.md) explicitly preserves
late-bound Machine microcode and source sends. [60O](60o-image-v8-identity-preservation.md)
preserves boxed identity. No proposal may assume these contracts away.

For every implementation slice, select the relevant existing tests plus:

- `cargo test --workspace --locked --offline` and the Birnam module suite;
- million-transition stack-depth and callback-control coverage;
- ordinary lookup/override/DNU behavior and mutation during argument evaluation;
- GC during slot transport, nested callbacks, suspension and one-shot restore;
- CodeBinding retention, replacement/rollback, unload and bounded churn;
- image-v8 ordinary/rebuild/baked recovery and symbol reclamation;
- project launch smoke, ensuring throughput work does not undo Design 65.

Before a release, complete Design 60H's immutable-archive acceptance, ordinary
performance comparison and longevity gate. The diagnostic results in this note
cannot substitute for those checks. Do not change thresholds, rename cases or
overwrite historical baselines to make an optimization pass.

## 6. Reproduction and provenance

Build a private release prefix, then run commands from the repository root:

```sh
scripts/rust-install.sh install --root "$PWD" --prefix /tmp/birnam-throughput-prefix --profile release
BENCH_QUICK=1 BENCH_OUT=/tmp/birnam-throughput-micro.json \
  /tmp/birnam-throughput-prefix/bin/birnam bench/micro.birnam
python3 scripts/control-throughput-probe.py /tmp/birnam-throughput-prefix/bin/birnam \
  --sample /tmp/birnam-control.sample.txt
```

Use a fresh/nonexistent private prefix when reproducing. Full benchmark driver:

```sh
BENCH_PREFIX=/tmp/birnam-throughput-prefix scripts/bench.sh --floor=rust
```

Research machine: arm64 macOS 14.6. Runtime source-manifest SHA-256:
`2955a62643ef047406b57af1839a2f44bba104149223894d3c7080342d7406ab`.
Floor dylib SHA-256:
`bef2f84ff0e9233e2be9762e540b75937a0f73efbe1465f911d6658c419cb1eb`.
The private prefix was built before the closeout commit; its source manifest
matches the tested implementation and is the identity used for these runs.

Temporary raw evidence: `/tmp/birnam-publication-closeout.VJrOxG/`.

| File | SHA-256 |
| --- | --- |
| `micro-current.json` | `ca132612d1c8ed960558a329024881bceb40c56cc2afe8766f728662b2c218f9` |
| `control.json` | `373ee3bb3659884172fb4a61be2d9a278795a70e3e71700d1e3cc8fad26b8198` |
| `control.sample.txt` | `6718707847e3fb24ea33e2c421f1460e6372d8fb9af3c92c3cd39ba1d0c9588b` |

The timing samples and relevant profile counts are preserved above so the
conclusions remain reviewable after temporary logs expire. A sample taken during
the earlier module suite included parsing and compilation; it was exploratory
only and is not the evidence used to rank the steady-state optimization.
