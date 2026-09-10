# 74E — Production slot-access slice: guarded, unboxed Frame slot access

> **Status: proposed, reviewed, implemented, verified and landed on `main`
> 2026-09-09, then REVERTED the same day by operator decision (commit
> `b3a839c4` reverted). The operator had authorized continuing Design 74 but
> not a runtime ABI change; this slice adds two runtime shim names
> (`birnam_frame_slot_get_v1` / `birnam_frame_slot_put_v1`) to the ABI
> identity, which rejects images saved by earlier runtimes. The record is
> kept as the worked design, with its adversarial review folded in (§7), for
> whenever a runtime ABI cut is wanted. Main is back at the 74D state: the
> guarded unboxed access exists only behind the development feature.**

## 1. What ships

Every generated StepMethod reads and writes its Frame slots through two floor
entry points that take the slot index as a machine integer:

```c
void* birnam_frame_slot_get_v1(void* machine, long index);
void* birnam_frame_slot_put_v1(void* machine, long index, void* value);
```

Each answers directly from the current Frame when the Design 74A guard holds
(the receiver's vtable is the canonical Machine vtable; that vtable's direct
`oopAt:` / `oopAt:put:` entries are the floor Methods; its metavtable is
`vtable_vt`; `vtable_vt` is its own metavtable with the floor `lookup`), and
otherwise performs the ordinary late-bound send inside the floor with exactly
the baseline's operands (boxed index, same selector object, same value,
same count), once. Conditions 2–4 are cached against the vtable
dictionary-mutation epoch; condition 1 is read on every access.

Semantics are the 74A proof's: a Birnam-level override of `oopAt:` or
`oopAt:put:` on the Machine, a private vtable on a Machine, a custom `lookup`
on `vtable_vt`, or a non-canonical metavtable all take the send, so Design
60B §1's metacircular visibility of Machine microcode is unchanged. What
changes is that the fast path allocates nothing and dispatches nothing.

Measured on the Design 74 fixture (74D §7): about half the baseline loop time
at 1,024 and 16,384 iterations, about 126 ns saved per slot operation, every
access hitting, the E2 semantic matrix identical.

## 2. What is removed

- The `dev-guarded-slots` cargo feature on `birnam-id`, `birnam-tcc` and
  `birnam-codegen`; `BIRNAM_DEV_GUARDED_SLOTS`, `BIRNAM_DEV_ELIDE_SLOT_BOX`,
  `BIRNAM_DEV_GUARDED_SLOTS_COUNT`; `dev_slot_access.rs`; the E1 boxed entry
  points and the E1 `static inline` C helpers; the process-global emission
  selectors in `resumable_c.rs`; `BIRNAM_BUILD_CARGO_FEATURES` stays (it is
  general build plumbing).
- The baseline emission `birnam_send(machine, birnam_intern("oopAt:"),
  (void*[]){MKV(n)}, 1L)` and the store prefix's selector intern and box.
  There is no fallback emission; the floor entry points are the emission.
- The counters. Nothing in production counts guard hits.

The 74B/74D fixtures and drivers lose their baseline/candidate switch. The
E2 matrix fixture is now `bench/slot-access-matrix.birnam`, driven by
`scripts/slot-access-matrix-smoke.{sh,py}`, which checks each case's printed
result against the recorded expectations (the two fatal override cases stay
out, tracked as `birnam-kh3`); the E1 fixture (`bench/e1-slot-access.birnam`)
stays as a retained probe like `bench/control-throughput-probe.birnam`; the
E1 and E3 drivers are retired with their records.

## 3. Obligations

### 3.1 Runtime ABI (exact-current cut)

`RUNTIME_SHIM_NAMES` in `birnam-image` gains the two names and
`RUNTIME_ABI_VERSION` goes from 4 to 5; the identity hash becomes
`26fd5ae57990a0f3`. Every image-v8 header carries that hash, so every
checkpoint saved by an earlier runtime is rejected before load, as 60H §2
requires. The cut is made verifiable rather than asserted: a `birnam-image`
test pins the hash literal so a mis-threaded bump fails closed; the host
names the failure (`RuntimeAbiMismatch { expected, observed }`) instead of
folding it into a corrupt-header error, satisfying 60H §3's diagnostic
shape; and `scripts/exact-current-rejection-smoke.sh` now mutates the ABI
field at offset 20 as well as the format version at offset 8.

