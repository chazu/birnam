# 74D — E4: elide the slot-index box on the guarded hit path

> **Status: written, reviewed, implemented and measured 2026-09-09 under the
> user's authorization to run the experiment (Design 74C §3.5's hypothesis).
> Result (§7): the continuation test is met at every size; the elided
> emission halves the fixture's warm time against the baseline and is 34–43%
> faster than E1 at the judged sizes, with every access hitting and nothing
> regressing. Development-only: it extends the `dev-guarded-slots`
> feature ([74B](74b-e1-guarded-slot-access-implementation.md)) and changes
> nothing in an ordinary build, an admitted artifact, the frozen IdApi, or
> the runtime ABI hash. Continuation test (Design 74 §7.5 as amended): E2
> identical, nothing regresses, gain above run-to-run spread, at any size.
> Two adversarial reviews (semantics and rooting; measurement validity) were
> folded in before the recorded measurement; §6 lists what they changed.**

## 1. Hypothesis

E3 measured the guarded path at 13–20% faster than the baseline with every
guard hitting. Its counters are process totals over the fixture's twelve loop
runs (cold and warm, both bodies), which 74C §3.3 divided by a single loop's
iteration count; the corrected figure is **124.5 guarded slot operations per
loop iteration** at every size (for example (24,478,438 − 742) / (12 × 16,384)).
Each of those still allocates a heap Integer (`MKV(slot)` → `birnam_mkSmallInt`
→ `make_integer`) for a compile-time constant that the floor decodes back to a
`usize` on the next instruction and never looks at again.

The box is a 16-byte object (8-byte header, 8-byte payload) plus its entry in
the heap's address map, so an iteration allocates about 2 KB of slot boxes and
the 16 MiB automatic-collection threshold is crossed about twice in a
16,384-iteration run and about once in the whole process at 1,024. Avoided
collections are therefore a minor part of the hypothesis; the cost being
removed is the allocation itself (zeroed allocation, address-map insert,
header write) and the sweep work those objects add to the collections that do
occur. On this fixture slot boxes are roughly 97% of all heap allocation by
bytes (the loop's only other allocations are about three Integer results per
iteration), which makes it a best case for the elision, not a representative
program; §4 states the result with that bound.

## 2. Legality

### 2.1 The box is unobservable on the hit path

Under 74A's guard conditions the hit path calls `stackless_oop_at` /
`stackless_oop_at_put` directly. The only consumer of the box is
`nonnegative_index`, which reads its Integer payload. The box is never stored
in a Frame slot, never passed to Birnam code, never interned, never returned.
Nothing in the language can name it. Therefore a hit path that computes the
same `usize` from a C `long` answers exactly what the boxed hit path answers,
performs the same store, and takes the same nil branches (a negative constant
cannot arise: `emit_get` and `emit_store_prefix` emit `u32` slot numbers; a
negative `long` reaching the floor is treated as a negative Integer is, nil).

### 2.2 The miss path is the baseline's send with the baseline's operands

On a miss, the box must exist because `oopAt:` is a real send whose argument
is an Integer. E4 creates it **inside the floor**, immediately before the
ordinary send, from the same constant: `make_integer(index)` then
`send_abi(machine, oopAt:, [box], 1)`. Generated code's `birnam_send` reaches
`IdApi.send` → `public_api::api_send` → `enter(|| send_abi(..))` on the
process-global Runtime; the dev entry runs on the thread-active Runtime and
calls `send_abi` directly. Those are the same object under the floor's
single-Runtime invariant (`Runtime::enter` aborts if two Runtimes are entered
on one thread), which is the only configuration generated code runs in, and
the re-entrant `enter` is a TLS install of the pointer already installed. The
selector is the execution objects' `oop_at` / `oop_at_put` Symbol (a strong
root), the same object `birnam_intern("oopAt:")` answers, so the send's
receiver, selector, argument values and count are what the baseline's send
sees. The 74A §4 fallback argument (no re-evaluation, one lookup, one effect)
is unchanged. The exported C entry points accept only an integer index, a
narrower contract than `oopAt:`'s (a Float or Word operand cannot be
expressed); the emitters only ever pass `u32` slot numbers.

### 2.3 What moves: allocation timing, and only that

The differences from E1, all on the boxed operand only:

