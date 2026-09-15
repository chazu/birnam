# 79B — Refactoring delivery and evidence

Status: implementation in progress, authorized 2026-09-15. Governing contracts:
[78](78-invariant-preserving-refactoring-review.md), [78A](78a-refactoring-adversarial-review.md),
[79](79-refactoring-implementation-plan.md), [79A](79a-implementation-plan-adversarial-review.md).

## P0 baseline

Source baseline: `b8d9f255`. Only the review/plan/navigation documents were dirty
at entry; they are retained in the planning checkpoint. Beads resolves to this
checkout with prefix `birnam`. Epic: `birnam-35m`; children `.1` through `.7`
track series A through G. D depends on A/B/C; F/G depend on D's ownership gate.

Fresh audit of the retained routine receipt passed all 36 steps before source
changes: inputs, artifacts, logs and dependency receipts match. This audit is
not new runtime execution. The routine receipt remains at
`.cargo-target/validation/routine-final-verified.json`. Future acceptance must
bind changed source and rebuilt artifacts and include the plan's additional
language/project/characterization cases.

## Series status

| Series | State | Evidence / limitations |
| --- | --- | --- |
| A: language bytes | Implemented; acceptance running | Unchecked arity and literal body helpers; checked readers and host predicate preserved. |
| B: exact floor | Implemented; acceptance running | Twelve real-adapter lookup/cache/retention cases. |
| C: relational | Implemented; acceptance running | Ten raw output callers; 120 native cases and allocation comparison; evaluator/native separation. |
| D: materialization | D1/D2 implemented; remaining gate open | Cache/CodeBindings ownership only. See 79C for deferred D3–D6 and review findings. |
| E: host | Implemented; acceptance running | Command/data/files/runtime/startup modules; real address-fenced bridge and project fixtures. |
| F: analysis reuse | Deferred at evidence gate | Static caller audit complete; paired preparation/lifetime evidence absent. No production changes. |
| G: transition sharing | G1/G2 implemented; acceptance running | Canonical closure callers only. Numeric/aggregate sharing deferred; see 79D. |

No series is accepted by this baseline record. No user installation or remote
repository has been changed.

## A implementation checkpoint

A1–A5 are implemented in `05e1da91`, `c58cf96c`, `01a33f23`, `a8e741d1`
and `2ff668b5`. Artifact owns total unchecked selector arity; pure Codegen owns
literal-body decoding. Compiler, portable-code and runtime callers use them.
Caller-specific envelope validation, C escaping, checked format readers and the
host's `_private` predicate remain at their existing boundaries.

Focused caller tests passed at each commit (`/tmp/birnam79-a1.log` through
`/tmp/birnam79-a5.log`). The current exhaustive run binds `2ff668b5` source and
a rebuilt release prefix outside the checkout. Receipt:
`.cargo-target/validation/79a/run.json`. The first invocation rejected an
in-checkout prefix, as the installer requires; it was restarted with a disposable
external prefix. The installer guard is unchanged. Language acceptance remains
pending until the exhaustive run finishes. This is an implementation checkpoint,
not a completed-series acceptance claim.

## Characterization prepared during A validation

Draft fixtures are being qualified outside the source tree so they cannot
invalidate the running source-bound receipt. They are not yet durable tests:

- B: image-owned `dlsym` trampoline, competing global symbol, per-adapter
  acquisition/lookup counts and unload markers. Original TCC registration and
  retirement adapters pass success, missing-symbol and failed-acquisition cases
  in six fresh processes. Codegen/Image paths remain to be run.
- C: real floor, actual ten exported adapters and a handwritten synchronous
  Store. An independently compiled copy of the current relational source passes
  120 cases: whole-result refusal, untouched sentinels, null/zero output,
  null-count precedence, callback-before-write, overlapping rule-term storage,
  overlapping count/output, and fully initialized integer/float/identity/String
  records (including an embedded NUL). These tests have no GC lifetime claim;
  collection is disabled within their isolated test process. The full language
  and lifetime suites remain required.

## B/C and host command checkpoint

The follow-on commits are isolated on `refactor/79-next-series` in
`/tmp/birnam79-implementation`, with a separate Cargo target. The primary
checkout remains at A's source while its exhaustive receipt is running.
No series is accepted on focused evidence alone.

B1–B5: `4deeebe9`, `a814d547`, `660c951d`, `9a07e727`, `63d8032c`.
The real-adapter matrix passes all twelve cases before extraction and after
 each migration. Receipts: `/tmp/birnam79-b/baseline.json`, `b2.json` through
`b5.json`. The new `floor-discovery` validation step includes fixture sources
in its inputs and is included by dynamic CI core selection. Validation tool
tests pass. Release acceptance remains pending.

