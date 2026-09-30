# 78 — Invariant-preserving refactoring review

> **Status: proposal; adversarial document review complete, 2026-09-15.** This records
> findings against `b8d9f255`. It authorizes no implementation, changes no project
> invariant, and does not accept any optimizer or performance result. The user
> requires the proposed refactors to preserve malleability and every other
> project invariant. Implementation remains subject to the gates below.

The bounded commit sequence and execution gates are specified in
[79 — Refactoring implementation plan](79-refactoring-implementation-plan.md).
This review remains the governing invariant and scope record.

## 1. Current state and recommendation

Birnam is an active pre-alpha, late-bound object environment with a Rust-only
production build/runtime path and arm64 macOS support. The live object model
is canonical; the relational Store is a derived reading. Linux remains future
work. The inspected checkout was clean at `b8d9f255`.

Design 74's four consumer families are integrated. [74AK](74ak-bounded-qualification-and-defaults.md)
keeps unboxed numeric intermediates, loop reuse, closure/literal elimination,
and counted indices opt-in following the bounded slowdown screen. Existing
aggregate defaults remain enabled. Refactoring must not change these defaults,
admission policy, refusal behavior, or the evidence required for promotion.

[77](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/77-validation-workflow.md) implements the current validation workflow.
The review re-audited `.cargo-target/validation/routine-final-verified.json`:
all 36 retained steps matched their recorded inputs, artifacts, logs and
prerequisite receipts. That is an evidence audit, not a fresh test run or
exhaustive, release, or hosted-CI acceptance. These results describe the inspected
baseline, not future refactor correctness.

[75](../../design/75-code-quality-and-responsibility.md) remains substantially applicable.
This review adds optimizer preparation and recovery-transition opportunities,
and narrows the earlier recommendation to meet the user's invariant-preserving
scope. In particular, the worker-selector correction is **excluded from the
refactor batch**, even though the discrepancy is real.

Recommended order:

1. Share equivalent language-byte mechanics, exact-floor discovery, and
   relational result writing in independent bounded changes.
2. Inventory native materialization ownership and failure paths before moving
   its code and state together. This has the greatest architectural payoff.
3. Separate host parsing/project data, then project orchestration and startup.
4. Explore reusable optimizer analysis and recovery-transition mechanics only
   after their equivalence and lifetime obligations are explicit.

No new general optimizer framework, dispatch cache, concurrency model, dynamic
library, public interface, image version, or compiler policy is proposed.

## 2. Invariant ledger

The authoritative designs below prevail over every proposed convenience. These
are concrete rejection criteria, not a claim that documentation proves safety.

