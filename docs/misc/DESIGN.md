# Birnam Language Design Goals

## Current Invariants

Birnam's current authority model is documented in
[Compiled Units Are Primary](../design/45-compiled-units.md): compatible
compiled dylibs are the primary executable layer of a live image; the fact
store is a durable data and reflection/meta API; recorded source supports
authoring, bootstrap, and explicit recovery. Any future work on persistence,
reflection, modules, or live reload must preserve that ordering.

## Implemented Architecture Sequence

The five interleaved executable-evolution and isolation slices are shipped:

```text
L1a — generation authority and image coherence
→ I1a — blocking single-worker isolate substrate
→ L1b — file-backed revision and image-baseline workflow
→ I1b — single-worker rolling replacement
→ L2  — blocking mailbox-safe live rollout
```

L1a established one current dispatch generation and coherent image recovery. I1a
provides clean process boot and bounded, blocking copied-value IPC. L1b adds the
file-backed authoring and saved-baseline workflow; I1b proves one health-checked
worker handoff. L2 adds between-request candidate activation and per-worker
receipts, never pool-wide atomicity or asynchronous completion. See
[design 49](../design/49-live-module-evolution.md) and
[design 48](../design/48-concurrency-reactors-and-isolates.md). The concrete
file-by-file execution plan and phase gates are
[design 50](../design/50-five-phase-live-modules-and-isolates-implementation-plan.md).

The next architecture choice is deliberately open. Initialization, image-native
source editing, migration, reactors/pools, richer supervision, and structured
operations remain later slices selected from actual product evidence.

## Proposed Developer-Experience Sequence

The next product-facing sequence is deliberately orthogonal to those runtime
follow-ups. It makes the shipped compiled-unit and isolate architecture usable
as an ordinary project workflow:

~~~text
51 — project manifest and dependency resolution
→ 52 — project check, build, and run
→ 53 — project-aware LSP and Emacs integration
→ 54 — relocatable bundles and runtime distribution
~~~

The source resolver, materialized `check`/`build` workflow, and first
project-oriented `run` slice are now implemented. Editor integration and bundle
relocation remain proposals. These slices share one project resolver, preserve
declared Module named:requires: identity, keep resolution read-only, and leave
runtime dispatch authority with compatible compiled units.
See [design 51](../design/51-project-manifests-and-dependency-resolution.md),
[design 52](../design/52-project-check-build-and-run.md), [design
53](../design/53-project-aware-language-tools.md), and [design
54](../design/54-relocatable-bundles.md).

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

- To implement Ian Piumarta's 'id' object model in Zig
- To use this to bootstrap a very fine-grained runtime consisting of a module loader, the object system, a compiler, a base-level smalltalk-like object language, and so on
- To grow that language piecemeal into a fully malleable programming language with distributed capabilities.

## Prior art

The repository at ~/dev/c/birnam contains many prior attempts.
