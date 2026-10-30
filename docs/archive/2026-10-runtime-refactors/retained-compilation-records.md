# Retained compilation records own compiler metadata

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-16. Implements the durable metadata boundary identified in
[compiler metadata ownership](compiler-metadata-ownership.md), following
[optional Store](../../design/optional-store-and-relational.md). Implementation and qualification are complete; this report accompanies the implementation commit.

## Representation and lifetime

A compilation record is the ordinary persistent Vector
`#[#compiledSourceV1 originalSource metadata]`. Its source is the original String
object, not a copy or a value-equal lookup key. Metadata retains the existing
`compiledSourceMetadataV1` value schema: source digest, module information, revision
inventory, method-source inventory and embedded-file paths.

The caller owns a newly prepared record. Registration makes the existing source
registry retain it; revision snapshots and candidate state can retain it independently.
Unreferenced records and their metadata follow ordinary collection. There is no new
native table, per-String attachment, object family, compiler cache or bootstrap
module. Generation's existing native registry now roots record values instead of
bare Strings; moving that registry implementation into Birnam remains separate work.

Records are persistent values. Changing metadata means constructing a new record
and replacing the responsible owner's binding. It does not silently modify an old
revision or a captured compilation. This is the usual collection/value contract,
not frozen dispatch: constructor, accessor, registry and Vector sends remain
replaceable. No method-identity guards are introduced.

## API

| Message | Behavior |
| --- | --- |
| `Codegen analyzeSourceMetadata: source` | Returns a prepared compilation record, or nil for unpreparable source. The old true acknowledgement and implicit Store write are removed. String input is analyzed through the current parser; a supplied valid record is reused without parsing. |
| `Codegen compiledSource: source metadata: metadata` | Constructs a record value without parsing. Readers/admission validate its metadata; constructing a value does not register or activate it. |
| `Codegen sourceTextOf: sourceOrRecord` | Answers the original source String, or nil for unsupported input. |
| `Codegen compiledSourceFor: sourceOrRecord` | Answers a supplied record or finds a registered record by source identity. Equal text is not an alias; no parsing or implicit cache insertion. |
| `Codegen retainedSourceMetadataOf: sourceOrRecord` | Returns the ordinary metadata value, including malformed values for inspection. |
| `Codegen compiledSourceMetadataFor: sourceOrRecord` | Validates and encodes retained metadata using the existing codec; missing/malformed data does not trigger parsing. |
| `Codegen recordModuleSource: sourceOrRecord named: name` | Prepares raw source if needed, validates it and registers the record. Answers the original source String. Duplicate equal source bytes retain the first registered source/record, as before. |
| `Codegen replaceCompiledSource: record named: name` | Explicitly replaces an existing registration with a validated record. It does not install methods or activate code. Existing snapshots remain unchanged. |
| `Codegen moduleSourceFor: name` | Continues to answer the registered original source String. |

For example, a caller can retain analysis and register it without a second parse:

```text
let prepared := [Codegen analyzeSourceMetadata: source];
[Codegen recordModuleSource: prepared named: #example];
let metadata := [Codegen retainedSourceMetadataOf: prepared];
```

An edit uses a new value and an explicit owner mutation:

```text
let replacement := [Codegen compiledSource: source metadata: editedMetadata];
[Codegen replaceCompiledSource: replacement named: #example];
```

The replacement must satisfy the metadata schema, source digest and module identity.
Changing metadata does not retroactively rewrite entered/captured code. Explicit
Generation operations observe the registry revision and reject stale preparations;
ordinary execution is not enrolled in a new transaction or observer protocol.
A replacement followed by restoration still changes that revision (ABA). Replacing
the exact same record is a no-op. Sealing changes registration policy, not metadata:
the existing control revision remains separate from the content revision, so sealing
after adoption does not spuriously stale a valid generation.

## Integration and format transition

`sourceRevisionOf:` returns the existing seven fields followed by its compilation
record at index 7. Index 3 remains the original source String. This eighth field
lets retired/current revision snapshots retain their own metadata, independently
of subsequent registry replacement. SourceLoader passes that record into registration
rather than repeating analysis. Candidate inventory hands back its record alongside
its emitted inventory, preserving the previous number of parses on candidate load.