| Invariant | What a refactor must preserve | Authority |
| --- | --- | --- |
| Malleability and dispatch | Fresh dynamic lookup at the existing send points; method replacement, inherited dispatch, shadowing, custom metacircular lookup, DNU, and reflection remain effective. Existing guard checks and fallbacks keep their exact authority and timing. | [42](../../design/42-object-canonical-relational-derived.md), [60A](../../design/60a-stackless-execution-semantics.md), [60F](../../design/60f-resumable-compiler-lowering.md), [74I](74i-public-api-guard-boundary.md) |
| Evaluation and observation | Receiver evaluated once, then arguments once left to right, before lookup. Preserve send/callback counts, error precedence, effects, allocation charges, GC/finalizer boundaries, and observable Frame state. Equal final values alone are insufficient. | 60A, 60F, [74](74-malleable-multicore-execution-and-optimization.md) |
| Active implementation ownership | Active/suspended Frames retain their entered implementation; later sends see current dispatch. Blocks, Continuations, bindings and leases retain needed code. Cache eviction is not permission to unload. | [60E](../../design/60e-resumable-registration-and-entry.md), [60J](../../design/60j-runtime-identity-and-history-compaction.md), [60N](../../design/60n-portable-resumable-native-carrier.md) |
| Value and lexical semantics | Preserve numeric results and error cases, lexical scope, shared mutable Cells, capture binding/identity, and global/Store reads at their original execution points. No expression is reevaluated or hoisted across observation. | 60F, [64](../../design/64-pipeline-threading-and-combinators.md), 74 |
| Object identity and durability | Preserve shared versus equal-but-distinct Oops, private vtables, captures and aliases; image round trips retain the same identity graph. Process-local Machine/control objects do not become durable. | [60O](../../design/60o-image-v8-identity-preservation.md), [60M](../../design/60m-portable-code-object-protocol.md), 60A |
| GC and bounded retention | Keep exact root ownership and safe-point handoffs; no raw Oop in a Rust container becomes an implicit root. Preserve weak indexes, retirement ordering, resource limits and the fixed-live-set retention bound. | 60J, 60N, [74AF](74af-consumer-teardown.md) |
| Metacircular protocol and expressibility | Preserve the existing overridability of Machine microcode and the Birnam-visible identity/reflection of Machine and StepAction Oops. Language/compiler policy must remain expressible in Id semantics. Keep the existing canonical action recognition and direct payload decoding; this does not introduce a send to interpret each action or support new action kinds. | 60A §3, 60F §4, 74 §§3.1/3.3 |
| Arbitrary native participation | Ordinary native Methods remain callable without compiler, Frame, deoptimization or recovery metadata. Opaque native calls are never replayed, speculated or reconstructed by this refactor. Preserve the existing bounded native-stack behavior of resumable execution, native barriers and legal control operations; do not promise suspension across arbitrary native calls. | 60A, 60E, 74 §§3.2, 5.3, 6.3 |
| Reentry and thread ownership | Preserve owner-thread Machines/Runtimes, native barriers, nested entry, and one-shot continuation behavior. No newly extended exclusive borrow or lock spans a callback-capable operation. | 60A, 60E, [60P](../../design/60p-v2-control-completion.md) |
| Object authority and policy | Store remains derived from the live object environment. Birnam-authored compilation/project policy stays replaceable where it is today; Rust sharing must not freeze it into a native shortcut. | 42, [44](44-toward-self-hosting.md), [65](../../design/65-self-describing-project-publications.md) |
| ABI, formats and bootstrap | Preserve IdApi/Method layouts, exported symbols, exact-current format admission, descriptor/generated-code contracts, source/native-load equivalence and independent cold bootstrap. Historical superseded layouts are not restored. | [60I](../../design/60i-stackless-artifact-byte-grammar.md), [60L](../../design/60l-portable-program-and-image-v7-byte-grammar.md) as amended by 60O/60P, [62](../../design/62-rust-native-rungs-migration-plan.md) |
| Publication | Ordinary run/install consume the successful publication without reading source or reparsing manifests; preserve rollback, offline dependency rules, diagnostics and native-library lifetimes. | 65 |
| Independent evidence | Production implementations may share proven mechanics; independent byte readers, golden fixtures, compiler interpreters and semantic comparison oracles remain independent. | [72](72-rust-mechanical-consolidation.md), 75, 77 |

A cached canonical layout proves a relationship between immutable compiler data.
It does **not** prove that a runtime method, lookup policy, constructor capability,
API callback or native object payload is still current. Do not use compiler
analysis reuse as runtime admission authority. A reflective guard that adds a
lookup effect is also a semantic change, as 74I demonstrates.

## 3. Candidates

Line numbers below are navigation hints at the inspected commit. Named functions
and modules identify the actual targets.

### R1 — Shared language-byte mechanics

**Strength: strong, with a narrowed scope.**

Evidence:

- `birnam-codegen/src/lib.rs:687` and
  `birnam-codegen-runtime/src/lib.rs:3400` duplicate selector classification;
  portable code and checked Artifact readers have related classifications.
- `birnam-codegen/src/lib.rs:981`, `birnam-codegen/src/portable.rs:247`, and
  `birnam-codegen-runtime/src/lib.rs:3138` repeat literal-body decoding.
- `birnam-host/src/lib.rs:7750` treats `_private` as arity one; Codegen treats
  it as zero. Source establishes the discrepancy; no worker reproduction was run.

All Rust paths in this section are under `rust/`.

**Before → after:** equivalent caller loops → shared pure byte-rule modules,
with caller-specific admission and errors retained. This gives locality to
language mechanics and leverage across existing callers.

**Permitted scope:** share only equivalent classifications; leave the host's
non-equivalent worker predicate unchanged. Share literal-body decoding while
retaining quote-envelope checks, initial capacity, recognized and unknown
escapes, embedded NULs, trailing-backslash behavior, and exact output bytes.
Keep checked portable-name length/NUL/quantity validation and failure order.
Do not impose UTF-8 or source-spellability on admitted format bytes.

**Explicit exclusion:** fixing `_private` worker admission is a separate
observable behavior correction. It must not be presented as a semantics-neutral
refactor or smuggled into this batch. This narrows Design 75 Q1 for this review;
it does not silently rewrite that earlier proposal.

**Proof required:** expected arities/bytes through actual caller interfaces,
including malformed envelopes and checked format refusals; generated-C/portable
byte comparisons. Keep independent fixture readers independent.

### R2 — Exact selected-floor discovery

**Strength: strong.**

