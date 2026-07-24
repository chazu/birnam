# Design 61a — R6 performance tradeoff record

Status: accepted, 2026-07-23

This record closes Design 61 Phase R6 and permits Phase R7 to begin. It accepts
two measured performance tradeoffs in one exact, noise-qualified Zig/Rust
snapshot. It does not change Design 46's general 10% micro and 5% other-case
thresholds, retire either case, or authorize the same deltas in a later
comparison.

## Accepted snapshot

The accepted comparison is:

- Zig result:
  `bench/results/v0.11.0-zig-2fe44d2-c8b632e8882358672a0de5a24897a735c09af2f080cb1aea55510e021fc3d6b1.json`
  (file SHA-256
  `237f42efcd8e555d6a3b59aa0a5f570fe0b2e3c594e6c57aff2918edbd062c0c`);
- Rust result:
  `bench/results/v0.11.0-rust-2fe44d2-239d348c297583789c4a4da8d5713ebcfd1b9ba1dbf12c04ecb44e56282ac0ea.json`
  (file SHA-256
  `98e90eb7e5049b0e17fa73838c2b220888ab0285e1427a9fd2b43bafc5cc65ca`).

Both floors use source commit `2fe44d2`, ReleaseSafe optimization, the same
machine, and the tracked host-noise guards. The Rust result's `dirty: true`
stamp is caused by the already-emitted, untracked Zig result from the first
half of the pair; no source, build input, or selected dylib changed between
halves. The exact result hashes above prevent that narrow exception from
becoming a general dirty-tree allowance.

`mise run r6-acceptance` first runs the current native-rung baseline checker
(semantic fixtures, the accepted optimized pair, and the artifact-inspector
baseline), then runs the latest replacement set: the unchanged grammar suite,
frozen CST parity, syntax diagnostics, canonical Rust/explicit Zig rollback
selection, the optimized native-rung harness, and the explicit Rust Codegen
runtime boundary (`test-rust-codegen-runtime`). It then runs the complete
`zig build verify -Doptimize=ReleaseSafe` closure, verifies both file hashes,
and runs the unchanged Design 46 comparator. It fails unless the only
threshold crossings are exactly the two entries below. The optimized closure
is fail-closed except for the exact, separately recorded `run-tests.birnam`
`sortedModules` fixture crash: one failed step, the passing assertion followed
by a `collection.zig:336` `vSortBy` segmentation fault, and the pinned
234/237-step, 434/440-test summary. Any changed snapshot, functional failure,
additional verify failure, missing or unusable case, additional regression, or
changed accepted measurement fails closed.

## Accepted tradeoffs

| Case | Zig | Rust | Measured cost | Decision and user benefit | Follow-up |
|---|---:|---:|---:|---|---|
| `query.aggregate` | 1,475,835 ns/op | 1,562,867 ns/op | +87,032 ns/op, +5.8%; 0.8 points beyond the 5% threshold | Accept. The absolute cost is 0.087 ms per benchmark operation, while the Rust floor supplies the already-proved ownership, provenance, panic, Miri, and cross-floor robustness work and enables one authoritative floor. | Keep the case active. Use this Rust result as the post-cutover comparison baseline, profile aggregate reads after R7, and recover to at most 1,549,626 ns/op (5% over this Zig reference) before the Zig-retirement checkpoint if the work is bounded. |
| `compile.stage1-stdlib` | 39,818 us | 44,564 us | +4,746 us, +11.9%; 6.9 points beyond the 5% threshold | Accept. This is about 4.7 ms on a cold whole-process module compilation, not an interactive send or steady-state runtime cost. The same snapshot improves source boot by 2.7%, image boot by 10.0%, scripts by 7.4–9.7%, and full doc generation by 30.3%. | Keep the case active. Profile process setup and Rust-floor initialization after R7 build integration; target at most 41,808 us (5% over this Zig reference) before the Zig-retirement checkpoint if it does not delay correctness or rollback work. |

## Decision boundary

The raw comparator reports two regressions, zero missing cases, and zero
unusable cases. This record changes their status from unaccepted to accepted
only for the exact snapshot above. All other cases satisfy Design 46, including
the previously blocking `string.split-native` case at +2.4%. Later benchmark
pairs use the normal thresholds without inheriting these exceptions.

R6 is therefore accepted for the Rust replacement and benchmark snapshot. The
exact `sortedModules` fixture crash is quarantined as a follow-up and is not a
Rust-floor or benchmark exception; R7 may integrate and select the Rust floor
while retaining the Zig oracle and rollback path required by Design 61.

## Latest acceptance reruns

After the Rust PEG corpus, diagnostics, generated-C, metadata, stress, and
relocated-install gates were added, `mise run r6-acceptance` was rerun on
2026-07-23. The native-rung baseline checker and the unchanged Design 46
comparison passed: the same two exact result hashes were present, with zero
missing or unusable cases and only the two tradeoffs recorded above. These
later PEG tests therefore add confidence without widening the R6 exception
set or changing the accepted snapshot.

