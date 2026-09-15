# 78A — Adversarial review of the refactoring proposal

> **Final proposal review: findings addressed, 2026-09-15.** Independent agent review
> of [Design 78](78-invariant-preserving-refactoring-review.md) against source
> at `b8d9f255` and the accepted designs. This reviews a proposal, not an
> implementation. No refactoring or semantic equivalence is qualified here.

## Verdict

The proposal correctly excludes the known worker-arity behavior correction and
contains substantial safeguards for malleability, native lifetime, callback
effects, optimizer authority and independent evidence. I found no proposed
change that explicitly requires weakening malleability. That is not a proof
that every implementation of these extractions preserves every invariant.

The initial review requested three medium-severity gaps be made explicit before
accepting the document. The follow-up review below records their resolution.
The examples below are source-derived counterexamples to plausible
implementations; they were not executed. Severity refers to the omission in
the proposal, not a newly confirmed production defect.

## Findings, ranked by severity

### F1 — Medium: the ledger incompletely states the arbitrary-native contract

**Location:** Design 78 invariant ledger; R4, R6, R7 and acceptance gate.

The ABI and reentry rows preserve calling layouts, barriers and ownership, but
do not explicitly preserve admission of arbitrary native Methods without
compiler recovery metadata. They also omit the explicit prohibition on
replaying an opaque native call during recovery, and the bounded native-stack
requirement for resumable control.

These are separate requirements in
[74 sections 3.2 and 5.3](74-malleable-multicore-execution-and-optimization.md)
and [60A section 2](60a-stackless-execution-semantics.md). Compatibility of the
IdApi table alone does not prove them. Design 75 also states arbitrary native
participation expressly in its section 2 contracts.

**Counterexamples:** an extracted materialization/entry owner requires a
portable descriptor for every installed Method; an optimizer preparation owner
refuses an otherwise ordinary external Method because it lacks recovery maps;
recovery repeats a native Method that already incremented an external counter.
All can leave the C ABI and eventual return value unchanged. Turning a resumable
recursive path into recursive native helper calls similarly preserves small
examples while losing the stack bound.

**Required change:** explicitly preserve ordinary native participation without
recovery metadata; treat opaque calls as potentially allocating, mutating and
reentrant; forbid replay; retain native barriers and bounded resumable stack
use. Preserve expressibility of language/compiler policy in Id semantics,
Machine microcode overridability, and the Birnam-visible identity/reflection of
Machine and StepAction Oops. Preserve the driver's canonical action recognition
and direct payload decoding: 60A section 3 explicitly prohibits interpreting an
action by sending it `execute` or another ordinary message. Add relevant native
callback/recovery and deep resumable-control cases to affected slices' evidence
maps; do not demand these unrelated suites for a pure byte-decoder extraction.

### F2 — Medium: R3 needs an explicit raw-pointer and aliasing boundary

**Location:** Design 78 R3 and its proof requirements.

The current code finishes callback-capable Store collection and input parsing
before writing caller storage. For example,
[`project_rule_live`](../../rust/birnam-relational/src/lib.rs) around lines
4090–4144 calls `collect_project_facts`, `parse_rule_definitions`, classification
and `parse_rule_terms`, then evaluates, checks capacity, performs `ptr::write`
and finally writes `written`. `project_live` around lines 1505–1556 follows the
same broad ownership boundary. Neither implementation creates an exclusive
Rust slice over the output while these operations run.

**Counterexample:** a native caller reuses suitably aligned scratch storage for
the input nodes and subsequent output records. Once parsing has copied the
input into Rust-owned values, the existing raw writes can overwrite that
storage. Creating `&mut [ClassifiedValue]` at adapter entry and keeping it alive
through input parsing imposes a new exclusive-borrow constraint. Likewise, a
Store callback that accesses caller-owned output storage can conflict with an
eager exclusive slice even if all pointer arithmetic is in bounds. Merely
preserving the null/zero-capacity case does not address this.

A streaming writer introduces a simpler visible difference: it can write a
prefix and only then discover insufficient capacity; current paths check the
complete result length before their first output write.

**Required change:** keep raw output/`written` pointers until callbacks and
input reads finish. Do not introduce new non-aliasing, alignment, initialization
or extent requirements through a convenient safe-slice interface. Preserve
untouched output and `written` on refusal, whole-result capacity checking, and
the final `written` write after all record writes. Test admitted input/output
overlap and callback access where the native contract permits them, along with
sentinel buffers on competing errors and insufficient capacity. These cases
must use real adapters; a pure writer unit test cannot establish their ordering.

This is an extraction constraint, not an assertion that every arbitrary raw
pointer overlap is valid. Test constructions must meet the existing caller
contract and Rust validity rules.

### F3 — Medium: R7 needs the actual caller admission split recorded

**Location:** Design 78 R7; also R6's checked-entry-point requirement.

R7 already says not to normalize different bounds and clear/result behavior.
The source reveals why checking only runtime-generated canonical layouts is
insufficient:

| Production analysis | Current admission and transition behavior |
| --- | --- |
| `numeric_chain::RecoveryMaps::derive` | Re-derives a template and requires `color_slots(program) == layout`; copies from a snapshot, clears, resets result, then the outer derive records the new result alias. |
| `closure_elision::Plan::derive_with` | Requires canonical coloring; copies, sets its result alias false, then clears. |
| `closure_elision::sequence::extend` | Reached through closure preparation; copies, sets result false, then treats missing bitmap words as no clears. |
| Public `virtual_aggregate::Plan::derive` | Checks local shape and bounds but does not require canonical coloring; copies, sets the result alias to `Some(pc)`, then clears. |