Evidence: `rust/birnam-tcc/src/lib.rs:240,283`,
`rust/birnam-codegen-runtime/src/lib.rs:1806`, and
`rust/birnam-image/src/lib.rs:566` repeat `dladdr`, platform flags and opening
the library supplying `IdApi.dlsym`.

**Before → after:** repeated loader sequences → one stateless discovery module;
caller-owned symbol typing, caches and handles remain outside its interface.
Locality improves for unsafe mechanics; leverage comes from three real callers.

Preserve exact selected-image lookup under local symbol visibility. Retain
caller-specific negative caching, acquisition timing, handle retention, null
checks and loader callback order/count. Do not consolidate caches, use a global
symbol search, add retries, or introduce automatic `dlclose`. A shared module
does not grant thread safety or multi-Runtime support.

**Proof required:** locally loaded selected floor and competing library names,
missing symbols and failed acquisitions, callback trace equivalence, retained
handle behavior, relocation, and existing native export/dependency audits.
This is mechanical preparation for platform work, not Linux support.

### R3 — Relational output writing, then evaluator separation

**Strength: strong for output sharing; conditional for clone removal.**

Evidence: `rust/birnam-relational/src/lib.rs:73` consumes a `Value` to create a
`ClassifiedValue`; ten projection/rule output paths use `iter().cloned()` before
conversion, including lines 1550, 1592 and 4136. String conversion only reads
byte length. Pure evaluation and native Store adapters also share this file.

**Before → after:** repeated capacity/write loops → one output-writing module;
later, pure evaluation → native adapters with explicit ownership. This gives
locality to output safety and leverage across ten paths.

First share writing with the existing conversion/allocation behavior. Separately
consider borrowing instead of cloning only after proving the removed allocations
are Rust-owned result-buffer copies, with no Birnam allocation charges, roots,
callbacks, finalizer scheduling or admitted allocation-failure contract attached.
Do not remove Birnam object allocations or alter value identity. Host-memory cost
may change; no speedup claim follows from source inspection.

Keep Store collection, input checks and status precedence in their current order.
Preserve output order, whole-result capacity checking before any write,
success-only `written` assignment after all record writes,
null-output/zero-capacity handling, String byte length and record padding.
Never construct a slice from null merely because its length would be zero.

Keep raw `output` and `written` pointers through Store callbacks and native
input parsing. Do not create an exclusive slice/reference over output storage
before callbacks and reads of potentially aliased input storage have completed.
An FFI caller may reuse writable input scratch for output where the current
contract permits it; a helper must not impose new non-aliasing, alignment,
initialization, or accessible-extent preconditions.
Preserve all-or-nothing publication: on refusal, neither output bytes nor
`written` may be modified where the existing path leaves them unchanged.
Do not zero output or set `written = 0` on entry as a convenience.

**Proof required:** native-adapter cases for capacity, empty/String output,
malformed inputs and competing errors; permitted aliased input/output scratch,
callback-before-write traces, and sentinel output/`written` on refusals;
existing query matching/order/identity
regressions. Move the pure evaluator without changing algorithms, deduplication,
query equality, rule planning or Store authority.

### R4 — Native materialization ownership

**Strength: strong architectural opportunity; highest-risk extraction.**

Evidence: `rust/birnam-codegen-runtime/src/lib.rs` holds shared registries at
lines 244–281, root reporting at 586, synchronous materialization at 6358,
named materialization at 7817, resumable-block materialization at 7967, and
binding retirement/reaping at 7771/7777. Three compile paths repeat setup and
cleanup while coordinating different root, registration and retention states.

**Before → after:** compilation paths coordinate shared registries → a private
materialization module owns its existing state and lifecycle operations;
publication/reflection call through that seam. This improves locality for native
lifetime reasoning and leverage across the three compile paths.

First inventory each path's Unit, generation, registration stage, roots, cache
retention, leases, native handles, callback-capable operations and failure exits.
Then extract code and its state without changing transitions, cache limits,
resource accounting, synchronization scope, or singleton lifetime. Only after
that slice passes may equivalent setup be shared.

Preserve distinctions between failure before admission, stage cancellation,
publication, withdrawal, cache eviction, retirement request, floor-confirmed
quiescence and physical release. The floor decides execution/retirement
eligibility; TCC owns native allocations; Codegen owns its retained references.
Do not replace explicit fallible compensation with `Drop`, infer unloadability
from cache absence, hold locks over sends/GC, or shorten fresh-Oop rooting. Preserve ordinary native Methods with no recovery
metadata and their direct/synchronous entry paths; a unified materialization
module must not require such metadata from every callable implementation.

