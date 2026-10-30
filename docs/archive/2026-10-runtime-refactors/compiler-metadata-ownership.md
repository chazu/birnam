# Compiler metadata ownership: inspection before durable state

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-16. Next slice after Bench/Test commit `5c70fe4c`, following
[optional Store](../../design/optional-store-and-relational.md).

## Subsequent owner migration

The [retained compilation record slice](retained-compilation-records.md) now moves
the durable metadata itself out of Store. The ledger and qualification below record
the preceding inspection-only boundary; they are historical where superseded.

## Boundary selected for the inspection-only slice

`Codegen moduleMetadataOf:` and `embeddedFilesOf:` are static observations used
by Host project indexing and asset discovery. Previously both called the metadata
preparation path: a fresh source String acquired a `compiledSourceMetadata` fact,
retaining it and its complete compiler inventory even after the caller dropped the
returned projection. Store keys compare source identities, so repeatedly reading
the same file into fresh Strings accumulates distinct entries.

The new inspection path first reads and validates already-published metadata. On
absence it parses through the current Lang receiver into operation-local compiler
data and returns the existing ordinary Vector projection. It does not publish a
cache or repair the compiled-admission state. Shapes, digests, asset ordering and
deduplication, nil behavior and existing diagnostics are retained. Parser and Store
callbacks remain ordinary sends; the claim is that inspection itself adds no facts,
not that arbitrary replaced callbacks cannot mutate the world.

Host constructs a fresh String for each static observation, so the old
identity-keyed cache did not provide reuse across those file reads. Repeated
inspection of the same unprepared source object can now parse again. A caller can retain
its returned projection. No String-keyed Map, native registry, subscription or
method-identity check is added. Source identities are not merged by equal bytes.
Published metadata is still read from Store; this does not complete Store-free
inspection or bootstrap. The compiler-produced artifact metadata format is unchanged.

## Dependency ledger

| Family / operation | Current authority and lifetime | This slice / next obligation |
| --- | --- | --- |
| Static module and asset projections | Caller's returned Vectors; copied compiler data during the operation | No publication on a cache miss; collect temporary source/results; preserve current parser/owner sends. |
| `sourceRevisionOf:` | Preparation path can publish metadata before returning the revision containing the original source identity | Retained. SourceLoader and candidate admission use it. Carry an explicit compilation record through those consumers before removing publication, avoiding extra bootstrap parses. |
| `analyzeSourceMetadata:` | Explicit preparation; `compiledSourceMetadata` fact on the source identity | Retained until a durable owner replaces it; do not silently turn this acknowledgement into an unrelated result shape. |
| Artifact compile and activation | Canonical metadata in artifact plus its published language representation | Retained. Activation validates supplied metadata and source digests without parsing or compiling adapters. |
| Candidate inventory | Compiler owns lowering; canonical programs and metadata cross the boundary | Retained. Audit candidate lifetime, failed preparation and selected implementation identity together. |
| Compiled metadata reads / Generation projection | Validated published metadata; no fallback to parsing | Retained. Missing or malformed data must still reject admission. |
| Source registry | Updated 2026-10-18: ordinary SourceRegistry in Globals owns a persistent revision/ordered-entry state; no native backing table | [Authored ownership](../../design/authored-source-registry.md) implements Id-only consumers, conditional source writes and early bootstrap. Reflection and aliases refer to actual owner state. |
| Image | SourceRegistryV2 transport references the ordinary restored owner; Store facts remain separate optional user data | Preserve owner/state aliases, source-free mapped startup and explicit native-code rebuild. Old inventory-only images must be recreated with the new runtime; no silent fact conversion. |
| Documentation inventory | Reads compilation-produced method sources through Codegen | Retained. Must remain usable without reconstructing ASTs during compiled startup. |

A global Map keyed by value-equal Strings would merge source identities. A strong
identity registry would still retain every inspected source unless given a real
lifecycle. Neither is an acceptable substitute for Store merely to claim removal.
A source/compilation record with owned text and metadata is a candidate for the
next durable slice, but its boundaries must be settled against the ledger above;
this slice introduces no wrapper and no permanent native service.

## Evidence and acceptance

The pre-change native test reproduces the smell: two batches of three fresh-source
observations add six facts and increase the post-GC object count from 19,492 to
19,690. The new regression measures raw allocation/fact counts after each operation
frame returns, exercises current parser replacement with collection, preserves
equal-text source identity, and checks missing/malformed compiled metadata boundaries.

Qualification passed against `/tmp/birnam-inspection/prefix`:

- Native source inspections at batch sizes 2, 0, 1, 10, 30 and 30 keep the raw Id
  allocation count at 19,492 and the fact count at 22 after collection; the Store
  assertion spy observes zero writes. Current parser callbacks collect safely.
- Two equal-text source objects remain distinct. Explicit preparation still
  publishes metadata; published queries work with parsing denied. Missing compiled
  metadata remains missing after inspection, and malformed published metadata is
  rejected without parsing or repair. Existing nil/malformed-source diagnostics pass.
- Source-loader golden digests, outputs, diagnostics and collecting/reentrant owner
  sends pass, along with candidate and durable artifact activation regressions.
- Installed startup and console operation pass with parsing and compilation disabled;
  missing/malformed metadata still fails without fallback. Host's project-observation
  test checks current message replacement and collection.
- Fresh-process artifact CLI acceptance passes with removed sources/assets, nested
  Globals, dependency ordering and preflight failures. The project CLI gate passes
  check/build/run/install/test, cache reuse/repair, source-independent execution,
  path dependencies and pinned Git dependencies in disposable fixtures.
- 327 Lean theorems, 88 rejected mutations and 203 expected TLC outcomes pass. The
  new laws distinguish local inspection results from publication/admission state;
  they are not a proof of the native compiler or arbitrary callback effects.

The [receipt](../../measurements/compiler-inspection-20261016.json) records provenance,
source hashes, commands and logs. The runtime implementation matches the isolated
build's source copy. This slice remains uncommitted. No user runtime installation,
release or push was performed; no CPU speedup or complete Store independence is
claimed. No temporal coordination protocol or new image representation is added.
