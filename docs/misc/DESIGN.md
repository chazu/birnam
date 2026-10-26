# Birnam Language Design Goals

Current-direction note (2026-10-18): the historical architecture sections below
retain superseded Store/MOP, generation and recovery descriptions. For those
topics, use [Store and Relational as optional facilities](../design/optional-store-and-relational.md)
and the [generation review](../design/generation-post-simplification-review.md).
Ordinary edits require neither facts nor generation certification; basic live
image loading permits partial application. Id remains the interoperability
floor, and Rust is a current bootstrap dependency, not a permanent language-design
requirement. The [source-registry design check](../design/source-registry-id-boundary.md)
applies that boundary to the next proposed ownership migration.

## Current Invariants

### Malleability and a minimal Id floor

Malleability is a primary invariant. Preserve current message lookup, delegation,
replacement, reflection, and the supported execution/control protocols. A faster
or smaller implementation must not silently freeze their meaning. Entered calls
retain the code and objects they require; subsequent sends observe current lookup.

The payload of an ordinary object is opaque to minimal Id. Allocation and generic
dispatch must not contain a fixed catalog of payload types such as Name, Method,
Fault, or reference array, whether expressed as an enum, numeric tags, or parallel
per-type tables. Owners above that floor define their payload formats, validate
those formats, and report references through replaceable object protocols.
Bootstrap primitives may implement the initial object graph; they do not make
those initial representations the only representations the system can support.

Ordinary heap objects have contiguous allocations containing their header and
payload. Their owners may refer to other objects or separately allocated storage;
that choice does not authorize Id to interpret the contents. Moving an allocated
object must preserve its identity and all supported references. Calling `realloc`
on an escaped Oop without such a mechanism is not a valid resizing implementation.

Id's message receivers, selectors, arguments, and results are Oops. Native calling
conventions and object-file metadata must not introduce a second runtime value
system. Oop reference encoding and allocation facts are distinct from payload
meaning: recording an address, extent, identity or canonical reference does not
permit the allocator to decide the object's language type.

Id does not provide a universal method-change subscription. The user retired
`methodChangesTo:` on 2026-10-26 while retaining explicit Store invalidation.
Dictionary edits remain ordinary edits; consumers maintain or invalidate their
own descriptions through explicit operations. This does not remove Id's own
lookup invalidation or Store's authorizer and observer for explicit Store edits.

Use [the glossary](../../glossary.md) for terminology. These requirements govern
new work; older migration descriptions below are not permission to contradict them.

### One message dispatch protocol

The ordinary message path is:

```text
send(receiver, selector, arguments) -> current method lookup -> enter the selected method
```

An ordinary send must not require another selector lookup on the selected Method
before entering it. In particular, making Methods Oops does not authorize an
implicit `apply`, `invoke`, `call`, or renamed equivalent protocol on every send.
A private implementation helper that enters the selected method's calling
convention is not a second language-level dispatch protocol. Custom lookup and
explicit messages remain live under the existing send semantics.

`apply` and the argument/byte-forwarding family introduced with it are not part
of Id's design. Do not add them, preserve them as an alternative path, or rename
them. Use `send` and the objects' defined data operations. Payload opacity,
malleability, GC correctness and the dispatch contract must hold together.

### Architectural changes must preserve the contract

An implementation or optimization task authorizes work within these invariants.
Changing an invariant requires an explicit user decision about that change; broad
approval to continue implementation is not approval for a newly invented protocol.
If a proposed implementation appears to require a conflict, stop that affected
work, describe the conflict and consider simpler designs before building dependents.

Before changing a core path, state its existing contract and show the proposed
before/after sequence of operations. Identify any new required message, lookup,
representation, registry, allocation, or lifetime mechanism. Explain the necessary
responsibility it serves and whether existing machinery can serve it. This is a
check on design changes, not an approval requirement for routine private refactors.

Tests must include counterexamples to prohibited designs, not just examples that
exercise the implementation. The dispatch contract test checks that Id installs
no `apply` method and makes any lookup on the selected Method fail. An ordinary
send must still enter that Method successfully. This observes behavior regardless
of a helper's spelling. Establish expectations from the contract before repairing
an implementation; never change the contract merely to make existing code pass.

Passing implementation tests is not proof of architectural compliance. Agent-written
plans, migration logs, benchmark reports and prior code cannot amend these
invariants. When a violation is found, mark it prominently in the working design
and acceptance status, preserve historical receipts as evidence of their actual
scope, and stop extending the affected mechanism until corrected. Do not rewrite
the contract or its tests to make the current implementation appear compliant.

### Bootstrap and executable authority

Birnam must cold-bootstrap from a source checkout using its declared non-Birnam
implementation toolchains, with no prebuilt Birnam binary, image, compiled
module, or other Birnam artifact. The implementation language is not itself an
invariant: Design 61 declares Zig and Rust for the planned floor rewrite while
retaining the current Zig cold compiler until an equivalently proven
non-Birnam replacement exists.

Birnam's current authority model is documented in
[Compiled Units Are Primary](../design/45-compiled-units.md): compatible
compiled dylibs are the primary executable layer of a live image; the fact
store is a durable data and reflection/meta API; recorded source supports
authoring, bootstrap, and explicit recovery. Any future work on persistence,
reflection, modules, or live reload must preserve that ordering.

## Implemented Architecture Sequence

The retained executable-evolution and isolation slices are:

```text
L1a — generation authority and image coherence
→ I1a — blocking single-worker isolate substrate
```