**Proof required:** failed compilation/registration and compensation; replacement
while old Blocks/Frames/Continuations remain live; nested callbacks; cache churn;
rollback; mapped/rebuilt images; eventual collection and actual native release.
Include a handwritten native Method without compiler metadata that records a
side effect, reenters Birnam, and is replaced before a subsequent send. Require
one effect per call, current subsequent dispatch, retained old code where needed,
and unchanged native barrier/control behavior. Reuse bounded native-stack and
long-running tail-transition coverage; no opaque native effect may be replayed.
Reuse `scripts/portable-code-binding-lifecycle-smoke.sh`,
`scripts/unit-cache-lifetime-smoke.py`, `scripts/evaluator-unit-lifetime-smoke.py`
and current image/native-lifetime suites. Existing tests are starting points;
map every inventoried failure exit to evidence before consolidating it.

### R5 — Host responsibility separation

**Strength: strong, beginning with pure parsing/project data.**

Evidence: `rust/birnam-host/src/lib.rs` combines command types/parsers,
Git dependency acquisition (2919), publication (4433), `HostRuntime` (4730),
project bridges and runtime startup (8267).

**Before → after:** one shared implementation context → command, project-workflow
and runtime-startup modules behind the existing coordinating interface. Locality
improves for project and platform changes without requiring a generic framework.

Move parsers and pure project-data functions and their existing tests first.
Then separate project orchestration from startup. Preserve existing public
re-exports, exact diagnostics, error precedence and caller-owned GC anchors.
Keep callback-visible `HostRuntime` identity/address and bridge registration
valid; moving a Rust value is not automatically safe while native state retains
its address. Preserve explicit bridge clearing and loaded-library lifetime.
Do not move Birnam project policy into Rust or make ordinary run/install consult
source. Preserve publication rollback, dependency order and offline behavior.

**Proof required:** parser errors before runtime access; project-check workflow,
publication failure/rollback, source-independent launch/install, bridge callback
lifetime, worker startup and relocated runtime loading.

### R6 — Reuse canonical optimizer analysis

**Strength: worth exploring; cost benefit unmeasured.**

Evidence: `rust/birnam-runtime/src/runtime/native_tier.rs:541,610,623` colors a
program, derives a numeric template and requests maps;
`rust/birnam-codegen/src/numeric_chain/recovery_maps.rs:45–46` derives the
template and checks coloring again. Numeric closure composition repeats the
template at `closure_elision/numeric_body.rs:39–40`; closure preparation and
`closure_elision.rs:147` repeat coloring.

**Before → after:** repeated analysis in composed calls → one private owner of
an immutable program's validated layout, with optional derived data reused only
within the existing bounded preparation lifetime. This gives locality to compiler
proofs and leverage to current composed callers.

Keep public checked entry points and their malformed-layout rejection. Reuse
requires the exact program, layout and lowering context; pointer equality,
a digest alone, or mutable runtime identity cannot establish that relationship.
No persistent/global cache is proposed. Preserve lazy preparation and existing
resource limits, admission/refusal decisions and disabled-family behavior.
Retain no AST, source bytes, native owner or root beyond its existing lifetime.
If sharing retained objects changes accounting or refusal thresholds, that is
outside this refactor and requires a separate design.

Reuse compiler facts only. Every live method/API/constructor/lookup guard still
runs at its original observation point. No analysis may authorize skipping a
send, allocation charge, mutation check, failure, or recovery path.

**Proof required:** malformed/mismatched layouts, differing program bytes,
disabled/refused/accepted families, current mutation and callback matrices,
preparation limits, retained bytes and teardown. Measure cold/refused/enabled
costs separately; existing performance regressions do not authorize promotion.

### R7 — Recovery-slot transition mechanics

**Strength: worth exploring; equivalence audit required first.**

Evidence: four production analyses simulate copies/results/clears in
`rust/birnam-codegen/src/numeric_chain/recovery_maps.rs:160`,
`virtual_aggregate.rs:141`, `closure_elision.rs:324`, and
`closure_elision/sequence.rs:178`.

**Before → after:** duplicated production transition simulation → shared proven
mechanics used first by the two closure analyses, retaining family-specific
admission and alias domains. Locality improves for V2 transitions; leverage comes
from multiple production analyses rather than sharing with the test oracle.

Current implementations differ in bounds behavior and clear/result ordering.
Do not silently normalize them. Establish equivalence over each caller's admitted
layouts, including result/clear overlap and absent bitmap words. If behavior
differs on admitted inputs, keep those paths separate and report the discrepancy;
a correctness correction is not authorized by this document.

