# Prioritized plan for simplifying the runtime and increasing Birnam implementation

Proposed work order dated 2026-10-30. Based on the
[runtime simplification assessment](runtime-simplification-assessment.md) at
`11e7a7e2123c616af4581b1bfc05a0f0cf7b9597`.

The objective is to improve performance, remove unnecessary mechanisms, and move
behavior into Birnam while preserving its malleability and independent source
bootstrap. Priorities below rank expected importance; later investigations are
not automatic dependencies of earlier work.

This plan does not authorize an agent to implement every item. Execute only the
scope assigned by the user, incorporating later user direction. Explicit user
decisions and [INVARIANTS.md](../../INVARIANTS.md) govern the work. Recheck the
current code before acting on findings from the assessment.

## Documentation work completed

Priorities 1 and 2 were completed as a documentation-only slice on 2026-10-30:
the active roadmap and NativeBackend reference were reconciled with source, their
prior versions were archived, and the [implementation guide](../implementation-guide.md)
was linked from agent guidance and the documentation indexes. The guide applies
the task brief and execution rules below without duplicating them.

Verification covers the changed active links and anchors, the NativeBackend method
table and relevant behavior, archive preservation, and whitespace. No runtime code
changed and no runtime tests or performance measurements were run for this slice.
Priorities 3–7 remain proposed work; the assessment retains its recorded baseline.

## Priority 1 Repair active architectural guidance

**Purpose:** prevent obsolete implementation instructions from shaping new work.

Reconcile the active self-hosting roadmap with the current invariants and source.
Move dated `IdApi` extraction instructions and progress reports into historical
material. Retain a concise active statement of the desired outcome, verified
obstacles, open decisions, and candidate experiments. Preserve explicit user
requirements such as independent bootstrap; do not treat a documentation cleanup
as permission to weaken them.

Check NativeBackend's documented selectors and Execution links against the
implementation. Correct obsolete API listings and missing anchors. Review only
the directly implicated documents and their indexes in this slice.

**Completion:** active guidance has no imperative dependency on retired designs;
the affected message references match source; local links resolve; history remains
available and clearly historical.

**Scope limit:** no runtime edits, language redesign, blanket archive rewrite,
new documentation platform, or restoration of code merely to satisfy stale prose.
If prose and source differ on a genuine language requirement, identify that
specific conflict rather than assuming either is correct.

## Priority 2 Establish concrete implementation examples and task discipline

**Purpose:** give agents a positive model of simple implementation and a bounded
way to work.

Add a short implementation guide using the existing glossary. Explain acceptable
private Rust helpers, temporary call roots, callback/reentry handling, and the
distinction between semantic information and Rust storage choices. Include a few
reviewed examples: direct Method entry, bulk byte handling, and an authored-policy
migration. Link to existing contracts instead of duplicating them.

Use the task brief and execution rules below. Add only a short pointer from agent
guidance if needed; do not accumulate a second competing set of invariants.

**Completion:** an agent can locate a relevant example, explain the behavior in
Birnam terms, and prepare a bounded task brief without inventing ownership or
borrowing concepts.

**Scope limit:** no ban on Rust features, mandatory service decomposition,
generalized adapter framework, or requirement for user approval of routine
implementation choices. A small useful guard need not be removed to match a style.

## Priority 3 Complete one small direct migration into Birnam

**Purpose:** demonstrate a repeatable migration that deletes production native
work rather than adding an authored wrapper.

**Current status: prototype investigated; production migration not delivered.**
The [candidate comparison](first-migration-comparison.md) selected method-source
inventory as a bounded part of metadata analysis. The production Rust collector
and its callers remain unchanged. Full metadata analysis and isolated literal
lowering still have the larger integration costs recorded in that comparison.

The [current inventory prototype](../measurements/inventory-bulk-20261031/README.md)
uses bulk child traversal and a Word for its private depth counter. Its initializer
now runs before Globals exists. Selected fixtures, substitute-CST behavior,
precise GC, actual deep chains and early seed bootstrap pass in resumable mode.
Synchronous compilation overflows the native stack on a 1024-ancestor chain;
its faster timings are diagnostic evidence, not a production execution choice.
The resumable prototype still takes about 353 ms and 839 ms for dense and real-module
fixtures, versus about 10 ms and 30 ms for complete native metadata analysis.
The bulk-versus-indexed CST protocol distinction also remains explicit. Neither
performance nor full production integration is qualified for cutover.

Runtime work has removed [deferred synchronous setup](../measurements/method-source-investigation-20261030/README.md),
[separate control-context Sequences](../measurements/inline-control-20261030/README.md),
and [redundant native argument buffers](../measurements/native-result-20261031/README.md).
The [image admission fix](../measurements/image-admission-20261031/README.md) closes
the previously recorded image validation failure. These are bounded improvements,
not completion of the migration or overall simplification goal.