| | E1 (74B) | E4 |
| --- | --- | --- |
| Read, hit | box, guard, direct read | guard, direct read |
| Read, miss | box, guard, send | guard, box, send |
| Store, hit | intern, box, value, guard, direct write | value, guard, direct write |
| Store, miss | intern, box, value, guard, send | value, guard, root value, box, root box, send |

Two consequences need stating:

- **Collection timing changes, and that is Birnam-visible.** Fewer
  allocations means automatic collections happen at different points, and
  two `System` selectors expose the difference: `liveObjects` answers the
  heap's allocation count and `gc` answers the number of objects a forced
  collection reclaimed. A program printing either inside a compiled loop
  prints different numbers under E4. Pending code-generation retirements are
  also drained at collection, so a queued retirement completes at a
  different iteration. The legality argument is therefore not "nothing can
  see it" but Design 60F §9 and Design 49: programs may not depend on
  allocation addresses, collection points, or retirement latency, and those
  counts are diagnostics. This is the same class of difference as the
  dispatch counters Design 74 §7.4 permits, one step more visible. A
  precondition of the E2 byte-identical requirement is that no compared
  fixture prints those counts; `gc-nested` calls `System gc` and discards its
  answer. Native finalizers still run only inside a collection with
  allocation forbidden, and any dictionary mutation they make still bumps
  the epoch before the next guard (74A 2.9).
- **Selector interning moves off the hit path.** E1 interned `oopAt:put:`
  before every store; E4 uses the execution objects' Symbol on a miss and
  nothing on a hit. 74A §6.2 established hoisting as legal (Design 60F §5:
  interning may be prepared earlier when it has no Birnam-visible effect, and
  `oopAt:` / `oopAt:put:` always exist). E1 kept it deliberately to isolate
  dispatch savings; E4 folds it in because it is on the same path and the
  isolation question is answered. The block terminators' `continueAt:` /
  `callSite:` publications still intern and box one program counter per
  executed basic block; E4 does not touch them.

### 2.4 The store path's rooting obligation

This is the one place a naive version is wrong. Design 60F §9's ingress
exception lets a producer hand a fresh Oop to its storage send through a
generated C local only when nothing between them can allocate. That is why
`emit_store_prefix` allocates the box **before** the value expression today.
E4 removes that pre-allocation, so on a **miss** the box would be allocated
after the value was produced and while it is still only a C local.

E4 therefore never allocates in generated code after the value. The store
entry point takes the value into the floor, and on a miss the floor pushes it
onto the Runtime's scoped host-root stack (the LIFO root stack the driver
already traces via `trace_scoped_host_roots`) through a `Drop` guard, then
boxes the index, roots the box the same way, performs the send, and the
guards pop in LIFO order even if the send unwinds. On a hit the direct write
is non-allocating and establishes the descriptor-traced Frame root exactly as
`machine_oop_at_put` does. Conservative native-stack scanning (the default
collector on this host) would also find the C local, and it is what protects
the baseline's own `MKV` result across `birnam_send` today; the explicit
roots are the argument for the new window, not the scan.

The read path has no such obligation: the box is fresh and goes straight
into the send's argument buffer, exactly as the baseline's `MKV` does.

### 2.5 Everything else is E1's argument unchanged

Guard predicate, epoch, per-access re-verification, lifetime rules (no
cached method address, no lease, nothing serialized), fallback boundary
(exactly one send with unchanged operands), nil mapping of every error, and
the containment argument (feature-gated exports, runtime ABI hash untouched,
dev artifacts cannot load on an ordinary runtime) are all 74A/74B's and are
not re-derived here. The 74A §2 rows 2.1–2.17 map identically; the only row
whose *mechanism text* changes is 2.9 (GC between accesses): the allocation
that could trigger a collection now happens on the miss path only, and the
guard was evaluated before it, so the ordering "guard, then the access it
covers" still holds on the hit path, and the miss path's send performs its
own lookup after the allocation.

## 3. What is built

