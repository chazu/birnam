# Birnam Language Design Goals

## Current Invariants

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
[design 49](../design/49-live-module-evolution.md) and
[design 48](../design/48-concurrency-reactors-and-isolates.md) for the retained
boundaries and historical rationale.

The next architecture choice is deliberately open. Initialization, image-native
source editing, migration, reactors/pools, richer supervision, and structured
operations remain later slices selected from actual product evidence.

[Design 60](../design/60-stackless-native-execution.md) records a feasibility
assessment for another possible substrate: explicit per-fiber frames and
resumable native methods supporting trampolining and continuations. It recommends
a narrow machine/ABI proof before any roadmap commitment and preserves the
current distinction between durable image state and in-flight execution.

[Design 61](../design/61-rust-libid-rewrite-plan.md) specifies a separate,
behavior-preserving rewrite of `libbirnam_id.dylib` in Rust. It keeps the current
synchronous ABI and higher Zig/Birnam rungs fixed, proves both floors through a
dual-backend verification window, and deliberately precedes any Design 60
execution-model change.

[Design 62](../design/62-rust-native-rungs-migration-plan.md) is the reviewed
continuation: migrate each higher native dylib, the independent cold compiler,
and the host to Rust while retaining existing Birnam policy seams. It preserves
one floor, dynamic rung boundaries, Object Canonical and the MOP law, compiled
unit authority, image v4, and the current synchronous stack model.

[Design 68](../design/68-remaining-zig-conversion-plan.md) is the next execution
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

[Design 58](../design/58-terminal-repl-and-programming-environment.md) preserves
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

[Designs 55, 58, and 59](../design/59-digla-adversarial-architecture-review.md)
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