Return the performance focus to [arithmetic and generated control work](../measurements/arithmetic-layers-20261031/README.md):
full Integer addition still measures about 44 microseconds and 49 managed allocations.
The CPU investigation rejected a checked-Method-data refactor after it failed to
improve performance. Further inventory cutover work needs a credible performance
path, rather than more qualification of the same slow implementation. The separate
[ordinary-native-return proposal](../measurements/native-result-20261031/README.md)
remains unapplied pending the user's independent-root-drive decision; measurements
and unrelated progress do not answer that question.

Choose one bounded compiler or policy operation after inspecting its callers and
bootstrap dependencies. Source metadata analysis or a small CST lowering operation
are candidates, not preselected requirements. Prefer behavior expressible through
existing Birnam facilities and measurable on a representative workload.

Record the behavior and before/after operation sequence before editing. Identify
which temporary Rust representations disappear and which information still matters.
Implement the authored operation, switch its normal callers, and remove the
replaced production path. Retain only demonstrated cold-build dependencies.

Establish the relevant baseline before changing the hot path. Compare outputs,
errors, effects, supported replacement, and relevant collection/reentry behavior.
Measure the operation and an affected end-to-end workload where practical; do not
infer application speed from a call-only benchmark.

**Completion:** the normal path demonstrably executes Birnam code; the old
production implementation or conversion path is removed; retained seed code has
an explicit dependency explanation; relevant checks pass; performance results and
limitations are recorded. A measured regression requires an explicit disposition,
not an unqualified performance-success claim.

**Scope limit:** no whole-compiler rewrite, new backend, collector redesign,
future thread-hosting work, or broad cleanup of neighboring modules. If the chosen
slice requires substantial new facilities, report that dependency and narrow or
reselect the slice within the assigned objective.

## Priority 4 Audit Execution lifetime bookkeeping

**Purpose:** determine whether existing references and states can replace redundant
registries, leases, or counters without compromising running or suspended code.

Trace one Method through construction, installation, capture, invocation,
suspension, replacement, collection, and final code release. Include callbacks,
failure, cancellation, and recovery where they affect retention or cleanup.

For each mechanism in ResumableEntry and ExecutionState, record its fact, writers,
readers, release condition, and the concrete failure prevented. Compare that fact
with existing object references and states. Distinguish executable storage from
metadata, object reachability, and temporary native execution requirements.

**Completion of the investigation:** every proposed deletion has a behavioral
argument, affected callers, relevant negative cases, and an estimate of the work
removed. State keep/reject rationale and uncertainty. Finding that a mechanism is
necessary is a valid result.

**Scope limit:** the audit itself does not authorize implementation. When a fix is
assigned, change one coherent lifetime path at a time. Do not remove bookkeeping
based on its name or port the whole engine as part of the investigation.

## Priority 5 Audit restrictions in the next selected migration path

**Purpose:** expose accidental representation requirements without broadening every
interface speculatively.

Examine exact-vtable checks, fixed representation assumptions, busy/reentry
rejections, and redundant validation in the selected path. Start with a concrete
question such as whether the source-metadata sequence reader requires Vector
identity or the documented sequence operations.

Identify whether each restriction follows from language behavior, a versioned
format, an actual storage-access requirement, or only current convenience.
Preserve required validation. Propose removal only with an explanation of the
supported behavior and what makes access valid afterward.

**Completion:** each examined restriction has a disposition with evidence; an
implemented removal includes the relevant substitute/replacement case and invalid
input checks without changing unrelated behavior.

**Scope limit:** no global type-check deletion, universal duck-typing mandate,
new capability system, or silent expansion of a format contract. This priority
can be included in a migration when that exact restriction blocks it.

## Priority 6 Extend only the migration approach that has worked

**Purpose:** gain sustained self-hosting progress without endless preparatory Rust
refactoring.

Use the completed pilot to select the next independent responsibility. Prefer
clear deletion opportunities, limited dependencies, and useful application
coverage. Stop at a coherent validated boundary between slices.

Track behavior implemented in Birnam, production native dependencies removed,
representations or conversions deleted, remaining cold-build seeds, and measured
performance. Separate production code, tests, generated output, and bootstrap
copies when reporting source counts.

**Completion:** each slice is independently reviewable and has its own delivery
evidence. Update current references and examples as the implementation changes.

**Scope limit:** no standing instruction to maximize deleted Rust lines, no
wrapper counted as migration, and no retrospective promotion of a successful
temporary technique into a language invariant.

## Priority 7 Reconsider a native language rewrite only with specific evidence

**Purpose:** make an implementation-language decision from demonstrated friction.