L1a established one current dispatch generation and coherent image recovery. I1a
provides clean process boot and bounded, blocking copied-value IPC. The planned
file-backed source workflow, worker replacement, and cross-worker rollout were
withdrawn as unnecessary policy surface. See
[design 49](../archive/design/49-live-module-evolution.md) and
[design 48](../archive/design/48-concurrency-reactors-and-isolates.md) for the retained
boundaries and historical rationale.

The next architecture choice is deliberately open. Initialization, image-native
source editing, migration, reactors/pools, richer supervision, and structured
operations remain later slices selected from actual product evidence.

[Design 60](../design/60-stackless-native-execution.md) records the implemented
S1 machine substrate and S2 selected named/method-owned-block lowering and image
recovery for explicit per-fiber frames and resumable native methods. It
preserves the distinction between durable image state and in-flight execution;
suspension and continuations remain later stages.
Ordinary sends and Machine calls share one private Runtime resolve-and-pin
authority; the public IdApi and metacircular lookup semantics remain unchanged.
[Design 60I](../design/60i-stackless-artifact-byte-grammar.md) is the physical-
format authority for the stackless artifact family; its exact byte tables,
fixtures, and independent checker closed the S2 pre-implementation design gate
on 2026-08-22. The S1 shared-resolution substrate is aligned, so S2 is ready to
implement.

[Design 61](../archive/design/61-rust-libid-rewrite-plan.md) specifies a separate,
behavior-preserving rewrite of `libbirnam_id.dylib` in Rust. It keeps the current
synchronous ABI and higher Zig/Birnam rungs fixed, proves both floors through a
dual-backend verification window, and deliberately precedes any Design 60
execution-model change.

[Design 62](../design/62-rust-native-rungs-migration-plan.md) is the reviewed
continuation: migrate each higher native dylib, the independent cold compiler,
and the host to Rust while retaining existing Birnam policy seams. It preserves
one floor, dynamic rung boundaries, Object Canonical and the MOP law, compiled
unit authority, image v4, and the current synchronous stack model.

The Rust-native runtime record in [Design 62](../design/62-rust-native-rungs-migration-plan.md)
is the current execution
plan: convert the remaining Zig residue into Rust-owned production, native
Birnam policy, or explicitly isolated evidence/rollback fixtures. It preserves
the dynamic-only dependency manifest, one owner per runtime authority, the
Store/MOP consistency law, cold bootstrap, and reversible recovery boundaries.

## Implemented Developer-Experience Sequence

This developer-experience sequence is deliberately orthogonal to those runtime
follow-ups. It makes the shipped compiled-unit and isolate architecture usable
as an ordinary project workflow:

~~~text
51 — project manifest and dependency resolution
→ 52 — project check, build, and run
→ 53 — project-aware LSP and Emacs integration
→ 54 — relocatable bundles and runtime distribution
~~~

The source resolver, Birnam-owned project policy, materialized `check`/`build`
workflow, project-oriented `run` slice, project-aware LSP/Emacs integration, and
the executable-relative runtime distribution closure are implemented. Compiled
module artifacts now self-describe their identity, dependency/native closure,
target/ABI, capabilities, and source/revision identity through the ID floor;
installed project/application roots require no manifest or build receipt. These
slices share one project resolver, preserve declared `Module named:requires:`
identity, keep source resolution read-only, and leave runtime dispatch authority
with compatible compiled units.
See [design 51](../design/51-project-manifests-and-dependency-resolution.md),
[design 52](../design/52-project-check-build-and-run.md), [design
53](../design/53-project-aware-language-tools.md), and [design
54](../design/54-relocatable-bundles.md), plus [design 57](../design/57-project-policy-in-birnam.md).

## Proposed Interactive Programming Environment

[Design 58](../archive/design/58-terminal-repl-and-programming-environment.md) preserves
the retired Digla terminal-client experiment. Its useful lessons remain in the
project-aware LSP and the generic Alien-backed Termbox adapter; the application
and runtime-session protocol are no longer shipped.

## Active Product Sequence

Distribution Design 54 and the Design 52 project workflow are complete through
source-free roots, read-only compile diagnostics, independent profiles, and
isolated project tests. The next product work should exercise those interfaces
before adding another large subsystem:

~~~text
small applications as acceptance tests
→ select the next failure model from evidence
~~~

[Designs 55, 58, and 59](../archive/design/59-digla-adversarial-architecture-review.md)
remain as historical records of the retired Digla experiment.

## Abstractly
- To build a completely moldable programming environment, where everything is accessible to the programmer - like Piumarta's COLA
- To build up semantics gradually, like iolang
- To be metacircular but comprehensible
- To prefer late-bound behavior (message passing) while also allowing for the creation of binary artifacts
- To be able to load any C shared library, wrap it in an interface layer using a simple message passing model, and thereby absorb its power
- To be able to reconfigure itself into a new language at runtime, piecemeal, over time.
- To be able to hot-reload itself, from the ground up, or piece by piece, yet...
- To be able to produce a standalone, AOT-compiled application for users to use without the extra baggage of the fully malleable 'system'
- To carry within itself the full extent of itself, including source code, bytecode, machine code, and a canonical AST (De bruijn encoding) so that code can be truly universal

## Concretely

- To implement Ian Piumarta's 'id' object model as a small native floor behind a
  stable C ABI
- To use this to bootstrap a very fine-grained runtime consisting of a module loader, the object system, a compiler, a base-level smalltalk-like object language, and so on
- To grow that language piecemeal into a fully malleable programming language with distributed capabilities.

## Prior art

The repository at ~/dev/c/birnam contains many prior attempts.
