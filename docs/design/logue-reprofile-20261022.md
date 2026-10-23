# Logue runtime reprofile, 2026-10-22

**Priority superseded:** The subsequent [Oop-only Id decision](oop-only-id.md)
puts removal of the carrier architecture ahead of the optimization experiments
below. The measurements remain baseline evidence. In particular, do not optimize
`Operand`/`Outcome` copying before settling their removal.

Profiled the installed runtime at `a271e4c5`, after the retained frame-table
sharing change. Installation verification passed for the exact file closure,
bytes, receipt and executable. No runtime or application implementation was
changed for this investigation. The proposals below have not demonstrated a
speedup yet.

[Machine-readable evidence and probe sources](../measurements/logue-reprofile-20261022.json)
contain the phase observations, language timings/counts, collapsed native samples,
copy histogram and source hashes. Full raw samples and terminal recordings remain
in `/tmp/logue-reprofile-20261022`.

## Reproduction

Two serial baseline processes exercised the actual Logue functions, native
Termbox and a drained 100×32 PTY: fixed fighter, generation, drawing, floor
transitions, 120 waiting turns, explicit collection. A third process performed
240 alternating horizontal movement actions. The previously interfering external
build was absent. CPU and wall time were recorded separately; no QoS injection
was used. Timing record construction is outside each timed phase, but allocates
and can move collection boundaries. This is not an OS keyboard-to-display test.

| Phase | Wall time observed |
| --- | ---: |
| Initial full-map drawing | 1.54–1.56 s |
| Descent logic | 11.3–12.3 ms |
| Descent drawing | 1.76 s |
| Ascent logic | 9.3–9.8 ms |
| Ascent drawing | 1.17–1.18 s |
| Ordinary waiting redraw, median | 21.7–22.0 ms |
| Waiting redraws with allocation-charge reset | 523–561 ms |
| Explicit collection after waiting | 486–502 ms |
| Movement logic, median | 12.4 ms |
| Movement redraw, median | 20.5 ms |

The movement run has five slow phases (two logic, three drawing), each 536–544 ms
and each accompanied by a charge reset. This strongly implicates automatic
collection in intermittent movement hitches. Charge decreases are an indicator,
not an exact collection count. Floor transitions additionally pay substantial
ordinary full-map execution cost. Presenting the already-built terminal buffer
was about 0.09 ms in both baselines.

## Instrumentation and interpretation

The four hypotheses were execution/root bookkeeping, printing/conversion,
collector tracing/retirement, and immutable ABI metadata copies. Separate
experiments keep instrumentation out of the quiet timing baseline:

- Native sampling: repeated full-map drawing with automatic GC disabled;
  repeated explicit GC over an already-collected game heap; and full-map drawing
  with automatic GC enabled. The no-auto run reached approximately 1.3 GB.
- Native copy interposition: count sizes between `PROFILE_READY` and completion
  of eight full-map redraws. This is a count experiment, not a timing comparison.
- Prepared language microprobes: compilation/checking outside timed intervals,
  three CPU samples, ordinary automatic collection. Separate allocation census
  and dispatch-metrics builds do not contribute timing results.

Rendering had 3,843 main-thread samples; copying was the largest named leaf,
372 samples (9.7%). Most copy stacks immediately enter unnamed generated code.
Binding/dispatch, allocation, root operations and internal hashing also remain
prominent. The auto-GC sample shows substantial metadata removal. The stable-live
GC sample has only 956 main-thread observations and emphasizes tracing and owner
queries: it must not be presented as a breakdown of the half-second garbage-rich
pauses.

Copy interposition counted 258,383,885 calls, of which 250,939,167 (97.1%) were
24 or 40 bytes: 137,619,344 and 113,319,823 respectively. That is approximately
31.4 million such copies per full redraw. Compiling a size probe against the
actual generated prelude confirms `Operand = 24`, `Outcome = 40`. The hook
records these through `memcpy`; the native sampler names the platform copy leaf
`_platform_memmove`. Treat these as combined copy observations, not proof of a
particular exported function or exact per-callsite attribution. The size match,
generated-code stacks and compiler lowering together strongly support ABI packet
traffic as the principal copy source.

Language probes confirm amplification around small source operations: prepared
Symbol identity comparison is 6.3 µs/76 sends, printing one Symbol 38.3 µs/517
sends, and comparing two printed Symbols 73.4 µs/1,025 sends. These include block
entry/return and are not primitive-only timings; send counts are not CPU shares.
The existing Logue `symEq` calls both printers, so its rendering exercises these
paths heavily. Replacing that application expression with identity is a separate
source decision, not a generally valid compiler transformation.

## Recommended experiments, in implementation order

### 1. Direct access to already-owned selector Symbols

**Modules:** `birnam-collect`, `birnam-object-model`, then remaining hot names in
`birnam-execution`.

`Memory::send` and `Model::send` hash a constant byte spelling to recover a Symbol
from their own initialized inventories on every send. Native hash samples have
these helpers as ancestors in both rendering and collection. Execution already
has eight direct hot-name entries, but remaining names use the inventory map.