If bounded work still encounters substantial Rust-specific complexity, identify
one responsibility that exhibits it and propose an equivalent Odin, Zig, or C
experiment. Compare the same behavior, integration requirements, code complexity,
performance, and maintenance burden. Account for independent bootstrap and any
additional toolchain cost.

**Completion:** a decision explains which problem the alternative language solves,
which obligations remain, and whether the benefit justifies broader migration.

**Scope limit:** this plan does not authorize a rewrite or an experimental native
implementation. Choose and assign such an experiment explicitly. Do not translate
the whole runtime merely because a small syntax comparison looks favorable.

## Agent task brief

Before a core implementation change, fill in this compact brief in the task's
existing working context or review description. A separate permanent document is
not required for every slice.

```text
Objective: one responsibility and the intended resulting behavior.
Authority: relevant user decisions and invariant/contract sections.
Current path: the actual sequence of operations and retained information.
Proposed path: the resulting sequence, including what is deleted.
Scope: intended modules and any necessary directly affected callers.
Exclusions: adjacent work that is not part of this task.
Mechanisms: additions or removals and the behavior each preserves.
Bootstrap: production path and any genuine cold-build dependency.
Evidence: focused tests, relevant integration checks, performance workload.
Finish: the observable state that completes this slice.
```

The brief explains an authorized task; it is not an automatic permission request.
Proceed with routine choices consistent with it. Keep it current if an authorized
change of direction alters the final implementation.

## Agent execution rules

1. Read the current invariants, glossary, affected contracts, and code. Use history
   for specific evidence, never as an implicit queue or design authority.
2. Implement the smallest complete behavior assigned. Necessary caller changes
   belong with it; opportunistic cleanup and unrelated facilities do not.
3. Keep implementation responsibilities inside the implementation. Do not add
   language ownership, borrowing, or memory-management obligations to satisfy
   Rust. Preserve actual memory and code validity through callbacks and suspension.
4. Justify new mechanisms by required behavior. Crate boundaries, a borrow-checker
   error, a historical pattern, or hypothetical future use are insufficient reasons
   for a new service, registry, wrapper, copy, or protocol.
5. Preserve malleability at the offered operation. Do not replace messages with
   hidden foreign-payload access, insert an application lookup after Method
   selection, or implement bulk work through per-byte messages.
6. Treat constraints and tests according to their meaning. Do not weaken a
   behavioral test or rewrite a requirement to excuse a violation. If an internal
   test becomes obsolete, explain why and retain coverage of the real behavior.
7. Preserve unrelated dirty and untracked work. Use the user's commit and
   integration instructions. Do not publish, install, or rewrite repository history
   merely because the implementation task is complete.
8. Report discovered adjacent issues separately and continue independent work in
   scope. Do not silently expand the task or repeatedly ask about routine choices.

## When an agent must pause dependent work

Surface the specific conflict before proceeding when the proposed change would
alter an agreed language requirement, remove a required behavior, depend on an
open invariant violation, or require work materially outside the assigned scope.
Explain the smallest affected decision and the available alternatives. Continue
unblocked work that remains within the assignment.

A longer-than-expected implementation, an awkward Rust representation, or an
archived design disagreement is not by itself a language conflict. Conversely,
passing tests does not authorize changed lookup, identity, replacement, recovery,
or bootstrap semantics.

If performance regresses, investigate and report the scope and magnitude. Do not
conceal it by changing workloads or thresholds. Apply the task's agreed acceptance
criteria; if none resolve the tradeoff, present that concrete decision before
claiming the slice complete.

## Review and completion evidence

Review each core slice along separate dimensions:

| Dimension | Required explanation |
| --- | --- |
| Behavior | Results, errors, effects, identity, replacement, and relevant execution/collection behavior remain correct. |
| Architecture | The operation follows the invariants; added mechanisms have concrete reasons; prohibited alternatives were considered. |
| Performance | Baseline and changed revision, build, workload, timing scope, results, and material limitations. Say when performance was not measured. |
| Migration | Which normal path now runs in Birnam, which native implementation was removed, and which seed dependencies remain. |
| Delivery | Checks actually run, their results, repository state, and installation/publication state when part of the assignment. |

Use [the validation guide](../validation.md) to select checks. A documentation
slice needs factual and link verification; a core runtime slice needs the relevant
behavioral and integration coverage. Run broader gates where the affected paths
require them, without treating unrelated repeated tests as additional proof.

Examples of negative cases include Method construction without dictionary edits,
ordinary dispatch without a second application lookup, whole-buffer access without
scalar fallback, and replacement without invalidating captured or suspended code.
Choose cases relevant to the slice rather than imposing every case on every edit.

Finish with the concrete change, removed work, validation, measured costs, and
remaining gaps. Mark investigation, proposal, implementation, and delivered
integration accurately. Completing an audit or writing a plan does not complete
the corresponding runtime change.