Compiled artifacts do **not** carry the runtime ABI hash; their manifests
record the artifact generation contract (`birnam-artifact-generation-v1`).
Artifacts built by an earlier release therefore still load and run their
old emission, harmlessly, because the canonical `oopAt:` / `oopAt:put:`
Methods remain on the Machine vtable. Every installed module is rebuilt by
`birnam-build` (§3.6), so a release prefix carries only the new emission by
construction, not by admission. Any future change that would break the old
emission (removing the canonical Methods, changing the Frame descriptor)
needs an artifact-side admission change of its own.

No adapter, no dual emission, no reader for the old hash. The next release
establishes its own checkpoint contract; rollback means selecting the
previous published release with its own checkpoints.

### 3.2 Exports and audits

The floor exports the two entry points; the TCC rung exports the same two
names, forwarding to the floor through the existing private-symbol discovery
(`floor_symbol_address`), **resolved once at rung bring-up** so a floor and
rung that disagree fail at initialization with a message naming the symbol
and the ABI, not at the first slot access. The rung lists them in its
generated-code symbol table and validation table, and
`scripts/rust-tcc-dylib-audit.sh`'s expected export list gains them. The
audit now runs inside `scripts/rust-driver-native-smoke.sh`, which the
cutover verify gate runs, so the export set is checked on every verification
instead of by hand. The names are persisted by spelling in the runtime ABI
identity; they are not IdApi struct fields, and the IdApi struct is
untouched.

### 3.3 Guard invalidation on image restore

Not needed, and recorded as such. Image restore reaches the floor only
through the IdApi: objects are freshly allocated, vtables arrive as exported
references or as portable block vtables, and method installation goes
through `add_method` and `ensurePrivateVtable`, both of which move the
epoch. No IdApi entry can re-point a live object's vtable header. The cached
conditions read two header words behind the vtable-registry epoch; that is
sound while `Runtime::ensure_private_vtable` is the sole production path
that re-points a live header, and the guard code now carries that invariant
as a comment on `slot_guard_conditions_hold`. A saturated epoch always
re-verifies, matching 74A §3.2.

### 3.4 Design 60F rooting contract

60F §7.1's ingress exception is restated for the new form: a store's value
travels from the producing expression into `birnam_frame_slot_put_v1` with no
allocation, send, helper or driver return between them, exactly as it
traveled into `birnam_send` before. On a hit the direct write establishes the
descriptor-traced Frame root; on a miss the floor roots the value and the
fresh index box on the scoped host-root stack through `Drop` guards before
allocating or sending (74D §2.4); the read path roots its box the same way.
`IdApi.send` does not itself root its argument buffer as 60F §7.1 says; the
old emission's boxed index was protected by the conservative native-stack
scan alone. The new miss path is therefore rooted more strongly than the
baseline was, and 60F §7.1 now says so. The emitter still produces the value
into a named local and passes it in the next statement; the verifier's
producer-to-ingress rule is unchanged.

### 3.5 Collection timing is Birnam-visible and unspecified

`System liveObjects` and `System gc` answer different numbers under the new
emission, and pending code-generation retirements drain at collection. 60F
§7.1's safepoint list and Design 49 already make those unspecified; 74D §2.3 records the
argument. No test in the tree asserts an exact live-object count across a
compiled loop (the module suites assert one-sided bounds); the slice adds
none.

### 3.6 Bootstrap and artifacts

`birnam-build`'s stage1 compiles every installed module with the new
emission; nothing is hand-maintained. Portable CodeObjects are unaffected
(they are lowered at load). The `abi/fixtures/portable-code-v1` image
fixtures embed the runtime ABI hash literally and were regenerated through
the generator into a fresh directory, re-summed, and re-verified; the
`birnam-image` v8 golden digests that byte-compare against them were updated
in the same change. The `stackless-s2` fixtures carry their own frozen v3
constants and are unaffected. The install receipt's `runtime_abi_hash` is
derived from `runtime_abi_hash()` rather than hardcoded.

### 3.7 Tests and gates

- The 74B/74D unit test is the ordinary guard test: agreement with the send
  on hits for live, raw, out-of-range and negative indexes; every miss
  reason; the miss fallback performing exactly one send; the store miss
  surviving a forced precise collection with the value intact.
- Codegen tests that pinned the old emission text are rewritten for the new
  form; `birnam-image` pins the ABI hash.
- The E2 matrix is `scripts/slot-access-matrix-smoke.sh` (eight non-fatal
  cases against recorded outputs), run by the native smoke inside the
  cutover verify gate together with the TCC export audit.