| Piece | Where | Behavior |
| --- | --- | --- |
| Floor entry points | `dev_slot_access.rs`, feature only | `birnam_dev_frame_slot_get_v1(machine, long index) -> Oop` and `birnam_dev_frame_slot_put_v1(machine, long index, value) -> Oop`. They always answer: the direct result on a hit, the ordinary send's result on a miss (performed inside the floor as §2.2/§2.4). They require the active Runtime the driver always provides (E1's entries degraded to a miss without one; E4 has no generated-code fallback to degrade to). A negative `index` is treated as the boxed path treats a negative Integer (nil on a hit; on a miss it is boxed as-is and sent). Counters share E1's attempts/hits with one counting check per access, plus `unboxed` (calls through these entry points) and `boxes` (miss-path boxes created). |
| Runtime | `runtime.rs` | `guarded_frame_slot_at(machine, index: i64)` / `_put(machine, index, value)` answering `Option<Oop>` as the E1 functions do, sharing `guarded_slot_access_eligible`; `fallback_frame_oop_at(machine, index)` / `_put` performing the rooted box-and-send. |
| TCC shims | `birnam-tcc/src/lib.rs`, feature only | Forwarders through `floor_symbol_address` for the two new names; symbol table and validation table entries under the feature. |
| Emission | `resumable_c.rs` | `slot_box_elision_selected()`: feature, `BIRNAM_DEV_GUARDED_SLOTS=1` **and** `BIRNAM_DEV_ELIDE_SLOT_BOX=1` in the compiling process. `emit_get` → `birnam_dev_frame_slot_get_v1(machine, NL)`; `emit_store_prefix` emits only `void* __birnam_value_N=`; `emit_store_suffix` emits `(void)birnam_dev_frame_slot_put_v1(machine, NL, __birnam_value_N);` (it gains the slot parameter). E1's emission is unchanged when only the first variable is set, so one dev prefix serves both candidates. |
| Drivers | `scripts/e2-slot-access-matrix.py`, `scripts/e3-slot-access.py` | Accept extra candidate environment so the same fixtures run in the elided mode; E3 gains a fourth configuration so one run yields baseline, E1 candidate and E4 candidate side by side. |

Unit test: the E1 guard probe (`guarded_slot_access_agrees_with_the_canonical_send_and_misses_on_overrides`)
gains rows for the unboxed entry points: hits agree with the send for live,
raw, out-of-range and negative indexes; after redefining `oopAt:` with a
counting method, the unboxed read misses and answers what the send answers
and the count moves by exactly one; the store miss path stores through the
send and its value survives a forced collection between the miss and the
send (the rooting obligation, tested by forcing `gc_collect` inside the
fallback under a test hook).

## 4. Measurement

The 74C §2 protocol verbatim (independent prefixes from one commit,
alternating fresh-process pairs after `bench-host-check`, cold/warm phases,
retained samples, counters from separate instrumented runs, peak RSS,
generated-code inspection, the no-auto-GC control probe), with these
additions and stated limits:

- **Four configurations per pair:** ordinary baseline prefix; dev prefix
  with no variable (control); dev prefix with `BIRNAM_DEV_GUARDED_SLOTS=1`
  (E1 candidate); dev prefix with both variables (E4 candidate). The E1
  column measures the whole E4 emission against the whole E1 emission: box
  elision, the per-store `oopAt:put:` intern moving off the hit path (74A
  §6.2), and a direct call in place of E1's `static inline` C helper. It does
  not attribute the gain among those three; all three ship together in any
  production slice, so the question asked is whether the emission as a
  whole is a free win.
- **Activation is proven, not inferred.** "Boxes created = 0" alone would
  also describe an E1 build, so the record must show for the E4 candidate:
  `unboxed` equal to `attempts`, `attempts` equal to the E1 candidate's at the
  same size with every attempt a hit and `boxes` zero, and the inspected E4
  unit importing exactly the two `slot` symbols and neither `oop_at` symbol.
  Inspection also counts `birnam_mkSmallInt` call sites in the E4 and
  baseline units; the difference is the slot-access sites removed, the
  remainder are source-level literals.
- **Order balance.** With four configurations and five pairs the alternation
  leaves the E4 candidate running on average later in each quad than the E1
  candidate, so any within-quad drift is charged against E4. The imbalance
  can only understate its gain.
- **Peak RSS** is one unmeasured run per configuration and is reported as a
  diagnostic only; E3's record shows a 27 MB spread between configurations
  at a size where nothing differs.
- **E2 matrix** re-run in the E4 mode, byte-identical stdout required. Every
  non-fatal E2 case hits on every access (redefining `step:` moves the
  epoch but leaves the Machine vtable canonical), and the two cases that do
  miss abort in both modes before counting, so the matrix exercises the
  hit path only. No Birnam-level construct can produce a non-fatal miss
  today (an `oopAt:put:` override on `Machine` recurses through its own
  frame access and aborts, tracked as `birnam-kh3`), so the store miss path
  and its rooting (§2.4) are covered by the unit test alone, which forces a
  collection in precise mode inside the fallback after the value is rooted
  and before the box is allocated, and checks the rooted value arrives at a
  replacement `oopAt:put:` intact. Automatic collection exists only on
  macOS/aarch64 (the native root range is a stub elsewhere), so the host is
  pinned to `Mac15,12` as in E3.

