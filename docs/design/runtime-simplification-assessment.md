# Simplifying the native implementation and moving behavior into Birnam

Assessment dated 2026-10-30, based on checkout
`11e7a7e2123c616af4581b1bfc05a0f0cf7b9597`.

Birnam should remain radically simple and malleable while becoming faster and
increasingly implemented in Birnam. The recommended next move is to clarify and
simplify individual responsibilities in the existing implementation, then move
selected behavior directly into Birnam. A wholesale translation of Rust into
Odin, Zig, or C is not yet supported by evidence that the implementation language
is the principal obstacle.

This is a focused source assessment, not a completed correctness audit or a new
performance measurement. Its recommendations are developed in the separate
[prioritized action plan](runtime-simplification-plan.md). Both documents are
subordinate to [INVARIANTS.md](../../INVARIANTS.md) and explicit user decisions.
They do not establish additional language requirements or authorize execution
of every proposed step.

Documentation follow-up: the [plan](runtime-simplification-plan.md#documentation-work-completed)
records completion of priorities 1 and 2. The findings below retain their reviewed
baseline; the obsolete documents are linked to archived snapshots.

## The problem to solve

Three problems need separate treatment:

1. Rust representations can obscure the behavior a Birnam implementation needs
   to express.
2. Runtime mechanisms can accumulate unnecessary state, indirection, copies,
   validation, and bookkeeping.
3. Agents can mistake those mechanisms, historical designs, or incidental test
   expectations for language requirements.

Changing implementation language may reduce the first problem. It does not
automatically resolve the other two. A literal translation could preserve the
same registries, restrictions, and state machines in a different syntax.

Memory safety is a runtime responsibility compatible with malleability. A captured
Method must remain executable after its binding changes; aliases must remain
valid through collection; suspended work must retain the objects and code it
needs. These requirements make live modification usable. They do not require
ownership types, borrowing rules, or borrow checking in Birnam.

Rust's references and interior mutation must still obey Rust's implementation
rules, including inside unsafe code. The implementation must accommodate those
rules without exporting them as Birnam semantics. See the
[Rust reference on interior mutability](https://doc.rust-lang.org/stable/reference/interior-mutability.html).
Removing memory checks or replacing references with unchecked pointers is not,
by itself, a simplification or a migration strategy.

## Findings in the current repository

### Active documentation contains competing instructions

The [reviewed self-hosting roadmap](../archive/2026-10-runtime-refactors/birnam-implementation-roadmap-20261030.md) opens with a
warning that older extraction sequences are superseded. Its body nevertheless
instructs readers to prepare archived layer boundaries and begin with an old
`IdApi` partition. It also retains dated progress reports about earlier API
versions and capability arrangements.

The root guidance tells agents not to derive requirements from history, while
this active roadmap still presents parts of that history as instructions.
Removing the obsolete instructions from the active document would be more useful
than adding another warning above them. Preserve the historical narrative in the
archive and keep the active roadmap focused on the desired end state, verified
obstacles, unresolved decisions, and candidate experiments.

The [reviewed NativeBackend README](../archive/2026-10-runtime-refactors/native-backend-reference-20261030.md) also lists
`named:synchronous:` and `descriptorFor:` and describes
`tryPrepareCall:slots:selector:in:tail:`. In the reviewed checkout, searches of
NativeBackend and Execution found those names only in that README. Its link to
Execution's `guarded-generated-call-preparation` section has no matching section.
These references should be reconciled with the implementation before another
agent treats them as missing functionality to restore.

### Some Rust mechanisms are small and useful

Id's [call Scope](../../rust/birnam-id/src/id/native_id.rs) records references
needed during a call and removes its record on exit. Its lifetime annotation and
`Drop` implementation help express a local implementation responsibility.
Neither is a Birnam borrowing feature.

Mechanically deleting lifetimes, `Drop`, `Rc`, `Arc`, or `RefCell` would substitute
a style rule for architectural judgment. A local guard can be simpler and more
reliable than duplicating cleanup across every return path. Conversely, the
presence of a safe Rust interface does not prove that the behavior it exposes is
appropriate for Birnam.

### Compiler types mix semantic information with Rust storage choices

The compiler's [Expr representation](../../rust/birnam-codegen/src/lib.rs)
distinguishes source spellings, escaped literals, borrowed bytes, and owned bytes.
Original spelling and decoded contents can affect language behavior or artifact
contents. Borrowing versus owning bytes explains how the Rust implementation
keeps storage valid.

A Birnam implementation should preserve the meaningful information and use
ordinary references and collection to retain it. It should not introduce
BorrowedString and OwnedString concepts merely to reproduce Rust variants.

The current compiler also projects CST objects into an `OwnedCst` tree before
lowering them. A Birnam lowering implementation could potentially use the existing
CST messages and construct its output directly. That is a candidate for removing
an intermediate representation, not proof that the projection can be deleted
without accounting for traversal, replacement, snapshots, and performance.

### Execution warrants a targeted bookkeeping audit

Execution's [ResumableEntry and ExecutionState](../../rust/birnam-execution/src/execution.rs)
contain implementation references, lifecycle states, active-frame counts,
continuation counts, acquired-entry counts, generation leases, and registries.
Several mechanisms may protect different stages of executable lifetime. Their
number alone does not establish redundancy or a language violation.

The useful investigation is to trace one Method through construction, invocation,
suspension, replacement, collection, and final code release. For each field and
transition, identify the required fact it represents, the failure its removal
would permit, and whether another existing reference or state already represents
that fact. Only then propose deletion or consolidation.

The resulting design must keep captured and entered Methods usable, allow future
sends to see replacements, resume suspended work without replaying effects, and
reclaim executable resources when they are no longer required. Existing counters
are not themselves the acceptance criteria.

### Message use does not settle representation restrictions

The [source metadata sequence reader](../../rust/birnam-codegen-runtime/src/source_metadata.rs)
requires exact `VectorVt` before sending `size` and `at:`. This may enforce an
intentional record format, or it may impose an unnecessary nominal restriction.
The operation's actual contract and callers need investigation before changing it.

Audit such checks by asking which behavior or representation is required by the
operation. Do not remove format validation indiscriminately, and do not assume
that sending messages after a type check proves sufficient malleability.

## A simpler style for the remaining Rust

Prefer ordinary records, direct functions, explicit state transitions, and
private algorithms whose responsibilities can be explained without Rust-specific
terminology. Keep Oops for Birnam object references and messages for interaction
with other objects. Native helpers may share algorithm code without sharing
hidden Birnam state or becoming a parallel object system.

Keep Rust references to mutable runtime storage appropriately scoped around
callbacks and reentry. A send may invoke replaced behavior, collect, or reenter
the implementation. Do not solve an implementation borrow conflict by silently
forbidding valid Birnam behavior. A different representation or shorter access
period may be needed; copying everything is not the default answer either.

Each added registry, token, counter, state, copy, or dispatch should have a concrete
reason. Explain which required behavior it preserves and why a simpler operation
does not suffice. Use the existing operation's granularity: whole-buffer work
remains bulk work, and one native helper call does not imply a new service object.

This style should be demonstrated by a few reviewed examples rather than an
ever-growing list of banned Rust constructs.

## Moving responsibilities into Birnam

For each selected responsibility, describe its inputs, outputs, effects,
replacement behavior, retained information, and current operation sequence.
Then express the proposed Birnam sequence and identify the native implementation,
conversion path, or duplicate state that will disappear.

Simplify the Rust only as much as needed to understand and validate that slice.
Preparing the entire runtime for later translation risks becoming an indefinite
intermediate project. A useful first slice is a bounded compiler or policy
operation expressible through existing Birnam objects. Source metadata analysis
or a small lowering operation are candidates, subject to a dependency check.

A wrapper around the same native algorithm does not complete the migration.
Completion requires the normal path to execute the authored implementation and
the replaced production implementation to leave that dependency path. Any genuine
cold-bootstrap copy must be identified and justified separately. Preserve an
independent build from declared non-Birnam toolchains without a prebuilt Birnam
executable, image, or module.

Compare behavior, architectural simplicity, performance, and self-hosting progress
separately. A port can remove restrictions while becoming slower. A Rust cleanup
can become faster without moving any behavior into Birnam. Source line counts
support these assessments but do not replace them.

## Improving agent work

The existing [agent guidance](../../AGENTS.md), [invariants](../../INVARIANTS.md),
and [glossary](../../glossary.md) already express the desired direction. The next
improvement should reduce contradictory guidance and provide bounded assignments.

An assignment should identify one responsibility, the behavior to preserve,
allowed implementation scope, relevant validation, and the point at which the
agent should stop. Routine implementation choices remain autonomous. A change to
language requirements or an expansion into unrelated facilities must be surfaced
explicitly before dependent work proceeds.

Review architectural compliance independently of test results. Distinguish tests
of language behavior from tests of implementation arrangements. Changing an
internal test requires explaining which assumption became obsolete and how the
underlying behavior remains covered; it must not conceal a regression.

Provide worked examples of direct Method entry, bulk byte handling, and a
successful authored-policy migration. Explain their operation sequences and
removed mechanisms. Do not generalize their incidental layouts or temporary
techniques into requirements for later work.

## When to reconsider the implementation language

Odin, Zig, or C may be easier to maintain for particular low-level responsibilities.
Developer familiarity matters. Odin provides explicit allocator facilities, and
Zig leaves memory management to the programmer; neither automatically removes
the runtime's obligations around references, callbacks, suspension, and executable
code. See the [Odin overview](https://odin-lang.org/docs/overview/) and
[Zig language reference](https://ziglang.org/documentation/master/).

Reconsider a broader rewrite after a bounded experiment demonstrates persistent
friction caused specifically by Rust. Compare the same responsibility and behavior
in each implementation, including build/bootstrap integration, complexity,
performance, and maintenance cost. A more concise spelling of the same machinery
is insufficient evidence for rewriting the whole system.

The immediate priorities are to repair active guidance, complete one direct
Birnam migration, and separately investigate Execution's lifetime bookkeeping.
Those results will make a later implementation-language decision better grounded.