The Rust TCC dynamic-boundary, separate-process artifact activation, full
Codegen, Alien, and image suites were then added and passed on both floors.
The R6 gate now also executes the latest Rust-Lang replacement set before
checking the snapshot. The native-rung harness now propagates the requested
optimization profile into its nested builds, so the gate exercises the same
ReleaseSafe closure used by the acceptance evidence. The latest optimized
rerun passed every focused replacement gate and 434/440 full-closure tests;
the sole failed step is the exact `run-tests.birnam` `sortedModules` fixture
crash described above. The gate accepts that one marker-exact observation,
while the exact accepted hashes remained unchanged and the same two recorded
crossings had zero missing or unusable cases. This additional cross-layer,
native-TCC, and Rust-Lang coverage does not widen the R6 performance exception
set or change the accepted snapshot. Commit `eacfa5f` extends the focused set
with the explicit Rust Codegen runtime boundary; the gate revision records the
fixture failure separately so it cannot mask a new failure.

On 2026-07-24, after the Rust image framing crate entered the installed Cargo
metadata gate, the complete `scripts/r6-acceptance.sh` run passed again from
commit `8ae8a10`. The focused Rust language, parity, diagnostics, selection,
native-rung, and Rust Codegen checks passed; the optimized verify closure and
the unchanged comparator again reported zero missing or unusable cases and
only `query.aggregate` (+5.8%) and `compile.stage1-stdlib` (+11.9%). The
accepted result-file hashes and the exact two-tradeoff boundary are unchanged.

After the artifact dependency-loader gate and its Rust Codegen regression test
landed at `13ec3d3`, the complete acceptance script was rerun at `3d55d58`.
The focused Rust Codegen target passed 17/17 tests, including the new
dependency-rejection case; optimized verify reproduced the exact quarantined
`sortedModules` fixture signature, and the digest-pinned comparator again
reported the same two crossings with zero missing or unusable cases. That
additional test changed only the functional coverage; it did not widen the R6
performance exception or alter the accepted benchmark snapshot.

After the Rust image framing preflight entered the opt-in Codegen boundary at
`f7a1de7`, the R6 pin advanced to 18/18 and the complete acceptance script was
rerun at `73b15c4`. The new valid-authority and bad-magic cases passed; the
optimized closure retained only the exact quarantined fixture signature, and
the digest-pinned comparator again reported exactly the same two crossings with
zero missing or unusable cases. This remains a functional coverage update, not
a new performance exception or benchmark snapshot.

After fact-record validation and the exact Wyhash code checksum check landed at
`49c79e1`, the complete script passed again on current head `a2b331f`. The 18-test
Rust Codegen boundary, pure image tests, optimized verify closure, and
digest-pinned comparator all remained green; the only verify observation was
the same quarantined `sortedModules` fixture, and the comparator retained zero
missing/unusable cases plus the same two accepted crossings.

The checksum implementation was then corrected to use Zig's fourth Wyhash
secret exactly (`...4cc3`) and extended with all seven Zig reference vectors at
`ff67918`. The complete R6 script passed at that head: the 18-test focused
boundary, workspace tests and clippy, optimized verify closure, exact fixture
quarantine, and digest-pinned comparator all remained green. The comparator
still reports zero missing or unusable cases and only the two accepted
crossings (`query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`).

The opt-in mapped-code handoff was then added at `d8a0787`: the 19-test Rust
Codegen boundary now rejects mapping outside image recovery and maps a
checksum-verified captured dylib through the existing Rust artifact loader.
The R6 pin advances to 19/19 functional tests; this adds no benchmark case or
performance exception.

The scalar-fact recovery handoff landed at `89c6283`. The 20-test Rust
Codegen boundary now preflights all scalar fact values before entering Rust
Store image-load mode; heap references remain explicitly rejected until heap
materialization lands. This is functional coverage only, so the R6 benchmark
pair and its two accepted crossings are unchanged.

The complete R6 acceptance script then passed at `89c6283`: 20/20 focused Rust
tests, the known `sortedModules` fixture exception in optimized verification,
and the existing two benchmark crossings (`query.aggregate` and
`compile.stage1-stdlib`).

The follow-up regression at `5faf0b3` exercises a facts record containing a
heap reference and proves that replay fails before the Store observes a
mutation. The complete R6 script passed again at that checkpoint with the
same 20/20 target and the same two accepted crossings.

The source-journal recovery coverage landed at `2586925`: the Rust boundary
replays a recorded alien source and re-derives an unbound scalar global during
image recovery. R6 passed again at that checkpoint with 20/20 focused tests,
the documented fixture exception, and only the same two accepted crossings.

