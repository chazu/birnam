# Compiled Units Are Primary

> **Status:** current contract, 2026-07-10. This document records the authority
> model implemented by image format v3 and the next live-reload boundary. It
> refines [40 — The Image](40-the-image.md), [42 — Object Canonical](42-object-canonical-relational-derived.md),
> and [43 — Modules](43-modules-and-config.md).

## The authority order

Birnam is simultaneously live and native. Those are not competing modes:
compatible compiled dylibs are the **primary executable layer** of a running
image. They own installed dispatch and top-level runtime initialization. A
normal image boot maps those units and starts sending messages without
recompiling them.

The remaining layers have distinct supporting roles:

- The **active object environment** supplies live identity, dispatch, and data.
- The **fact store** is durable data plus an informal reflection/meta API. Its
  method facts mirror the dispatch dictionary; they do not override it.
- **Source** is retained for authoring, inspection, bootstrap, and explicit
  recovery. It is not the normal runtime authority for a compatible image.

This order prevents a subtle but damaging interpretation: the dylib embedded in
an image is not a cache that can be discarded in favor of facts or source. It is
the executable artifact the image saved.

## What Is Shipped

### Image v3

`src/id/image.zig` writes v3 images. Each `.code` record contains a checksum and
its relocatable dylib bytes. Loading preflights the complete record stream,
including every compiled-unit checksum, before it replaces store state or
materializes heap objects. v2 images remain readable; new saves emit v3.

A full save captures the primary unit before classifying data that depends on
it, then publishes the completed image with an atomic rename. A rejected save
does not truncate or replace the prior image.

### Primary Unit Contents

`[Codegen compileImageCode:]` compiles both:

- recorded module programs (`__modsrc__`), which run top-level initializers for
  named live objects such as parser grammars; and
- recorded method definitions (`__methods__`), which install compiled dispatch.

This is necessary for a compiled image to be self-consistent. Method sources
alone cannot recreate a module-owned object established by its top-level
program. Appended image generations that change either source category carry a
composing `.code` unit; later units win in stream order.

### Durable State Contract

Every durable value must be one of the following:

1. **Directly encodable** into facts and heap records.
2. **Source recoverable**, such as a top-level block with a recorded
   `__globalsrc__` definition.
3. **Compiled-unit recoverable**, such as a method-mirror fact, a code-owned
   global, or a module-owned global whose recorded module program initializes
   it.

Anything else rejects the save. It is never silently omitted from a published
image. `[System imageSaveReport]` provides the status, fact and slot counts,
captured code-unit count, and a diagnostic reason.

### Explicit Recovery

`--rebuild`, an ABI/architecture mismatch, or a data-only legacy image takes
the source-recovery path. The boot message says this explicitly, then replay
evaluates recorded module programs before method definitions. Alien replay and
source-backed-global derivation run after code resolution.

Recovery is a portability and repair mechanism. It does not redefine the
normal compatible-image boot path.

## Current Boundary

The runtime can compile and load native code live, images compose compiled
generations, and `CompiledUnit` provides a first-class staged dispatch lifecycle
for replacing one active generation while preserving an exact rollback point.
The transaction covers method additions, replacements, and explicit removals; it
does not pretend arbitrary initializer effects are reversible. Existing dylibs
remain loaded for process lifetime because Birnam cannot safely unload code while
closures or dispatch entries may still reference it.

The current platform and verification boundary is deliberately narrow:

- [arm64 macOS only](../misc/PLATFORM-SUPPORT.md);
- `zig build verify` is the required cross-layer gate; and
- the verification suite covers checksums, rejected-save preservation, mapped
  boot, source recovery, module initialization, and composing code generations.

## CompiledUnit Lifecycle

The concrete design is [47 — CompiledUnit Lifecycle](47-compiled-unit-lifecycle.md),
implemented after adversarial review. It scopes "transactional" to staged dispatch
generations (add/replace/remove), keeps arbitrary initializer effects outside the
claim, routes supported mutation through Store's MOP, and gives image composition
an honest reflected executable generation.

The implemented vertical slice:

1. compile a named candidate unit without changing active dispatch;
2. validate its metadata and load it as a candidate;
3. atomically activate its method bindings as one generation;
4. retain the prior unit as the rollback generation; and
5. expose active units and activation history through the reflection/meta API.

The implementation retains old dylibs indefinitely. Safe unloading
requires a later proof that no method, block, callback, or object slot can reach
their code. Correct live replacement and observable provenance come first.

The verification suite covers named-unit reload, failed-candidate inertness,
supersession and rollback, managed drift, and compatible/rebuilt image boot. See
the [user guide](../guide/05-compiled-units.md) for the supported API.
