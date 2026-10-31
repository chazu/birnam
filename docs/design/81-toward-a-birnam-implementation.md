# 81 — Toward a Birnam implementation

Birnam's long-term objective is a compiler, libraries, object runtime, and
execution machinery implemented in Birnam, with a small documented platform
interface. Preserve its malleability, performance, and independent source
bootstrap while moving complete responsibilities out of the native implementation.

[INVARIANTS.md](../../INVARIANTS.md) governs the design.
[ARCHITECTURE.md](../../ARCHITECTURE.md) maps the current implementation.
This roadmap describes the target and open work; it does not prescribe a crate
layout, a sequence of native service extractions, or a new object protocol.
The [simplification plan](runtime-simplification-plan.md) ranks proposed next
steps and defines how to keep an assigned slice bounded.

## Target and distinct milestones

Distinguish three outcomes:

1. The compiler is written in Birnam and compiles itself and the shipped modules.
2. The normal compiler/runtime implementation is built from Birnam and brought up
   through a small loader, with declared platform and external-library dependencies.
3. The independent source-bootstrap implementation is reduced or replaced, if
   subsequently chosen.

The first two do not require deleting genuine Rust/C cold-build seeds. A clean
source build must require no prebuilt Birnam executable, image, or module. Moving
an algorithm into Birnam and removing generated C/libtcc are separate milestones;
external platform/FFI libraries also require a separate decision.

### Installed system

The installed stage-1 executable should essentially be the siphon: it loads the
components that bring up the system and hands execution to them. Moving the same
Rust runtime into dylibs does not achieve the Birnam implementation target.

Supported compositions include compiled Birnam modules, an image, an image plus
modules, and these artifacts supplemented by source. Retained source, including
source in images and embedded resources, remains useful for inspection, editing,
and explicit rebuilding. Source compilation uses the compiler brought up through
that loaded system; it must not secretly invoke the Rust bootstrap compiler.
Compatible saved code remains executable content under the
[runtime contract](../contracts/runtime.md#compiled-artifacts-and-images).

Stage 0 may retain the independent native implementations needed to produce these
components. Stage 1 must not bring up its Birnam replacements by first executing
the superseded Rust semantic implementation and then continue with it underneath.
Trace initial bring-up, ordinary execution, source compilation, and image recovery
when claiming this target is met.

Id is the first semantic component. Its loaded Birnam implementation constructs
its initial object graph, storage, Symbols, interning, and dispatch using declared
platform operations. Platform memory services are allowed; a hidden Rust object
allocator, registry, or language runtime beneath Id is not the intended endpoint.
Collection and higher execution facilities remain separate responsibilities;
their eventual arrangement must follow the invariants rather than an archived
layer count or native API partition.

These are final-system requirements, not a demand to rewrite Id before moving a
useful compiler or policy operation into Birnam on the current runtime.

## Current implementation and remaining obstacles

The current system still uses Rust implementations of Id, collection, execution,
compiler algorithms, native emission, host operations, and image handling. It
therefore has not reached the installed-system target. The relevant source map
is in [ARCHITECTURE.md](../../ARCHITECTURE.md), not an old migration inventory.

Several responsibilities already have authored implementations: the
[Lang grammar](../../modules/lang.birnam),
[source registry](../../modules/source-registry.birnam),
[candidate policy](../../modules/candidate-policy.birnam), and substantial
[task/control policy](../../modules/stackless-control.birnam). Extend from what
exists instead of scheduling those responsibilities for replacement from scratch.
Thread-hosted domains and general asynchronous timer/I/O integration remain
future work; they are not prerequisites for an unrelated migration.

The [builder](../../rust/birnam-build/src/main.rs) compiles authored modules using
native components and a build-only Lang seed, then produces startup/tool images.
[Bootstrap](../../rust/birnam-bootstrap/src/lib.rs) establishes runtime load order.
A proposed port needs its actual dependency closure checked in both places.
A production wrapper over the same native algorithm is not a completed migration.

Three questions remain to be resolved for the relevant slices:

- Which compiler operations can run through existing Birnam objects with fewer
  conversions and acceptable cost? Native CST projection and lowering are
  candidates to investigate, not a mandatory replacement representation.
- Which facilities are actually missing for a Birnam-authored initial Id or
  another low-level kernel? Existing Word, byte, FFI, and compiler facilities
  do not by themselves prove that the complete construction path works.
- How can collection and executable-code validity be maintained during bring-up,
  callbacks, suspended execution, replacement, and image recovery? Describe the
  required references and operations without importing Rust borrowing into Birnam.

## Candidate work and open decisions

Begin with a bounded compiler or policy operation that current facilities can
express. Show the current and proposed operation sequences, identify removed
native work, and validate behavior and performance. The
[implementation guide](../implementation-guide.md) gives concrete examples and
links to the task discipline. Routine private changes need no extra approval.

For a separately assigned runtime experiment, choose a small kernel that exposes
an actual missing facility: for example, initial object construction and dispatch,
a parser matching loop, or a frame-storage operation. Establish the needed
platform calls and replacement behavior for that kernel before generalizing.
Do not add a native wrapper for every operation and count the wrappers as a port.

A self-sufficient Birnam Id experiment would need to construct initial objects
and vtables, install a Method, and send a message without Rust Id implementing
that path. It would establish only the exercised slice. Its existence would not
prove a full collector, compiler, image system, or installed runtime replacement.

Compiler self-hosting should compare successive stages and independent
semantic/format checks. Define which outputs must agree: incidental native
object-file metadata is not necessarily a useful fixed-point criterion.
Backend replacement should remain independently testable from frontend migration.

Choose additional low-level primitives, any direct-machine-code backend, and the
siphon's exact platform operations through concrete designs when their tasks
require them. This roadmap neither settles those interfaces nor authorizes a
wholesale native-language rewrite.

## Evidence for a completed migration

For each responsibility, demonstrate the authored implementation running on the
normal path, identify the displaced production implementation, and account for
any retained cold-build seed. A retained seed must have real clean-build consumers;
it must not be a renamed production fallback.

Preserve lookup and replacement effects, identity and aliases, error/effect order,
collection and callback behavior, stackless execution, scheduling, cancellation,
inspection, recovery, and live code validity wherever the slice affects them.
Use the [runtime contract](../contracts/runtime.md) and
[validation guide](../validation.md) to select checks; do not copy an old test
matrix without examining its scope.

Report correctness, architectural compliance, performance, and migration progress
separately. Count production native code, tests, generated output, third-party
code, and bootstrap copies separately. An installed-system claim also requires
source bootstrap, supported startup paths, source compilation, saved-image
recovery, and packaged execution evidence for the resulting product.

## Historical context

The [roadmap snapshot before the 2026-10-30 repair](../archive/2026-10-runtime-refactors/birnam-implementation-roadmap-20261030.md)
preserves earlier extraction sequences and progress reports. Those sequences,
API field counts, and capability arrangements describe their recorded revisions.
They are not prerequisites or instructions for new work.