Artifact files still carry their canonical metadata bytes. Compilation no longer
asserts an in-process fact merely because an artifact was written. Activation creates
an owned record from the artifact metadata and registers it after successful execution.
Image source-registry entries carry records; the existing value codec preserves their
aliases and nested metadata. Mapped startup and compiled admission never reconstruct
metadata by parsing source. Static String inspection parses text locally; callers
wanting retained metadata pass a compilation record explicitly.

There is no compatibility mirror. Core code no longer reads, writes or retracts
`#compiledSourceMetadata` facts. Explicit user facts with that name remain ordinary
independent data and are preserved by images. Existing images with bare-String
source-registry entries require rebuilding; they are rejected rather than repaired
from facts or source. The generic image value encoding and artifact metadata codec
are unchanged, but the source-registry/revision value contract has changed.

## Validation and remaining scope

Required checks cover record reclamation, equal-text identity, original source
aliases, explicit-fact independence, registry replacement and snapshot retention,
Store/db name removal during metadata operations, parser-disabled startup and bad
metadata rejection, candidate activation/rollback and ABA drift, mapped/rebuilt
image aliases, source-free artifacts, and the language suite.

Lean models source identity and immutable snapshots plus owner revision rules. TLA+
checks stale/ABA commits, snapshot aliasing and accidental fact publication. These
are abstract contracts, not proofs of the native compiler or arbitrary callbacks.

Full Store-free bootstrap is still outstanding: lifecycle facts, Alien recipes,
process-local owners and remaining seeds retain dependencies. This slice removes
compiler metadata fact storage; it does not rename all those dependencies away.

## Qualification results

The final isolated build is `/tmp/birnam-durable/final-prefix`. Runtime source
copies match the working tree. The preceding `/tmp/birnam-durable/prefix` build
qualified the same record representation with the full language/image matrix;
the final build separates registry content/control stamps and makes explicit
String analysis use the current parser even for registered text.

- Preparing, reading, registering and replacing records works with the `Store` and
  `db` namespace bindings removed. This is a dependency probe after bootstrap, not
  a claim that the entire bootstrap closure can omit the Store library yet.
- Temporary inspections and unregistered preparation batches retain zero additional
  objects/facts after precise collection. Current metadata accessor replacement
  and collection are exercised; equal-text inputs retain distinct identities.
- Parser/compiler-disabled installed startup passes, including missing/malformed
  retained metadata rejection. Explicit String analysis uses the current parser;
  passing a retained record avoids parsing.
- Candidate retirement passes in bootstrap/running modes and an added metadata-ABA
  prepared-commit case. Seven ledger modes, source-loader goldens, generation
  projection, and exact activation/rollback with callback preservation pass.
- A caller-held, unregistered record survives mapped/rebuilt images with original
  source aliases and its revision snapshot intact. The writer source is deleted.
  Explicit facts under the former metadata attribute remain independent.
- The full image matrix passed source-free mapped/rebuilt restoration, twenty
  resaves with stable retention, snapshot consistency and live failure preservation.
  The full language suite passed 826 assertions and all postchecks on the preceding
  representation build; focused ownership, startup, generation, image, artifact and
  project CLI gates were rerun against the final owner semantics.
- Final artifact/project CLI checks pass compile, activation, dependency preflight,
  removed sources/assets, check/build/run/install/test, cache reuse/repair, and pinned
  dependencies. A pre-cutover bootstrap image is rejected by the final runtime.
- Lean checks 332 theorems and rejects 89 mutations; TLC obtains all 209 expected
  outcomes. New cases detect stale/ABA commits, snapshot aliasing, implicit facts,
  and spurious invalidation caused by treating sealing as a metadata edit.

The first build exposed a remaining String-only reader; it was corrected. A test
fixture initially assumed Vector had indexed replacement; it now constructs new
persistent values explicitly. Generation qualification then caught the policy-seal
stamp mismatch; content revisions now exclude sealing. All affected final checks
pass. No new bootstrap module or native payload family was added.

The [receipt](../../measurements/retained-compilation-records-20261016.json) separates
those runs and records commands, source hashes and corrections. This change and the
preceding inspection-only slice are committed together. No user installation, release
or push was performed; no CPU speedup is claimed.