- `scripts/rust-cutover-verify.sh` and the 60H release acceptance gates
  (archive integrity, install layout, exact-current rejection now including
  the ABI field, image resave plateau, stackless image, portable resumable,
  portable code binding, portable image v8, stackless control, bounded churn,
  host-gated benchmark, baseline contract, performance compare, longevity)
  run unchanged on the release candidate.
- Performance: the Design 46 comparator against the tracked v0.14.0 snapshot
  will report large improvements on send-heavy cases and may report more
  near-zero nets as ill-conditioned. The 60HC rule judges micro flags by raw
  medians; a stdlib or macro case whose net inverts because its subtracted
  loop got faster would fail closed and need a 60HC decision at that time.
  No benchmark name, body or threshold changes.

### 3.8 Documentation

60F §7.1 gains an amendment for the slot-access form and its rooting;
60B §1 gains one sentence that Machine microcode sends are guarded, not
removed; 74 §9 records the slice; 74A–74D are left as the experiment record;
CHANGELOG records the runtime ABI bump under Changed.

## 4. Sequencing

Land on `main` after the full ordinary verification (workspace tests, module
suites on a fresh prefix, cutover verify, TCC and floor dylib audits, stdlib
boot timing), then run the deferred v0.15.0 release with it. One release then
carries one exact-current cut; releasing v0.15.0 first and this slice in
v0.16.0 would cut the ABI twice in a row for no benefit. The release's
performance comparison against v0.14.0 is the production measurement of this
change; the 74D record is the experiment's.

## 5. Risks

- **Missed epoch site.** Any mutation of dispatch-relevant vtable state
  that does not move the epoch makes the cache stale. Every dictionary and
  parent mutator in `vtable.rs` moves it (74B's test enumerates those; the
  owner, layout and trace-hook setters do not, and the guard does not read
  them). The only production path that re-points a live object's header is
  `ensure_private_vtable`, which moves it (§3.3). A missed site would make a
  guard hit where the send would have dispatched to an override, which is
  the one failure this design must not have; the guard test's override rows
  are the check.
- **Hand-written C against the runtime prelude.** Anything that emitted or
  expected the old `oopAt:` form breaks at compile time, not at run time,
  because the old symbols and helpers are gone; the codegen tests find it.
- **Fixture churn.** Regenerating ABI fixtures is mechanical but touches many
  binary files; the verifier and SHA256SUMS keep it honest.
- **Linux.** Nothing here is target-specific; the guard reads heap headers
  and dictionary entries the same way everywhere. Automatic collection's
  Darwin-only native root range (74D §4) is an existing limitation, not a new
  one.

## 6. Size

Roughly: 200 lines removed (dev scaffolding, E1 paths, counters), 60 changed
in the emitter and floor, 40 in the TCC rung, one restore-site line, test
rewrites, fixture regeneration, four short doc amendments. One working day
including the verification runs.

## 7. What the adversarial review changed

One review ran against the proposal and the 74D code before implementation.
The legality half survived unchanged: epoch coverage, restore behavior,
rooting, store shapes, the single-Runtime argument, and the completeness of
the emission removal were all confirmed. The release-obligation half did not
survive as written, and the implementation follows the corrected record:

- The rejection smoke tested only the format version; it now tests the ABI
  field, the host names the mismatch, and a test pins the hash (§3.1).
- Artifacts do not carry the runtime ABI hash; §3.1 says what actually
  rejects and what is rebuilt by policy.
- The restore-site epoch bump had no cross-crate mechanism and, on tracing
  the restore path, no need; §3.3 records the argument instead.
- The receipt's ABI hash was a hardcoded string no gate checked; it is
  derived now (§3.6).
- The image fixtures and the v8 goldens both embed the hash; both were
  regenerated (§3.6).
- The TCC rung resolved the floor symbols lazily and aborted at first
  access; it resolves at bring-up and names the failure (§3.2).
- The export audit was run by nothing; it and the matrix smoke run inside
  the native smoke that the cutover gate runs (§3.2, §3.7).
- 60F section references were wrong (§7.1, not §9), 60F §7.1 described a
  rooting rule `IdApi.send` does not implement, and the read-path box was not
  rooted; all three corrected (§3.4).
- Epoch saturation froze the cache instead of forcing re-verification;
  fixed to match 74A §3.2 (§3.3).
- `private.rs` does not define the "private carrier" notion cited; the
  citation is gone (§3.2).

