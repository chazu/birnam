# Implementing Birnam behavior simply

Use this guide to explain and change one responsibility without turning native
implementation choices into language semantics. It applies the existing
[invariants](../INVARIANTS.md), [glossary](../glossary.md), and
[runtime contract](contracts/runtime.md); those sources and explicit user
decisions govern the work. The examples describe current code, not templates
that every future implementation must reproduce.

## Define the assigned operation

Before a core change, use the compact
[task brief](design/runtime-simplification-plan.md#agent-task-brief): name the
behavior, show the current and proposed operation sequences, identify what is
removed or added, and select relevant checks. Keep the brief in the task or review
description; a new permanent design document is not required for every edit.

Follow the plan's [execution rules](design/runtime-simplification-plan.md#agent-execution-rules)
and [conflict guidance](design/runtime-simplification-plan.md#when-an-agent-must-pause-dependent-work).
Proceed autonomously with routine choices within the assignment. An adjacent
opportunity is a separate finding, not an automatic extension of the task.

## Native implementation choices

Prefer direct records, functions, loops, and explicit state transitions. A native
method may reuse a parser routine, hash algorithm, or byte-copy helper. Reusing
algorithm code does not authorize hidden access to another Birnam object's state
or a bypass of its replaceable behavior. Crate boundaries do not imply services.

Rust lifetimes, references, collections, and cleanup guards can be useful private
tools. Explain the responsibility in concrete terms: retain an object reference,
keep code mapped, release a buffer, or finish a frame transfer. Do not translate
these mechanisms into Birnam ownership types or borrowing rules, and do not remove
them solely because they use Rust features.

A send can execute replaced code, collect, or reenter. Identify the references and
native storage accesses that must remain valid across it. End an incompatible
temporary access before calling out, or choose a representation that supports
the required behavior. Rust's unsafe code must still obey Rust's memory rules.
Neither a raw pointer nor a blanket copy makes the problem disappear.

Id's [call Scope](../rust/birnam-id/src/id/native_id.rs) is a useful small example:
it records the receiver, selector, arguments, selected Method, context, and code
owner needed during the call. Its cleanup removes the temporary record. The
synchronous argument area is valid for that call; suspended execution retains
its needed values separately. This does not give Birnam programs a borrowed-value
type or require one allocation for every retained reference.

The compiler's [Expr representation](../rust/birnam-codegen/src/lib.rs) illustrates
a different distinction. Source spelling, escaping, and literal contents can be
meaningful information; borrowed versus owned bytes explains Rust storage.
Preserve the former when porting, and let ordinary Birnam references retain values
where appropriate. Do not mechanically create an OwnedString/BorrowedString pair.

## Example 1 Direct Method entry

**Behavior:** ordinary sends use current lookup and enter the selected Method.
An entered or captured Method retains its meaning; later sends see replacements.

The [historical correction](archive/2026-10-runtime-refactors/id-send-correction-20261023.md)
records this change:

```text
Before: send -> receiver method lookup -> application lookup on selected Method
        -> application dispatch -> implementation
After:  send -> current method lookup -> enter selected implementation
```

The current path is in [id_dispatch.rs](../rust/birnam-id/src/id/id_dispatch.rs)
and [bootstrap_values.rs](../rust/birnam-id/src/id/bootstrap_values.rs). The
bootstrap Method implementation reads its own declaration, context, and code
owner. Generic dispatch does not acquire a catalog of other objects' payloads.
Custom lookup and doesNotUnderstand handling remain part of the real send path.

The removed second lookup and its forwarding machinery did no required language
work. Keeping call references valid still does required work. Explicit invocation
of a captured Method remains separate from an implicit application protocol.

**Evidence to consult:** the `dispatch-contract` case in
[oop_slice.rs](../rust/birnam-siphon/examples/oop_slice.rs) makes lookup on the
selected Method fail while an ordinary send still succeeds. The
[Oop core script](../scripts/oop-id-slice-test.sh) runs that negative case.
This tests the prohibited operation, not merely the spelling `apply`.

## Example 2 Whole buffers as whole values

**Behavior:** consumers obtain a complete byte value through the offered bulk
operation while retaining data validity through callbacks and collection.

```text
Before: read size -> send indexed read once for every byte -> assemble output
After:  read size -> request range through bulk message -> receive groups
        -> validate complete result
```

The [byte contract](contracts/bytes.md) defines the current provider and destination
messages. [byte_transfer.rs](../rust/birnam-abi/src/byte_transfer.rs) implements
native helpers. Providers interpret their own bytes and snapshot the requested
range; consumers do not inspect foreign payloads. Indexed access remains available
for genuinely indexed work, with no automatic scalar fallback for whole buffers.

Grouping removes per-byte sends while preserving the operation's replacement
behavior. The current 8192-Oop group size, temporary destination, native buffers,
and callback declarations are implementation choices. Do not infer a new Id
carrier, zero-copy entitlement, or mandatory use of these exact helpers from this
example. Evaluate further copies or bookkeeping against the operation and its cost.

**Evidence to consult:** [bulk_bytes.rs](../rust/birnam-siphon/tests/bulk_bytes.rs)
checks scalar-access avoidance, sizes across a group boundary, nested reads,
collection/reentry, invalid or incomplete output, and retained destinations after
a read ends. These tests require the runtime setup identified by the test and
[validation guide](validation.md); an ordinary Cargo run can skip them.

## Example 3 Source registry policy in Birnam

**Behavior:** preserve ordered compilation records, original source identity,
duplicate-registration behavior, and explicit conditional replacement. Source
registration does not itself install Methods or make ordinary edits depend on
Store or certification.

The [earlier source-registry assessment](archive/2026-10-runtime-refactors/source-registry-id-boundary.md)
records the former native table. Its proposed compensation machinery is historical;
the current [runtime contract](contracts/runtime.md#editing-and-explicit-undo)
governs explicit undo and partial effects.

```text
Before: consumer -> native registry implementation -> private records/counters
After:  consumer -> current Globals binding -> SourceRegistry message
        -> ordinary state and collection operations
```

[source-registry.birnam](../modules/source-registry.birnam) holds the state in one
ordinary slot and implements registration, lookup, and replacement. Codegen's
[source helpers](../rust/birnam-codegen-runtime/src/source.rs) resolve the binding
and send messages; they do not maintain a second registry. Native metadata
analysis remains native, so this example does not claim compiler self-hosting.

The authored policy preserves original source references, uses current String
equality for duplicate text, and uses existing conditional slot replacement for
conflicting edits. Task-transition policy and conditional publication serve
different purposes: a task transition alone does not prevent synchronous callback
reentry. Neither makes an arbitrary cross-object operation transactional or
authorizes replay of callback effects.

The [builder](../rust/birnam-build/src/main.rs) includes the early authored source
registry before named module admission. There is no need to reproduce the old
native registry merely because bootstrap needs the registry early. Every new port
must independently establish its own clean-build dependency closure.

**Evidence to consult:** [source-registry-test.birnam](../modules/source-registry-test.birnam)
covers identity, declaration order, clones, reflection, conditional conflicts,
revision changes, and retained snapshots. The code-generation
[image path](../rust/birnam-codegen-runtime/src/image.rs) retains and validates
the ordinary registry object. Image and callback changes require their relevant
integration checks in addition to these language tests.

## Review the result

Explain why each new mechanism is necessary and what work was removed. Review
architecture separately from correctness, and performance separately from both.
Do not change language requirements or weaken behavioral tests to hide a violation.
When an internal test becomes obsolete, identify its incidental assumption and
retain coverage of the underlying behavior.

Use the plan's [completion evidence](design/runtime-simplification-plan.md#review-and-completion-evidence)
and the [validation guide](validation.md). Source inspection, a passed focused
test, a complete integration run, a benchmark, and an installed product establish
different facts. These examples identify code and checks; they are not fresh test
receipts or a requirement to run every listed test for every task.