The malformed-alien continuation regression landed at `90d5181`: a bad
recorded source is skipped while a valid neighboring source still counts as
replayed. The complete R6 script passed again with 20/20 focused tests and no
new benchmark exception. Commit `bc56a67` hardens that best-effort contract by
clearing the stale diagnostic after a partial success; malformed entries are
still skipped, the valid replay count is returned, and `lastError` is clear.
The complete R6 acceptance script passed again at that checkpoint: 20/20
focused tests, the documented `sortedModules` fixture exception, and exactly
the two accepted crossings (`query.aggregate +5.8%` and
`compile.stage1-stdlib +11.9%`).

The scalar image-field coverage landed at `507522f`: the existing 20-test Rust
Codegen recovery case now replays and reads back float, boolean, symbol,
String, and nil values through Rust Store while retaining the heap-reference
fail-closed assertion. R6 passed again at that checkpoint with 20/20 focused
tests, the same fixture exception, and no change to the accepted benchmark
snapshot.

Export-reference recovery coverage landed at `20444bb`: the same scalar image
fixture now resolves an `ExportRef("Store")` through the frozen import/export
surface and verifies the live Store identity. R6 passed again at that
checkpoint with 20/20 focused tests, the documented fixture exception, and the
same two accepted benchmark crossings.

The follow-up heap regression at `e653bf4` covers cumulative relocation IDs: a
`VectorNode` points forward to a `RangeVt`, raw words survive the second-pass
fill, and a fact can use the relocated object as its Store entity. R6 passed
again at that checkpoint with 20/20 focused tests and the same accepted
benchmark snapshot.

Heap recovery landed at `f8ec3e7`: Rust Codegen now parses cumulative v4 Heap
records, imports each named vtable, allocates layout-sized objects, fills Oop,
raw, and code slots in a second pass, and swizzles `HeapRef` fields before the
Store load begins. An unresolved reference still fails closed without Store
mutation. R6 passed again at that checkpoint with 20/20 focused tests, the
documented fixture exception, and the same two accepted benchmark crossings.

Method replay preflight landed at `4fe24b2`: every recorded operation is now
emitted successfully before the first operation is evaluated, so a later
generated-C failure cannot leave an earlier method mutation applied. The R6
focused-output pin was corrected at `b20ccb5` to match the aggregate Zig build
report while retaining the exact 20-test boundary. The complete acceptance
script passed again with the documented fixture exception and only the two
accepted benchmark crossings.

Malformed heap framing coverage landed at `b0c9245`: a validly framed Heap
record that claims a missing object now fails during Rust heap materialization,
before the Store load begins, and the pre-existing Store value remains absent.
The complete R6 script passed again at that checkpoint with 20/20 focused Rust
tests, the documented fixture exception, and the same two accepted crossings.

The positive heap recovery fixture now spans two Heap records at `80a52bd`,
proving that cumulative relocation IDs survive record boundaries while forward
Oop references and raw words still swizzle correctly. R6 passed again at that
checkpoint with 20/20 focused Rust tests, the documented fixture exception, and
the same two accepted benchmark crossings.

Code-slot reconstruction was hardened at `6c85f4f`: v4 elides code payloads,
so Rust now explicitly zeros those words until mapped-unit recovery fills them.
A custom layout fixture checks the zeroed code slot alongside Oop and raw-word
reconstruction. Cargo/clippy, the focused 20/20 target, and the complete R6
acceptance script all passed with no new benchmark exception.

Invalid fact-operation coverage landed at `d1ff90a`: Rust now rejects
operation bytes outside assert/retract before `imageLoadBegin`, matching the
Store ABI's abort boundary while keeping malformed images fail-closed. The
focused Rust Codegen target remains 20/20 and the complete R6 gate passed
again with the documented fixture exception and the same two accepted
crossings.

The replay path now honors v4's explicit little-endian encoding for basis and
transaction words at `570d3e3`, rather than relying on the host byte order.
Cargo tests/clippy, the focused 20/20 target, and the full R6 gate all passed;
the accepted benchmark crossings remain unchanged.

The heap fixture at `8fe6b6c` now also covers a `NullOop` slot in a materialized
object, alongside forward Oop relocation, raw words, and elided code slots.
The focused Rust target remains 20/20 and R6 passed again with the same two
accepted crossings.

Rust image replay now rebuilds the relational rule cache at `bf54131` when the
`db` surface is present, matching the Zig loader after facts and the MOP mirror
are restored. The focused target and full R6 gate passed with no benchmark
change.

The Rust PEG default switch landed at `734cb2c`: canonical
`libbirnam_id_peg.dylib` now selects Rust, while the Zig matcher remains
available through `-Dpeg=zig` and the installed oracle filename. Rust-selected
ReleaseSafe verify and the complete R6 acceptance gate passed after the switch.
The benchmark comparator retained exactly the two accepted crossings:
`query.aggregate` +5.8% and `compile.stage1-stdlib` +11.9%; no additional
exception was accepted.
