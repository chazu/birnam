# 79B — Refactoring delivery and evidence

Status: implementation delivered within the gates below; integrated acceptance
running. Authorized 2026-09-15. Governing contracts: [78](78-invariant-preserving-refactoring-review.md),
[78A](78a-refactoring-adversarial-review.md), [79](79-refactoring-implementation-plan.md),
and [79A](79a-implementation-plan-adversarial-review.md).

## Scope

Source baseline: `b8d9f255`. Candidate source: `96bdd2b8`. The initial dirty
review/plan/navigation documents were retained in a planning checkpoint. Epic
`birnam-35m` and children `.1`–`.7` track A–G. Recommended sequencing is not proof
of a technical dependency or permission to bypass a gate.

| Series | Delivered | Remaining gate / exclusion |
| --- | --- | --- |
| A: language bytes | Shared unchecked selector arity and literal-body decoding; caller validation preserved. | Integrated acceptance pending. |
| B: exact floor | Shared acquisition mechanics for TCC registration/retirement and Codegen; each adapter retains its cache and lookup policy. | B5 Image migration deferred: Image source is a frozen compatibility authority. |
| C: relational | One raw output writer for ten adapters; pure evaluator separated from native adapter; unnecessary Rust String copies removed. | Integrated acceptance pending. |
| D: materialization | D1 cache and D2 CodeBindings storage, operations and root segments have private owners. | D3/D5/D6 need the broader failure/lifetime matrix. D4 has non-equivalent orchestration order. See [79C](79c-materialization-ownership.md). |
| E: host | Command, project data, filesystem, runtime and startup responsibilities have private modules with existing public exports. | Integrated acceptance pending. |
| F: analysis reuse | Static caller and ownership audit. No production changes. | Paired preparation-cost and retained-lifetime evidence absent. See [79D](79d-optimizer-refactoring-gates.md). |
| G: transition sharing | One private alias transition for the two canonical closure callers. | Numeric and aggregate sharing deferred because their contracts differ; see 79D. |

No user installation or remote repository has been changed. All native acceptance
uses a disposable external prefix. This document does not claim completion of B,
D or F, or promote an optimizer family.

## Preserved boundaries

- **Malleability:** live send guards, Store authority, replacement/withdrawal,
  compensation and retirement decisions remain at their existing boundaries.
- **Compatibility:** all eight frozen recovery authorities match the existing
  checker and baseline. The ABI root, IdApi layout/fixture, frame descriptor,
  registration authority, resumable emitter and Image lib/v8 remain protected.
- **Native lifetime:** no new root provider, Oop representation, compilation
  transaction, compensation destructor or physical-release authority. Root visit
  order and the long-lived module-source guard remain intact.
- **Caller domains:** checked readers, envelope validation, host `_private`
  predicate, refusal ordering and optimizer defaults remain unchanged.
- **Native output:** collection/parsing/evaluation finish before publication;
  capacity refusal leaves output untouched; records are initialized fully and
  count is written last. Raw-pointer input/output and count/output overlap remain
  supported without introducing exclusive output references.
- **Host bridge:** `HostRuntime`, the process bridge pointer, address fencing and
  `Drop` retain their layout and in-place lifetime contract.

These are the implementation constraints and review conclusions. The evidence
below qualifies the delivered scope; it does not prove every possible GC or
failure path needed for the deferred materialization moves.

## Characterization and source review

