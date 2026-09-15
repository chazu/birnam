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
| A: language bytes | In progress | Caller inventory: unchecked compiler/runtime/portable-code arity; ordinary emitter, portable encoder and runtime literal decoding. Checked readers and host predicate are excluded. |
| B: exact floor | Pending | Loader fixture must preserve the image owning dlsym. |
| C: relational | Pending | Native aliasing/write-order characterization required. |
| D: materialization | Pending inventory gate | No lifetime extraction authorized by a passing byte-rule test. |
| E: host | Pending | Preserve HostRuntime address and bridge lifecycle. |
| F: analysis reuse | Pending investigation gate | No cost/default claims. |
| G: transition sharing | Pending investigation gate | Preserve each caller's admitted layout domain. |

No series is accepted by this baseline record. No user installation or remote
repository has been changed.