**Continuation test.** All E2 rows identical; no reported case regresses
beyond its spread; the E4 candidate's warm median improves on the E1
candidate's by more than the larger of their spreads on the 1,024 and 16,384
cases. No percentage bar. If the gain is real, the production slot-access
slice proposal (Design 74 §7.6, already due from E3) includes the elision;
if it is not, the proposal proceeds without it.

## 5. Cost and risk

About 150 lines of Rust, 30 of C prelude and emitter, driver flags, and one
test extension. The risks are the store-path rooting (§2.4, tested
directly) and a mistake in the "always answers" contract of the new entry
points (a miss that forgot to send would silently drop a store; the E2
override cases and the unit test's counting method catch that). The
experiment stays behind the feature and the second environment variable, so
nothing reaches an ordinary build.

## 6. What the adversarial reviews changed

Two independent reviews ran against the first draft and the first
implementation before the recorded measurement. Corrections taken:

- **Arithmetic.** The per-iteration slot-operation count was 12× too high
  (a process total divided by one loop's size) and the box size 2× too
  high; §1 now carries 124.5 operations and 16 bytes, and the collection
  argument was demoted accordingly. 74C §3.3–3.5 carry an erratum.
- **Visibility.** `System liveObjects` and `System gc` expose allocation
  and reclaim counts, and pending retirements drain at collection, so §2.3
  now argues from 60F §9's prohibition rather than from invisibility, and
  the E2 byte-identical requirement states its precondition.
- **Fallback selector.** The first implementation re-interned the selector
  on every miss and held it only in a local across the box allocation; it
  now uses the execution objects' Symbol, a strong root.
- **Unwind safety.** The scoped roots are held through `Drop` guards, as
  every other user of that stack does; the index box is rooted across the
  send as well as the value.
- **Attribution.** The E1 column does not isolate the box alone (the store
  intern and E1's `static inline` C wrapper also go); §4 says what the
  comparison measures instead of claiming isolation.
- **Activation proof.** "Boxes = 0" was tautological; the record must show
  `unboxed = attempts` and the unit's imports.
- **Coverage honesty.** No E2 case reaches a store miss, and none can from
  Birnam today; §4 says the rooting obligation rests on the unit test, which
  now forces its collection in precise mode.
- **Instrumentation cost.** The first implementation checked the counting
  flag twice per access on the E4 path and once on E1's; both now check
  once.
- **Host and residue.** Automatic collection exists only on macOS/aarch64;
  block-terminator publications still box a program counter per block.

Findings recorded but not acted on for this experiment: the fixture prints
only the median of its five warm repetitions; order balance across four
configurations with five pairs favors E1 slightly; peak RSS is a single
sample; the empty-body case has no checksum-style validity check; the two
bodies share one process's collection budget.

## 7. Measurement record

Taken 2026-09-09 on `Mac15,12` (AC power, `bench-host-check` preflight
before every quad), both prefixes built from the tree committed as
`07e6a181` (the receipts record the parent commit with a dirty tree because
the prefixes were built minutes before that commit; both carry the same
source manifest digest), rustc 1.95.0, release profile. Five alternating
fresh-process quads per size: ordinary baseline, E1 candidate
(`BIRNAM_DEV_GUARDED_SLOTS=1`), E4 candidate (both variables), control (dev
prefix, no variable). Every run's receiver answer and checksum matched.
Everything is in [`74d-e4-record.json`](../../measurements/74d-e4-record.json).

### 7.1 Warm medians (microseconds for the whole loop; spread = max − min of five)

| n | body | baseline | E1 | E4 | control | E4 vs baseline | E4 vs E1 | E1−E4 exceeds larger spread |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 0 | empty | 15 (1) | 13 (2) | 9 (3) | 15 | −40.0% | −30.8% | yes (reported only) |
| 0 | step | 20 (1) | 19 (4) | 14 (2) | 21 | −30.0% | −26.3% | yes (reported only) |
| 1 | empty | 38 (2) | 32 (3) | 22 (2) | 41 | −42.1% | −31.2% | yes (reported only) |
| 1 | step | 52 (6) | 43 (4) | 31 (2) | 55 | −40.4% | −27.9% | yes (reported only) |
| 32 | empty | 775 (126) | 607 (55) | 403 (21) | 770 | −48.0% | −33.6% | yes (reported only) |
| 32 | step | 1,009 (205) | 778 (68) | 532 (26) | 984 | −47.3% | −31.6% | yes (reported only) |
| 1,024 | empty | 25,298 (895) | 20,878 (1,370) | 12,645 (1,729) | 24,800 | −50.0% | **−39.4%** | **yes** |
| 1,024 | step | 32,030 (2,419) | 26,276 (1,446) | 17,255 (2,059) | 32,099 | −46.1% | **−34.3%** | **yes** |
| 16,384 | empty | 488,634 (45,777) | 428,253 (68,424) | 246,106 (37,473) | 488,782 | −49.6% | **−42.5%** | **yes** |
| 16,384 | step | 682,552 (36,833) | 592,847 (52,328) | 337,973 (52,184) | 691,994 | −50.5% | **−43.0%** | **yes** |

Per iteration at 16,384 with the `step:` body: baseline 41.7 µs, E1 36.2 µs,
E4 20.6 µs. The E1 column reproduces E3's figures within spread (E3: 433,231
and 602,785 at 16,384), and the control matches the baseline within spread at
every size, so the dev prefix's presence still costs nothing.

### 7.2 Verdict against the continuation test

**Met.** Every E2 row is identical in the E4 mode (§4, rerun on the rebuilt
prefix in both modes); no reported case regressed; and at both judged sizes
and both bodies the E4 candidate beats the E1 candidate by more than the
larger of the two spreads, by a factor of two to four. The reported sizes
show the same direction. Under Design 74 §7.5 as amended and §7.6, the
production slot-access slice proposal that E3 already earned now includes
the elision.

### 7.3 Activation and inspection

For every size the E4 candidate's instrumented run reports `unboxed` equal
to `attempts` (742 / 2,236 / 48,550 / 1,530,598 / 24,478,438), every attempt
a hit, and `boxes = 0`; the E1 candidate's run at the same size reports the
same attempts with `unboxed = 0`. The inspected E4 unit imports exactly
`birnam_dev_frame_slot_get_v1` and `birnam_dev_frame_slot_put_v1` (neither
`oop_at` symbol) plus `birnam_send`, and its text has 82 slot-read and 105
slot-write call sites and 35 `birnam_mkSmallInt` call sites, against 222 in
the baseline unit (each slot access boxed inline) and 141 in the E1 unit (the
reads' boxing inside the un-inlined C helper). The 35 remaining are
source-level literals and the block-terminator program-counter boxes §2.3
notes.

### 7.4 Diagnostics

Counting overhead is not measurable (E1 candidate, `step:` body: counting
25.7–26.2 ms against 27.1–28.1 ms plain at 1,024; 572–645 ms against 572–601
ms at 16,384). Peak RSS is one sample per configuration: at 1,024 the E4
process stays at 146 MB where the others reach 224–254 MB, consistent with
never crossing the 16 MiB automatic-collection threshold during the process;
at 16,384 all four sit at 271–283 MB. The retained no-auto-GC control probe
(production AOT unit, baseline emission in both prefixes) measured 4.62–5.33 s
baseline and 4.49–4.70 s dev prefix. Objective-C references on the same host
(five runs, 16,384 iterations): message version 27 / 99 µs (empty / step),
boxed variant 27 / 334 µs, primitive C loop 5 / 14 µs, so the E4 candidate's
`step:` loop is about 3,400× the unboxed message version, down from 6,300×
for E1.

### 7.5 What this says and does not say

The fixture is the best case §1 describes: slot boxes were about 97% of its
heap allocation, so halving its time is the ceiling for this change, not a
forecast for programs that allocate for their own reasons. What carries
over to any resumable code is the per-access saving itself, about 126 ns per
slot operation on this host (15.6 µs over 124.5 operations), which every
StepMethod pays on every slot read and write today. The remaining per
iteration cost (20.6 µs for roughly ten sends, 124 slot operations, action
publications and driver transitions) is now dominated by the driver and
dispatch machinery again, not by allocation; the block-terminator boxes
(one per executed basic block) are the obvious next allocation to look at,
and are the same kind of change as this one.