The public `virtual_aggregate::Plan::derive` checks local shape/bounds but does
not require `color_slots(program) == layout`. Numeric `RecoveryMaps::derive`
and closure `Plan::derive` do require canonical coloring. Preserve these
caller-specific admitted domains: do not force the aggregate entry point through
R6's canonical-analysis owner or add a shared canonical-layout gate. Tests using
only ordinary runtime-produced canonical layouts cannot detect this narrowing.
If the shared primitive cannot preserve the aggregate caller's existing domain,
keep that caller's implementation separate.

Preserve snapshot-before-write parallel copies, result initialization, slot
clears, PC/continuation selection and observable root/alias state. No runtime
Machine slot send or guard is removed. Do not share this implementation with the
independent colored/uncolored execution interpreters or format readers.

**Proof required:** alias identity, parallel-copy cycles, root/clear maps,
caller-specific malformed-layout refusal and accepted noncanonical aggregate
layouts, actual Frame comparison and forced recovery at every
relevant exit, including mutation, collection, capture/suspend/resume and both
GC modes. Existing numeric map, aggregate and closure sequence tests anchor this
comparison; complete matrices use the current validation plan.

## 4. Lower-priority observations

- The native-tier parent registry owns closure budget/staging/resume state
  manipulated by its closure module. A later ownership extraction must retain
  detached-owner drop after registry borrows end and the existing preparation
  state machine; it is not part of R6 or R7.
- Migration-era broad `allow(dead_code)` and opening comments need an audited
  cleanup. For example, Codegen runtime's introduction says it does not invoke
  TCC despite its materialization paths. In-tree absence alone does not establish
  that exported, generated-code or fixture interfaces can be removed.
- Line count is a navigation symptom, not the reason for extraction. A move that
  merely exposes the same registries across more modules provides no ownership
  improvement.

## 5. Implementation and acceptance gate

For each selected slice:

1. Record its affected rows in the invariant ledger, its existing observation
   points, admitted inputs, failure paths and owners. State the allowed internal
   cost change separately from externally observable behavior.
2. Make move-only changes separately from shared-mechanics changes. Preserve
   interfaces and all current non-equivalent behavior. Do not widen scope to
   resolve a discovered discrepancy.
3. Test through real production caller interfaces, using independent expected
   results or existing independent oracles. Compare effects, errors, identity,
   callback order, Frame state, allocation accounting and lifetime—not just
   values or successful execution.
4. Follow [the current validation guide](../../validation.md): focused checks for
   iteration; routine correctness; exhaustive coverage for compiler/runtime/GC/
   recovery/language/ABI changes; release qualification when bootstrap, loading
   or publication changes affect cold/relocated behavior. Both supported GC
   modes and required profiles remain in their relevant matrices.
5. Bind final evidence to final source/artifacts. A timeout, missing case,
   interrupted matrix, or old baseline receipt is not a pass. Do not reduce
   coverage to meet a budget. Performance claims need separate scoped evidence.
6. Reject the slice if it needs a change to a ledger invariant or cannot prove
   equivalence at an affected observation point. Report the issue separately;
   do not resolve it by weakening an invariant or updating the oracle to match.

No invariants are amended by this proposal. No implementation is qualified yet.
The expected outcome is less duplicated implementation and clearer ownership
with the same malleable language environment and existing public contracts.

## 6. Adversarial review record

An independent agent challenged the draft against current source and accepted
contracts, then re-read the revised proposal. The full findings, source-derived
counterexamples, limitations and final disposition are in
[78A — Adversarial review](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/78a-refactoring-adversarial-review.md).

| Finding | Revision | Final disposition |
| --- | --- | --- |
| F1: implicit arbitrary-native and metacircular contracts | Added native participation without recovery metadata, no opaque replay, bounded resumable stack behavior, Machine microcode overridability, StepAction Oop visibility with unchanged direct decoding, and Id-semantic expressibility; added R4 evidence requirements. | Addressed at proposal level |
| F2: output sharing could impose new FFI alias/borrow constraints | R3 keeps raw pointers through callbacks/input reads, preserves existing caller preconditions and whole-result capacity checks, and requires alias/sentinel/ordering cases. | Addressed at proposal level |
| F3: recovery analyses admit different layouts | R7 records aggregate local validation versus numeric/closure canonical validation; a shared canonical gate is prohibited, and accepted noncanonical cases need coverage. | Addressed at proposal level |

The final independent verdict found no unresolved material objection to the
proposal's invariant-preserving scope. No proposed refactor requires weakening
an invariant as constrained here. This is not empirical proof of an unimplemented
refactor: each selected implementation still owes its ownership/equivalence
inventory, focused counterexample checks and applicable final validation gates.
No source behavior, project invariant, optimization default or tracker acceptance
was changed by writing or reviewing these documents.
