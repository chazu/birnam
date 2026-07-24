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
baseline), then runs the latest N5.1-N5.4 Rust-Lang functional set: the
unchanged grammar suite, frozen CST parity, syntax diagnostics, canonical
Rust/explicit Zig rollback selection, and the optimized native-rung harness.
It then runs the complete `zig build verify -Doptimize=ReleaseSafe` closure,
verifies both file hashes, and runs the unchanged Design 46 comparator. It
fails unless the only threshold crossings are exactly the two entries below.
Any changed snapshot, functional failure, verify failure, missing or unusable
case, additional regression, or changed accepted measurement fails closed.

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

R6 is therefore complete. R7 may integrate and select the Rust floor while
retaining the Zig oracle and rollback path required by Design 61.

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
ReleaseSafe closure used by the acceptance evidence. `zig build verify
-Doptimize=ReleaseSafe` and `mise run r6-acceptance` were rerun on 2026-07-23:
both passed, the exact accepted hashes remained unchanged, and the same two
recorded crossings had zero missing or unusable cases. This additional
cross-layer, native-TCC, and Rust-Lang coverage does not widen the R6 exception
set or change the accepted snapshot.
