# Comparing candidates for the first bounded compiler migration

Source assessment dated 2026-10-30 at
`58fddef260750bc6a0a07e1b40e6d7f311f9e94f`.

**Initial recommendation:** move the method-source inventory algorithm into Birnam as the
first compiler migration. It collects the original source text of syntactic
literal `Reflect defineMethod:` declarations for documentation. This is a bounded
part of source metadata analysis, distinct from lowering method bodies, installing
Methods, or deciding which declarations execute.

**Feasibility result, 2026-10-30:** the early-bootstrap proof succeeded, but the
authored prototype failed the performance check in both default resumable and
synchronous compilation. The production migration is not proceeding on this
implementation. See the [experiment and replay instructions](../measurements/method-source-inventory-20261030/README.md).
The comparison below records why this candidate was selected before measurement;
it is not a recommendation to accept the measured slowdown.

Confidence is moderate: source inspection establishes the algorithm, its callers,
existing object operations, and a bootstrap dependency. It does not establish
runtime performance or prove the proposed bootstrap wiring. This comparison
completes candidate selection for [priority 3](runtime-simplification-plan.md#priority-3-complete-one-small-direct-migration-into-birnam),
not the migration itself. The [invariants](../../INVARIANTS.md) and
[implementation guide](../implementation-guide.md) govern implementation.

## Comparison

| Candidate | What must move with it | Assessment |
| --- | --- | --- |
| Full source metadata analysis | Module and method recognition, lowering, portable programs, implementation inventory, digests, retained-record conversion, and artifact/candidate integration | Too broad for a first slice. `Metadata::inspect` is not merely a few syntactic queries. |
| Public `moduleMetadataOf:` | A five-field result, but currently backed by full revision recognition and its lowering/inventory work | A small public result conceals substantial dependencies. Porting only its result assembly would leave the analysis native. |
| Embedded-file inventory | CST traversal, literal recognition and escape decoding; the traversal helper also recognizes native-library dependencies | Viable later, but either leave a second native walker or include both inventories and their artifact consumers. Keep it separate from actual file embedding and lexical-shadowing rules. |
| Artifact module-name recognition | CST traversal and literal Symbol/Vector recognition; last syntactic declaration wins | Small algorithm, but changes artifact identity and dependency extraction, and differs deliberately from strict source-revision admission. Higher integration risk than documentation inventory. |
| Method-source inventory | CST traversal and declaration recognition, returning verbatim source spans | Recommended. No Rust expression result, no literal decoding, no Method installation, and an existing documentation consumer. Bootstrap and all metadata producers still need integration. |
| Literal CST lowering | Wrapper/span recognition followed by Rust `Expr` construction, consumed recursively by the rest of lowering and portable encoding | Small function, poor isolated migration. A Birnam result converted back to `Expr` adds a new representation exchange while leaving the full CST projection and most lowering native. |

The principal distinction is the amount of behavior and representation conversion
needed to complete the operation. Source-line counts alone favor literal lowering
but hide its callers' dependency on Rust's expression tree.

## Why full metadata analysis is larger than expected

[Metadata::inspect](../../rust/birnam-codegen/src/source_metadata.rs) gathers
source digests, artifact module identity, revision information, method-source
texts, and embedded paths. Its
[revision inspection](../../rust/birnam-codegen/src/source_revision.rs) calls
`lower_program`, resolves method implementations, encodes portable bodies,
projects top-level Block bindings, and constructs implementation inventories and
multiple digests.

The public query [moduleMetadataOf:](../../rust/birnam-codegen-runtime/src/source.rs)
uses the strict revision result, not just the artifact's syntactic module-name
recognizer. Replacing it with a quick scan for `Module named:requires:` would
change which sources are accepted. The
[project source observer](../../rust/birnam-host-project/src/project/source.rs)
and authored SourceRegistry consume this operation.

[Literal lowering](../../rust/birnam-codegen/src/lib.rs) returns `Expr<'a>` and is
called inside recursive expression lowering. Normal source evaluation, emission,
artifact compilation, candidate planning, and revision inspection depend on that
lowering. A literal-only port would need a new conversion back to Rust expressions
or a larger change to downstream consumers. Neither is justified for this pilot.

## Selected behavior

The current [method_sources algorithm](../../rust/birnam-codegen/src/source_revision.rs)
walks the CST in depth-first source order and collects complete source spans for
these syntactic forms:

- `Reflect defineMethod:on:as:` with three arguments and a literal selector.
- `Reflect defineMethod:on:as:doc:` with four arguments and a literal selector.

It includes declarations inside initializer blocks and preserves their exact
source bytes and occurrence order. It does not deduplicate declarations, execute
them, resolve lexical shadowing, validate their eventual runtime installation, or
interpret quoted examples as declarations. Dynamic selector expressions are not
literal declarations. Strict revision recognition remains a separate operation.

Wrapper handling, byte-span validation, malformed CST results, and current depth
limits need explicit cases during implementation. Existing limits describe the
present implementation; their existence does not establish universal language
limits. Do not silently widen or weaken them while porting this operation.

The output feeds `Metadata.method_sources`, its existing binary and ordinary-value
encodings, and ultimately `Codegen recordedMethodSources` and
[Doc](../../modules/doc.birnam). Keep those encodings and consumer interfaces.

## Current and proposed operation sequences

Current fresh-source path:

```text
obtain source bytes -> current Lang parse: -> CST objects
  -> copy/project complete CST into Rust OwnedCst
  -> Metadata::inspect -> Rust method_sources traversal
  -> native lists of source bytes -> retained metadata / artifact encoding
  -> recordedMethodSources -> documentation
```

Proposed path for this one inventory:

```text
obtain source and current parsed CST
  -> current compiler message -> Birnam declaration traversal
  -> ordered ordinary source Strings
  -> existing metadata assembly / encoding -> documentation
```

Keep the parsed CST available to the authored operation at the existing metadata
producer. Do not parse twice or rebuild another CST from Rust data merely to call
Birnam. Other metadata analysis and lowering still need `OwnedCst`; this slice
does not claim to remove that representation from the compiler.

Use existing CST messages (`ruleName`, `start`, `end`, `childCount`, `childAt:` or
`children`), String byte-range operations, and ordinary collections. Verify their
actual implementations and early availability. A whole source span is a String
operation, not a reason to copy one byte per send.

An authored operation on the existing compiler object is sufficient in principle;
no new service object, registry, or Id primitive is needed. Its exact selector and
internal method organization remain routine implementation choices. The call
must use current lookup and preserve valid references across callbacks and GC.
Account for observable parser/accessor behavior and snapshot consistency when
placing that call among the existing projection and analysis steps.

## Complete production call coverage

There are three current `Metadata::inspect` call sites in the runtime:

| Caller | Responsibility and required treatment |
| --- | --- |
| [source_metadata::inspect_fresh](../../rust/birnam-codegen-runtime/src/source_metadata.rs) | Fresh explicit inspection/preparation; supply the authored inventory without registering the source. |
| [artifact compilation](../../rust/birnam-codegen-runtime/src/artifact.rs) | Persist the same inventory in newly compiled artifacts, including early authored artifacts. |
| [candidate inventory](../../rust/birnam-codegen-runtime/src/candidate_inventory.rs) | Compare newly observed metadata with retained metadata; use the same authored inventory so the two paths agree. |

Change metadata assembly to accept the computed inventory, rather than computing
it again with the Rust walker. Remove that walker's production use across all
three callers. Its wrapper/span/keyword helpers have other consumers and cannot
all be counted as deletions.

Retained-record reads, compatible artifact loading, and image restoration must
continue to use saved metadata without reparsing source. The existing native codec
can still transport the method-source list; this is a compiler-algorithm migration,
not a codec or whole-metadata migration.

## Bootstrap dependency and first implementation checkpoint

The [builder](../../rust/birnam-build/src/main.rs) first builds native compiler
components with the Lang seed, then compiles early authored artifacts. Artifact
compilation invokes `Metadata::inspect`. The
[bootstrap loader](../../rust/birnam-bootstrap/src/lib.rs) loads native Codegen
before the early authored artifacts and the production Lang implementation.

Consequently, an authored inventory collector cannot simply be installed late and
assumed available while compiling itself and its prerequisites. The current Lang
seed supplies grammar construction, not this inventory algorithm. No existing
compiler-inventory seed switch was found in the relevant Cargo manifests.

The first implementation checkpoint is an explicit route for that initial
compilation and early installation. Prefer testing the existing
[Codegen compile: path](../../rust/birnam-codegen-runtime/src/lib.rs) first:
`compile_source` calls `emit_source`, native compilation, and Unit initialization,
without `Metadata::inspect`. Explicitly executing the resulting initializer could
install the authored collector before artifact compilation needs its metadata.
The build-only Lang seed already supplies parsing at this stage. Early method,
value, and execution dependencies still need a cold-process proof; source inspection
alone does not show that this initialization sequence works.

If that route cannot meet the behavior directly, a bounded build-only native
inventory seed is acceptable only with demonstrated cold-build consumers and
exclusion from the installed production path. Do not add one before establishing
why the existing compiler route is insufficient.

If feature-gating a seed inside Codegen is chosen, account for Cargo feature
unification and every shipped dylib depending on the shared compiler crate.
Today's installer rebuilds Bootstrap without its grammar-seed feature; that alone
would not strip a new Codegen seed. Do not add a silent missing-method fallback or
leave a native collector under an authored forwarding method.

Choose the smallest working construction route. This checkpoint does not authorize
a new bootstrap architecture, a universal seed service, or an Id rewrite.

## Bounded implementation brief

**Objective:** execute method-source recognition in Birnam for all fresh metadata
producers and remove the displaced native production algorithm.

**Scope:** the authored compiler operation, the three metadata producers, required
CST retention and metadata-assembly changes, build/bootstrap wiring, and focused
tests. Preserve current source-registry behavior, formats, and public queries.

**Exclusions:** full lowering, all metadata analysis, literal dependency inventory,
native emission, Execution lifetime bookkeeping, representation-check policy,
general asynchronous compilation, or changes to source-language semantics.

**Expected benefit:** a complete authored compiler responsibility and a small
native algorithm removed. It establishes an integration pattern for later work.
It does not by itself eliminate the full CST copy or promise faster compilation.

**Cost to measure:** extra message traversal, String slicing, temporary collections,
and any remaining conversion into native metadata. These may outweigh the small
native loop removed. Do not introduce caching, another tree representation, or
native per-node shortcuts to conceal a regression.

**Finish:** ordinary source inspection, artifact compilation, and candidate
inspection all use the authored collector; compatible saved metadata still loads
without parsing; independent cold build and installed startup succeed; any native
seed is explicitly confined to its build role; relevant checks and measurements
are reported. Completing only the authored walker does not complete the slice.

## Validation and measurement plan

Use [source_loader.rs](../../rust/birnam-siphon/tests/source_loader.rs) for existing
nested-definition, quoted-example, dynamic-selector, and documentation cases.
[source_inspection.rs](../../rust/birnam-siphon/tests/source_inspection.rs) covers
current parser/accessor use, source identity, temporary retention, no implicit
publication, and retained records that must not reparse.

Add focused cases for both accepted selectors, exact byte spans and ordering,
duplicates, nested and shadowed syntactic receivers, malformed/deep CSTs, and
collecting or failing replacements. Show that replacing the authored operation
affects all three producers. Show that installed execution does not call the old
collector. Preserve native failure versus ordinary Error conventions at the
actual message boundary; do not invent a different error protocol for the port.

Exercise artifact roundtrips, candidate metadata comparison, source-free image
startup with parsing disabled, documentation output, and a clean-prefix build.
Select the relevant broader checks using [validation.md](../validation.md).

Before editing, measure one already-parsed inventory workload and an end-to-end
metadata/artifact workload with sparse and dense declarations, nested blocks,
and realistic module sizes. Record revision, build, warm-up, iteration counts,
timing boundaries, and repeated samples. Keep startup separate from the timed
operation where possible. Compare the same inputs afterward and report any
regression explicitly. No benchmark or runtime qualification was performed at
this source-only comparison checkpoint; the subsequent experiment is linked above.