| Area | Evidence and practical limit |
| --- | --- |
| A | Focused tests at each A1–A5 commit cover byte semantics and callers; checked formats and the host predicate retain their own tests. |
| B | Nine fresh-process cases exercise the actual TCC registration, TCC retirement and Codegen adapters: selected image versus a competing global symbol, acquisition/lookup counts, failed acquisition, missing symbol, and retained loader reference. Image remains unchanged. |
| C | 120 native cases across ten adapters cover refusal/sentinels, null/zero output, error precedence, callback-before-write, overlapping input/output and count/output, and every scalar/identity/String record field, including embedded NUL. The isolated fixture disables GC and makes no native lifetime claim. |
| C allocation comparison | All 120 pre-extraction allocation and floor-live deltas matched before clone removal. The ten two-String cases then used exactly two fewer Rust allocations each; other Rust counts and all floor-live deltas stayed equal. C6 module separation and the current candidate retain those same 120 rows. This is a bounded copy reduction, not a speed claim. |
| D1/D2 | Three fresh-process state cases cover LRU and failed-admission effects, staging retention, current/retiring binding roots, exact root order, source-lock duration, and whole-callback exit after poisoning. Opaque tokens use a recording mark callback, not the collector. |
| D source audit | Fixed baselines `b9ae3c77` for cache and `47a0216a` for bindings establish unchanged declarations/bodies after necessary private visibility. Image binding reservation stays before Store; post-Store publication retains the original abort behavior. Compile/registration/compensation stacks were not moved. |
| E | 86 ordinary host library tests and four build-driver tests pass. A separate real native bridge fixture covers failed startup, repeated same-address installation, competing-address rejection, matching clear and nonmatching preservation at in-place Drop. The competing token is never dereferenced. |
| E source audit | 375 of 377 parsed items match after private-visibility normalization. The two structural differences are the sibling-visible temporary-directory field and the split HostRuntime impl container; actual methods, HostRuntime and Drop match. |
| E workflow | The existing full project fixture covers build/check/test/run/install, malformed cache and rollback, then relocated published run/install with authored inputs and dependencies hidden. |
| G | All 97 pure Codegen tests pass after both migrations. Canonical caller rejection tests and independent physical Frame/emission oracles remain separate from the helper. Literal expected-value helper tests cover copy cycles, result/clear overlap and the 63/64 bitmap boundary; those cases are not claimed to reach canonical production layouts. |

The independent adversarial plan reviewer also re-inspected D1/D2 and G's actual
scope. No source-level invariant regression was identified in those bounded
moves. Its objections require current combined native/language evidence and keep
the broader D failure matrix open; the complete disposition is recorded in 79C.

## Acceptance

The final source-bound release plan contains **84 steps**: workspace/tool and
format tests, installation/native workflows, the five added fixture/project
steps, exhaustive debug and release matrices, evaluator/native-storage lifetime,
recovery, native exports and independent cold-source installation.

```sh
CARGO_TARGET_DIR=.cargo-target mise exec -- python3 scripts/validate.py \
  --suite release --deadline 21600 --output .cargo-target/validation/79-final \
  --prefix "$(cat /tmp/birnam79-prefix.txt)"
```

Current receipt: `.cargo-target/validation/79-final/run.json`. Current log:
`/tmp/birnam79-final-release-resumed.log`. Acceptance is pending until all 84
selected steps pass and the receipt audit matches current inputs/artifacts.
A passing subset invocation is not full acceptance.

Following the complete release receipt, the same prefix runs the language suite.
A separate diagnostic release target runs the existing unit-cache lifetime
fixture in leaf, numeric-body and numeric-body with counted-frame-index modes.
Those fixtures separately exercise native GC/retirement and normal-floor/TCC
identity. They are diagnostic lifetime evidence, not production timing evidence.
Follow-up receipt: `.cargo-target/validation/79-followup/run.json`.

### Validation issues resolved without weakening contracts

1. The first combined run reached `loop-invariant-debug`, then argparse rejected
   the generated command because the validation plan omitted required `--profile`.
   Commit `66ceacbb` supplies the requested profile and adds an actual parser
   regression test. Debug/release loop-invariant checks then passed. The changed
   plan invalidates earlier matrix receipts; affected steps are rerun.
2. Recovery subsequently caught edits to the frozen ABI root and Image lib.
   Commit `96bdd2b8` places the helper declaration under the existing ABI private
   module and restores Image lib byte-for-byte. B5 and its Image-specific fixture
   were removed from this batch. The compatibility checker and frozen baseline
   were not changed. All eight authority checks now match.
3. On `96bdd2b8`, the scoped preflight passed installation, all nine loader cases,
   all three codegen-state cases, recovery across all families, native exports and
   cold-source builds. This is six plan steps, not the complete 84-step release.

Earlier A-only and interrupted/failed integrated receipts remain historical
logs, not acceptance of the current candidate. Matching passed evidence may be
reused only through the existing source/artifact/dependency checks.