Evidence is in
[`recovery_maps.rs`](../../rust/birnam-codegen/src/numeric_chain/recovery_maps.rs)
around lines 44–48 and 160–172,
[`closure_elision.rs`](../../rust/birnam-codegen/src/closure_elision.rs) around
147–149 and 324–342,
[`sequence.rs`](../../rust/birnam-codegen/src/closure_elision/sequence.rs) around
178–194, and
[`virtual_aggregate.rs`](../../rust/birnam-codegen/src/virtual_aggregate.rs)
around 45–59 and 141–158.

**Counterexample construction:** begin with a two-or-more-step input admitted
by aggregate `Plan::derive`. Add an in-bounds clear bit for a call's result slot
without otherwise changing its valid references. The aggregate implementation
sets `Some(pc)` and then clears that slot. A helper that applies clears and then
publishes `Some(pc)` instead produces a different next-step alias map. A new
canonical-layout requirement can reject the altered layout outright, whereas
the existing aggregate public API need not reject it. Neither outcome is a
mechanical sharing of its existing behavior.

**Required change:** explicitly record this admission split and keep aggregate
sharing deferred until its accepted-input behavior is accounted for. Preserve
public caller refusal behavior even when current internal callers only supply
canonical layouts. The first closure-only extraction must establish that its
two bitmap loops agree on their reachable layouts. A canonical-only integration
test does not establish equivalence for every exported derivation API.

## Other risks checked and already constrained by Design 78

- **R1:** the ordinary emitter removes outer bytes from any length-at-least-two
  input; portable/runtime decoders require quote envelopes. For `b"abc"`, the
  ordinary decoder body is `b"b"` while the others refuse. `_private` also has
  different host/compiler arity behavior. Sharing a total validated literal or
  selector interface would change callers. The draft already retains those
  differences and excludes the worker correction.
- **R2:** TCC caches each symbol address, including failure, in its own
  `OnceLock`; Codegen and Image cache an acquired handle but call `api.dlsym`
  again for later symbol requests. A successful TCC acquisition retains its
  loader reference even if the symbol is missing. Sharing symbol caches or
  returning an automatically closed handle changes callback counts or lifetime.
  The draft's stateless seam and caller-specific ownership rules address this.
- **R4:** `reap_code_bindings` first snapshots candidates under the binding lock,
  calls floor retirement outside the lock, removes confirmed rows, then invokes
  GC. Wrapping this in a single owner method must not lengthen the lock scope or
  drop Unit ownership before floor confirmation. The draft requires an inventory
  before consolidation and explicitly preserves these distinctions.
- **R5:** `install_project_bridge` stores the address of `HostRuntime` in
  `PROJECT_BRIDGE_RUNTIME`; `Drop` clears only a matching address. Moving that
  value after installation can break both callback access and cleanup. The draft
  explicitly protects address identity and bridge clearing.
- **R6:** `RecoveryMaps::derive` is intentionally separate from `Template` so a
  boxed consumer need not pay virtual-map costs. Runtime preparation also has
  family and size gates. Reuse must not become eager preparation, change refusal
  thresholds or treat immutable compiler facts as live method/API authority.
  The draft already states those limits. Validate live mutation through native
  callbacks as well as source method replacement, as required by F1.
- **Evidence independence:** sharing a production helper with independent
  colored/uncolored interpreters could make wrong parallel-copy behavior agree
  on both sides of a comparison. The draft explicitly prohibits that sharing.
  Keep expected-byte and malformed-input cases independent of the new helpers.

## Scope and limitations

This was a read-only source/design review apart from this companion document.
I inspected the candidate implementations, Design 75, and relevant contracts
in 60A and 74/74I. I did not audit every failure exit or every authoritative
design in the ledger; those remain explicit per-slice obligations. I ran no
heavy tests, native experiments, benchmarks or fresh validation matrix, and
did not independently re-audit the baseline's 36 retained receipts.

The initial verdict requests document hardening for F1–F3. It does not request
source changes, approve an implementation batch, or claim that prose alone can
guarantee preservation of malleability.

## Revision review

I reread the revised proposal, including the follow-up metacircular and raw
pointer clarifications, against the source evidence above and 60A section 3.

| Finding | Disposition in revised Design 78 |
| --- | --- |
| F1 | Addressed in the native-participation and metacircular ledger rows and R4's handwritten-native, callback, no-replay and bounded-stack evidence requirements. The added action wording correctly preserves direct canonical decoding; it does not introduce arbitrary action dispatch. |
| F2 | Addressed in R3's raw-pointer lifetime, permitted aliasing, absence of new alignment/initialization/extent requirements, whole-result refusal behavior and final `written` ordering. Real-adapter overlap, callback and sentinel cases are required. |
| F3 | Addressed in R7's explicit distinction between aggregate and canonical-only admission, exclusion of a shared canonical-layout gate for the aggregate caller, and required accepted-noncanonical coverage. |

**Final verdict:** no unresolved material objection to retaining these as
bounded, invariant-preserving refactoring proposals with the stated gates.
The revised scope requires no change to malleability or the other inspected
project invariants. This verdict approves the document's constraints and
preservation obligations only. It does not authorize implementation, qualify
an implementation, or establish that every possible extraction is equivalent.

F1–F3 are closed as proposal omissions. Their production proof obligations
remain open until a selected implementation supplies the required evidence.
Companion-document local links and trailing whitespace were checked; no runtime
tests or benchmarks were run for this review.
