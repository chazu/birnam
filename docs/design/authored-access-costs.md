# Authored access: runtime bookkeeping costs

2026-10-14. Follow-up to [owned Globals](owned-globals.md), based on `760f44f6`.
The baseline already uses authored Globals. This pass keeps that ownership model
and application code fixed, and reduces general runtime bookkeeping.

## Findings and retained changes

A five-second sample of a warmed binding-read loop put ordinary Id dispatch,
Symbol inventory queries, admission checks, root registration, and thread-handle
access among the largest named leaf costs. Generated code is only partly
symbolized. These samples identify investigation targets; their overlapping call
trees are not additive attribution or prospective speedup estimates.

Three changes are retained:

* **Id thread identity.** Each Id message previously called
  `std::thread::current().id()`, acquiring and dropping a reference-counted Thread
  handle to read an immutable value. A private thread-local now retains that
  ThreadId value. Message and module-initialization entries still compare it against their
  owner's identity every time. It is neither an OS-thread address that can be
  reused nor a cached permission to enter a particular Id. It contains no Oops,
  method entries, or references to language services.
* **Symbol query positions.** The eleven fixed query names already have cached
  randomized digests. They now also have integer position hints into Id's current
  ordered Symbol inventory. A hit reads the current record and checks both digest
  and complete name bytes. A stale, wrong, or out-of-range hint falls back to the
  existing digest index. Missing answers are never cached. Retirement, index
  shifts, collisions, and re-interning therefore retain their existing meaning.
  The eleven position fields occupy 88 bytes per Id on this 64-bit host, not per
  Symbol or object, and retain no Symbol or executable.
* **Vector input protection.** Native language-operation entries borrow the
  receiver and Object arguments' existing caller protection, as Map and numeric
  entries already do. They still separately root detached tree/tail snapshots,
  intermediate values, and new results. A two-input `at:` avoids two redundant
  retains and two releases. Raw `sizeWord`/`atWord:` entries are unchanged.

These changes introduce no cached method selection, foreign payload access,
special native Globals path, or new monkeypatch checks. Current Id lookup and
existing optimization admission/deoptimization paths remain in force. The Symbol
hint is validated against current owned data; the ThreadId is immutable platform
identity; input borrowing uses the native ABI's established invocation lifetime.
None of these premises freezes an editable language object or dependency Method.

## Experiments and limits

The first read-loop pilot runs two fresh processes per variant in BAAB order,
with three timed repetitions of 2,000 reads per process. Startup, warm-up and
explicit between-repetition collection are outside the thread-CPU timer; normal
automatic GC stays enabled. Every repetition checks the answer, 14,000.

| Experiment | Candidate/baseline median CPU | Decision |
| --- | ---: | --- |
| Hoist the common lookup-policy check within a binding batch | 1.005 | Removed; no useful measured gain |
| Force-inline fixed Symbol query lookup | 0.992 | Removed; difference too small to justify it |
| Vector duplicate-input roots alone | 1.006 | No read-loop CPU gain established; deterministic root traffic reduction |
| Validated Symbol position hints alone | 0.982 | Qualify with application measurements |
| Symbol hints plus Id thread identity | 0.927 | Qualify; strongest read-loop result |

The Vector-only application pilot measured sparse batches at 181.217 ms versus
179.833 ms and generation at 2,869.116 ms versus 2,947.849 ms. This mixed small
result does not establish a standalone application speedup. Its retained benefit
is removing duplicate root registrations under an already-established contract.
Final combined application measurements are kept separate from these pilots.

## Application results

Final Logue runs used BAAB order with two fresh processes per variant/workload,
three samples per process, and normal GC. The replacement scenario retains its
fixed single-sample configuration. The table summarizes the process medians.
All 34-check regressions and every checkpoint/replacement oracle passed.

| Workload | Owned-Globals baseline | Candidate | CPU reduction |
| --- | ---: | ---: | ---: |
| Sparse arena batch | 178.382 ms | 164.017 ms | 8.1% |
| Dungeon generation | 2,862.530 ms | 2,621.822 ms | 8.4% |
| Replacement scenario batch | 96.989 ms | 86.678 ms | 10.6% |

Baseline/candidate process-median ranges were 177.583–179.181/163.982–164.052 ms
for sparse batches, 2,825.295–2,899.766/2,615.409–2,628.236 ms for generation,
and 93.990–99.988/85.495–87.861 ms for replacement. These are observations on this
host, not universal guarantees or confidence intervals.

Timings exclude startup, fixture preparation, snapshots, JSON, and explicit
between-batch collection. No qualification or sampling ran concurrently with final timing.
The timing prefixes have identical applications, authored artifacts, and saved
images; only the Id and Vector dylibs differ. A separate fresh prefix validates
building and bootstrapping the candidate. Both warmed Logue profiles also passed
the independent state oracle; Id's repeated Thread-handle access disappeared
from the candidate's hot-leaf listing. Profile counts are not speedup estimates.

The final binding-read loop also ran in BAAB order, with three repetitions per
process: median CPU fell from 2.070 s to 1.922 s for 2,000 reads, a **7.2%**
reduction. This is the full loop, including arithmetic and traversal, not an
isolated lookup latency. The write microbenchmark was not remeasured in this pass.

## Invariant checks

The Symbol tests exercise every fixed name in fresh inventories, missing names,
arbitrary stale positions, retirement/re-interning, and a wrong-name record with
the same digest. The thread test compares the cached value with Rust's actual ID
across 32 successive worker lifetimes and repeated reads. A native test also
attempts an actual Id message from another thread, verifies rejection, and checks
that the owner thread can still use the carrier.

The Vector regression enters through both synchronous and resumable generated
code. During index conversion its callback drops the native caller's external
leases, replaces the Vector's root and tail, and forces precise GC. Inputs and
the detached snapshot must survive, the read must return the original value,
and the old nodes must become collectible after the invocation. A failing
conversion preserves its native failure. The root counter observes only the
Vector entry, excluding legitimate roots acquired by the resumable caller.

The existing Lean contracts apply directly:

* `SlotHints.validated_hint_equals_current_lookup`: a validated current record
  agrees with current lookup for unique keys.
* `SymbolNames.cached_digest_uses_current_inventory`: the digest narrows a
  current inventory without retaining an old Symbol answer.
* `Invocation.input_root_equivalence` and `needed_survives`: covered inputs may
  borrow caller protection while local snapshots and new values retain theirs.

The full formal gate checks these and the existing TLA+ admission, invocation,
recovery, and lifetime scenarios. These are abstract contract checks, not a proof
of Rust TLS, the Rust compiler, or the entire VM. Native and application tests
check the implementation and produced libraries.

Qualification passed the fresh build, 14 Id unit tests, 24 focused native test
binaries (including Isolate), all 826 language assertions and auxiliary owner
checks, all 26 REPL cases, the Globals Store-absent floor, and new/legacy
source-free images in mapped and rebuilt modes. The formal gate checked 263 Lean
theorems, rejected 60 deliberately broken variants, and obtained all 140 expected
TLC outcomes. This is a focused native selection and broad language/image
qualification, not a rerun of the complete release gate.

The [measurement receipt](../measurements/authored-access-costs-20261014.json)
records source/runtime hashes, profiles, all pilots, final timing runs and
qualification results. Qualification ran against the working tree before commit;
the installed release was not replaced. This pass does not resume the remaining Store refactors or establish
that the complete Globals migration's CPU cost has been recovered.