C1–C6: `2508dade`, `b9f9599d`, `fce2d588`, `9a15f8cd`, `5ad9e4e5`,
`1975957c`. All ten writers now publish through one raw-pointer helper after
collection, parsing and evaluation. The evaluator owns pure values/rules;
the adapter owns the same API/vtable/cache/root state and native exports.
The existing Rust surface is re-exported. The new `relational-output` step
runs the 120-case native matrix; its sources are validation inputs.

C4 audit: `Value` owns Rust scalar data and `Vec<u8>` String copies. Record
conversion has no IdApi, send, Oop allocation or root operation. It only reads
scalar fields/String length into a fully initialized 40-byte native record.
Before clone removal, all 120 allocation and floor-live deltas matched C1.
Afterward, the ten two-String cases each use exactly two fewer Rust allocations;
all other Rust counts and all Birnam live-object deltas are unchanged. Callback
and overlap assertions still pass. Module moves preserve those same deltas.
Receipts: `/tmp/birnam79-c/c1.json` through `c6.json`; semantic crate tests pass.
This supports the bounded Rust-copy reduction, not a runtime speedup or a GC
lifetime claim. Exhaustive/language acceptance remains pending.

E1 is `5f94bd9d`: command types/parsers and their tests moved to a private module
with existing public re-exports. The worker arity predicate remains outside it;
its characterization still invokes the existing predicate. Host crate tests
pass. Project data/filesystem/runtime separation is still underway.

A's first complete-suite attempt exhausted its 1800-second total budget during
`numeric-sequence-debug`; no semantic assertion failed. It was resumed against
unchanged A source with `--deadline 7200`, preserving matching passed steps and
all matrix coordinates. Current log: `/tmp/birnam79-a-exhaustive-resumed.log`.

## Host separation checkpoint

E2/E3 are `803a9595` and `c5d175fb`. Pure project records, validation, dependency
ordering and publication-source construction moved with their tests. Filesystem
staging, Git acquisition, source walking and rollback moved separately.

Bridge characterization (`5b308156`) ran before the live-runtime move. It invokes
an actual native `Project pathForUri:` callback, exercises failed startup,
checks repeated installation at the same address, rejects a competing address
before callbacks, and destroys the real runtime in place. Separate fresh
processes check matching-address clearing and nonmatching-address preservation.
The competing token is never dereferenced. Baseline receipt:
`/tmp/birnam79-e/bridge-baseline.json`; the same checks pass after E4/E5.
The existing full project smoke also passed against the A baseline prefix,
including relocated published run/install with authored input and dependencies
hidden. Its log is `/tmp/birnam79-e/project-baseline.log`.

E4 is `df36bfb4`; E5 is `697872b0`, `bc0110ed`, `2a9d9422`.
Live project orchestration and decoders, startup, interaction, and resource
resolution now have private modules. The coordinator retains `HostRuntime`,
`PROJECT_BRIDGE_RUNTIME` and `Drop`. Token-level comparisons confirm unchanged
object/Drop definitions and unchanged moved function/method bodies; only
necessary private Rust visibility changed. Host crate tests pass (86 normal
library tests, the native bridge fixture ignored in ordinary Cargo execution,
and four build-driver tests). The added `host-bridge` and `host-project` plan
steps exercise those separate proof obligations in fresh installed prefixes.
Their fixture/script sources are hashed inputs and CI core selection includes
them. Validation tool tests pass. Full release/language acceptance is pending.

## Integrated candidate

The follow-on work was fast-forwarded into the primary checkout at `2e8d14d6`.
A's earlier exhaustive run was interrupted deliberately and remains incomplete;
it is superseded for acceptance by the combined candidate release run, without
removing any required matrix coordinates. No source edits are made during it.

D1/D2 are documented in [79C](79c-materialization-ownership.md), including the
independent adversarial follow-up and its acceptance objections. F and G's
scope/domain decisions are in [79D](79d-optimizer-refactoring-gates.md). G commits
are `0dc608d1` (caller characterization), `edf5ebcd` (first caller/helper) and
`2e8d14d6` (sequence caller). All 97 pure Codegen tests pass at G1 and G2.

Current combined command:

```sh
CARGO_TARGET_DIR=.cargo-target mise exec -- python3 scripts/validate.py \
  --suite release --deadline 21600 --output .cargo-target/validation/79-final \
  --prefix "$(cat /tmp/birnam79-prefix.txt)"
```

The expanded plan has 84 steps, including the five new fixture/project steps,
both debug/release exhaustive matrices, evaluator/native-storage lifetime,
recovery, native export audit and independent cold-source installs. Receipt:
`.cargo-target/validation/79-final/run.json`; log:
`/tmp/birnam79-final-release.log`. Full language execution follows a passing
release receipt against the same private prefix. Completion is still pending.
