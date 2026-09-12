# 60P — Complete V2 control and retire Frame layout V1

Status: implemented and verified, 2026-09-11.

The user requested completion of V2 suspension/capture and removal of the old
Frame layout after 74O exposed the incomplete control paths. This authorizes
runtime implementation and migration of handwritten probes. Public IdApi and
Method ABIs, canonical descriptor-v2 bytes, image formats and language malleability
remain intact. Unrelated version-1 protocols are not Frame layout V1.

## Control contract

A suspension or capture yields a result-bearing invocation. It uses an existing
V2 Call edge: source/target PCs, result destination, parallel copies, clear map
and root maps already describe the state before and after the supplied answer.
No new serialized edge kind or compiler-specific selector recognition is needed.

Publication validates the delimiter and exact Call edge, then stages it without
committing. The visible PC is the successor; predecessor roots remain active.
A Machine microcode override may convert an already staged matching Call into
Suspend or Capture. Mismatched result slots, PCs and transitions are rejected
before mutation. Ordinary Id Machine operations expose these actions as
`suspendAt:continueAt:` and `captureAt:continueAt:`.

While suspended or detached, the old root state and pending edge retain values
needed by the transfer. Resume/restore supplies the result and commits the
parallel copies, clears and successor roots exactly once before invoking the
next StepMethod. Continuations preserve one-shot consumption, original code
ownership and existing native-entry barriers. No native C activation is captured.

V2 becomes the only Frame representation. Handwritten probes declare genuine
V2 layouts and edges, using conservative root maps where precision is irrelevant.
No runtime switch, compatibility interpreter or synthetic V1 transition path is
retained. Tests for unsupported raw Frame slots move to V2 bounds/root-map checks.

## Required validation

- Real driver suspension and capture/restore with compiler-produced V2 layouts,
  nontrivial copies/clears, changing root maps, and correct supplied answers.
- Ordinary Machine sends and overridden microcode retain their effects.
- GC while paused/detached; active-generation ownership and one-shot consumption.
- Invalid/mutated actions, wrong results/PCs, nested-entry barriers and failed
  restore leave the appropriate prior state intact.
- Migrate the V1 probes and retain the broader runtime, native-tier, reflective,
  continuation and generated-deoptimization regressions.
- Debug/release checks and the full production verification gate; record actual
  completion evidence here before claiming the implementation finished.

## Implementation notes

The compiler already emits result-bearing Call edges for ordinary sends. A
Machine `callSite:` override can publish the matching Suspend/Capture action
through the new ordinary methods, so no compiler selector recognition or new
serialized edge is necessary. The driver validates the returned action against
the exact pending result destination. Publication preflights action storage and
numeric capabilities before staging; resume checks epoch capacity before commit.

All handwritten descriptors now go through V2 validation. The broader release
run exposed probe function-address assumptions: identical inert functions were
folded together, and an identity helper could acquire distinct addresses across
codegen units. Test-only barriers and an out-of-line helper make those assumptions
explicit. The registration production code is byte-identical to `24e200a3`;
compatibility runners compare its production portion while allowing maintenance
of the trailing `cfg(test)` module. Other frozen files remain byte-identical,
and receipts continue recording whole-file hashes.

The focused checks cover four executions: actual compiler-generated code and a
cyclic-copy layout, each suspended and captured. They check ordinary Machine
publication, tampered result rejection, rejected publication without Frame
mutation, precise GC with native-stack scanning disabled, changing root maps,
parallel copies and clears, wrong-Machine restore without consumption, and
one-shot completion. Existing regression tests retain generation-pin and native
entry-barrier coverage. General deoptimization reconstruction admission remains
separate work; completing V2 does not implement general inlining or unboxing.

## Completion evidence

[60P receipt](../measurements/60p-v2-control.json) records commands, source hashes,
frozen-authority comparisons and the four matching debug/release executions.

- Focused debug: 2 tests, all four suspension/capture executions pass.
- Entire runtime crate in release: 139 passed, 7 diagnostic tests ignored.
- `CARGO_TARGET_DIR=.cargo-target MISE_TRUSTED_CONFIG_PATHS="$PWD" mise run verify`:
  455 workspace tests passed, 11 diagnostic tests ignored; native-prefix,
  image/registration lifecycle, two clean archive builds and relocated execution
  all passed. The clean builds produced identical source/output closures.
- All eight compatibility runners import successfully; the authority comparison
  accepts test-only changes and rejects a production mutation.
- No Frame layout V1 constructor, constant, direct PC setter or raw-slot branch
  remains in the Rust runtime. Public ABI, descriptor encoding, image and compiler
  authorities remain unchanged. No source changes occurred during the final gate.

An earlier gate attempt correctly rejected concurrent source edits; only the
final frozen-source run is recorded as successful. The user installation and
remote repository have not been changed by this task.