Replace the hot internal lookup with explicit fields or indexed entries. Preserve
the existing inventory, its initialization/interning sequence and root traversal.
This removes work before dispatch; it does not remove the actual Id message,
cache a method, freeze a namespace value, or bypass an allocator/getter.

This is distinct from the previously deferred generated-unit selector-cache
proposal: it does not alter lazy interning or full-cache fallback. Collect and
ObjectModel already contain individual direct selector fields, so the technique
fits existing ownership rather than requiring a new runtime service.

**Acceptance:** measure owner sends, live/dead GC cases and Logue serial ABBA;
exercise replacement, reentry, roots and image recovery. This is the best first
experiment because it is small and targets both rendering and collection.

### 2. Share immutable native signature metadata

**Module:** `birnam-execution` can own a shared reference to the existing ABI
`Signature`; changing its public representation need not be necessary.

`Execution::invoke` clones a signed Method's `Signature` on every invocation.
`Signature` owns a `Vec<u8>`, so this copies/allocates immutable convention bytes.
Captured Methods clone the same data too. Use immutable shared ownership while
retaining the Method's convention and executable lifetime.

This is source-confirmed unnecessary copying, but its individual CPU contribution
has not been isolated. Expect a smaller opportunity than generated packet copies.
Do not hold a registry borrow across a callback just to avoid the clone. Current
argument conversions, factories, failures and dispatch must remain unchanged.

**Acceptance:** signed invocation at multiple arities, including nullable and
failure-observing signatures, mutation/reentry, capture/lifetime/image checks;
measure host allocations as well as CPU. The previously rejected argument-vector
normalization experiment is a different change and remains rejected.

### 3. Lower small aggregate copies directly in the native compiler

**Module:** the vendored TinyCC ARM64 backend, with its generic struct-assignment
seam in `vendor/libtcc-src/tccgen.c`; alternatively a narrowly justified generated
helper adjustment in `birnam-native-backend`.

`vstore` lowers struct assignment to an out-of-line `memmove` unless a target
provides `TCC_TARGET_NATIVE_STRUCT_COPY`. No target implementation is present in
this checkout. The generated prelude passes and returns 24-byte `Operand` and
40-byte `Outcome` structures throughout its helpers.

Implement a bounded constant-size copy lowering, retaining the existing fallback.
Register loads/stores could avoid repeated library call overhead for these tiny
packets without changing the ABI. Preserve required overlap behavior, alignment,
register clobbers and structure-return conventions. Measure emitted code size as
well as execution: removing calls does not mean removing the required data copy.
The 9.7% sampled leaf share is evidence for a target, not a promised 9.7% speedup.

No language message or logical frame needs to disappear. This optimizes existing
native glue, rather than introducing Rust-stack storage for language execution.
Its benefit is backend-specific; it does not prevent replacing the implementation
with Birnam later. Do not combine it with uninitialized call/root scratch, which
would need a separate lifetime and conservative-root analysis.

**Acceptance:** independent C copy fixtures for supported sizes, alignment and
aliasing, ABI/foreign-call tests, full native codegen/runtime gates, paired Logue
and language probes, identical terminal bytes and game state.

### 4. Compact physical storage for tiny behavior dictionaries

**Module:** `birnam-id/src/id/vtable.rs`; more exploratory than the first three.

Block creation still creates a distinct delegated behavior and then its Block.
A dictionary's first binding reserves three separate backing arrays: keys,
Operand values and weak-key flags, each with initial logical capacity four.
Profiles show considerable binding/allocation work, and the inline conditional
probe creates two delegated behaviors per invocation.

Investigate a single owned backing allocation or stable inline small storage,
keeping the raw dictionary view, logical capacity, ordered keys, weak semantics,
mutation and each behavior's separate identity. Small dictionaries already use
linear search below the threshold; avoiding a HashMap is not a new opportunity.

Sharing behavior objects between closures would change reflected identity and
mutation, so that is outside this proposal. The remaining storage change is
semantically plausible but less simple and needs allocation evidence before
implementation is justified.

**Acceptance:** independent raw-view/iteration/weak-key tests, growth and mutation,
reflection, collection/image/lifetime checks; then measure block construction and
real Logue, including rejected/no-benefit results.

## Other leads and boundaries

- Private address-key maps in Execution, Collect and ObjectModel still use the
  general hasher. Id already has a specialized `AddressMap`. Applying that idea
  selectively may help, but first distinguish pointer-key hashing from the
  constant-name hashing above and audit observable iteration order. Do not change
  source-level Map hashing or string-key collision protections on this evidence.
- The numeric hotness path can ask the live Integer evaluator about warmed
  one-argument calls that it declines. Restricting those queries might remove a
  malleable hook. This profile does not establish their cost, and the short
  dispatch census does not establish warmed behavior. Defer this candidate.
- Collector whole-workspace reuse was already measured and rejected. The current
  samples support reducing per-object helper/lookup cost, not retrying that
  allocation-only idea without new evidence.
- Do not remove roots, fresh identities, dynamic getters, or source message sends
  merely because their counts are high. The language intentionally exposes those
  contracts. The first three proposals target implementation work around them.

The next useful slice is proposal 1, then the small metadata change in proposal
2, followed by the more substantial compiler work. Keep each experiment separate;
retain it only after correctness checks and quiet paired measurements.
