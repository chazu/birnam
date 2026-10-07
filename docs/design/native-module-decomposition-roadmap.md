# Native module decomposition roadmap

The follow-on [runtime and ABI simplification plan](runtime-and-abi-simplification.md)
tracks interface, rooting, representation and bootstrap cleanup separately from
the completed extractions recorded here.

Updated: 2026-10-04.

All five decompositions below are agreed directions. Start with **source compiler
and native backend**. This document tracks extraction, acceptance and later Birnam
replacement separately. The order after the first extraction is the recommended
sequence and can change as dependencies become clearer.

The goal is to give each native module one complete responsibility, a small
Id-message interface and ownership of its own state. That makes it possible to
replace its implementation with Birnam independently. Moving Rust files or making
another crate that borrows the old owner's state does not complete a decomposition.

All five extractions are implemented and validated. The first delivery batch
(Host owners and extractions 1–3) was committed and pushed on main as `9aace02e`.
The final batch adds AlienCompiler and ImageFormat. Subsequent Birnam rewrites
remain separate milestones.

## Order and status

| Order | Decomposition | Extraction | Birnam replacement |
| --- | --- | --- | --- |
| 1 | Source compiler and native backend | Implemented and validated | Planned after extraction |
| 2 | Compilation and generation publication | Implemented and validated | Existing CompiledUnit policy stays Birnam; remaining bookkeeping can follow |
| 3 | Project observation and project build | Implemented and validated | Build sequencing and publication construction are initial targets |
| 4 | Alien wrapper compiler and Alien runtime | Implemented and validated | AlienCompiler rewritten in Birnam |
| 5 | Image format and image restoration | Implemented and validated | ImageFormat authored in Birnam; minute-scale admission regression resolved and installed startup verified |

Update each row when work starts. An extraction is complete only when production
callers use the new native owner through Id, its acceptance checks pass, and the
displaced implementation path is removed. Record the commit and evidence location
when it is delivered. A Birnam replacement is a separate milestone.

### Small rewrite checkpoint: Printing (2026-10-04)

Default Object/Symbol printing is now in `modules/printing.birnam`, loaded after
Lang on the primitive floor. The native Printing crate and production inventory
entry are removed. Bytes owns generic Symbol-name copying; formatting policy
remains authored. Printing is marked loaded so public Module reload preserves
replacements. ProjectBuild remains deferred as a larger rewrite.

Validation: cold Lang seed compiles the artifact without a Rust printer; focused
output/printing, compiler, text/error and memory checks pass, including collecting
constructors, native failures, one live String factory call per render, root
printer replacement/reload and wrong arity. Workspace check passes. A full install
and full-image startup were not rerun; the ImageFormat performance issue remains.

### Small rewrite checkpoint: SourceLoader (2026-10-04)

`modules/source-loader.birnam` now owns System source loading. The native crate
and production inventory entry are removed. Registration still precedes evaluation;
nil registration does not prevent execution; exact Error results retain identity;
notification consults Namespace only. Original filenames remain owned for diagnostics
while the original path Object is passed to Codegen. Public Module reload preserves
System method replacements.

Three small reusable owner operations support the policy: File I/O returns a
bounded owned path/content Bytes snapshot (nil on file/encoding failure), Namespace
provides nullable language lookup over its live `at:`, and System Output writes
UTF-8 diagnostic text to stderr. They contain no compiler/loading policy.

Validation passed: cold seed compilation without the Rust source loader; focused
source-loader, compiler, embedding, system-services and output integration checks;
six shared-owner unit tests and workspace check. Source-loader checks include
collecting/reentrant dependencies, source identity, filename mutation, error/failure
propagation, sealed registration and reloads. Full install/image startup was not
rerun at that checkpoint. Printing and SourceLoader were subsequently committed
and pushed as `81b93307`; a fresh installed startup was verified after the
ImageFormat performance work in `c59df7ca`.

### Small rewrite checkpoint: BlockLoop, Range and SystemGC (2026-10-04)

All three are now authored, in the requested order. Their Rust crates, workspace
entries, lockfile packages and native inventory entries are removed. Bootstrap
loads compiled BlockLoopControl/BlockLoop, Range and SystemGC artifacts after
Lang. The existing compiler seed builds them without the removed native modules.
The [authored contracts](authored-small-modules.md) describe the small generic
owner operations used for typed operands and ordinary reference slots.

BlockLoop retains exact Error-family recognition, live callbacks, constant stack
depth and the bootstrap suspension barrier. Range retains inclusive signed i64
bounds, nil for oversized counts/invalid indices, ignored `do:` callback results,
folding, printing and materialization. Its endpoints now use ordinary reference
slots. SystemGC retains live count-query/result-constructor order, count bits,
canonical setter validation, alias receivers and GC's existing policy. Real
allocation diagnostics include authored capture/argument construction.

Stackless control explicitly requires BlockLoop and Range, because it overrides
their methods. This gives generation adoption/image recovery the same dependency
order as actual bootstrap and prevents a managed-source mismatch on Range `do:`.

Validation passed: workspace cargo check; the ordinary-slot layout unit test;
seven production integration binaries (BlockLoop, collections, SystemGC,
compiler, Object Model, numbers and tagged tables); all native export/install-name
and dependency audits; fresh release installation with new startup/tool images;
exact installed receipt; source-free image recovery of Range aliases, clones,
both i64 endpoints and SystemGC; REPL arithmetic, FizzBuzz and collections.
The REPL arithmetic session completed in 6.00 seconds. No exhaustive Image GC
or whole-language suite was rerun.

The isolated acceptance prefix is `/tmp/birnam-small-ports/final-prefix`, with
TCC's explicit resource-metrics feature for the compiler tests. Logs are
`/tmp/birnam-small-ports/install.log`, `focused-tests.log` and `acceptance.log`.
The installed user prefix was not replaced.

### Small rewrite checkpoint: NativeLoader and Error (2026-10-05)

Both are authored and their Rust crates, lockfile/workspace entries and native
inventory entries are removed. Native Loader uses typed Bytes/Bootstrap operations
over current Id sends. Error uses ordinary reference slots and loads before Lang;
the existing grammar seed compiles its source into an empty build prefix.
Execution adapts source nullable arguments and Id's variadic DNU convention.
Canonical ErrorPrototype publication preserves owner identity during image
recovery. No native Error implementation or loader policy remains in production.

The [contracts and acceptance](authored-small-modules.md#native-loader) document
the owner boundaries, native failure/result handling, root lifetime, fresh
bootstrap and image/baked-entry checks. The installed user prefix is unchanged.


### Native value foundation (2026-10-05)

The [value foundation](native-value-foundation.md) tracks the agreed Word / Bytes /
Execution split and the five remaining full value-module ports separately.
Incoming native ABI adaptation now survives captured Method aliases and
mapped/rebuilt images. Execution and PortableCode initialize before the native
value modules; compiler capture storage uses generic Object Model reference
slots. Compiler/reflection startup still requires the five native value owners.

## Rules shared by every decomposition

- Each new native owner has the standard module descriptor, explicit completed
  inputs and its own state. Native consumers use current Id lookup.
- Transfer ordinary Objects or canonical bytes. Keep Rust IR, private payload
  layouts, native capabilities and mutable state borrows inside their owners.
- Preserve creating-thread execution, exact roots across effectful sends, copied
  byte borrows, error behavior and existing observation order.
- Compiler-produced metadata stays in Store. Compiled startup rejects missing or
  malformed metadata; it never parses source as a fallback.
- Keep TCC's mapping ownership and Execution's Method, Block, frame and active-code
  retention. Preserve idle publication and delayed physical reclamation.
- Reuse existing algorithms and authored policy. Add no parallel resolver,
  generation model, generic service catalog or second runtime.
- First extract with behavior unchanged. Then replace one implementation with
  Birnam, using the same externally observable acceptance checks, and remove the
  displaced Rust implementation.

These extend the existing [functional ownership contract](90-functional-modules.md).
Role names below describe proposed ownership; final receiver, crate and selector
names remain implementation decisions.

## 1 Source compiler and native backend

### Responsibility

The source compiler owns source/CST interpretation, semantic lowering, source
identity, embedded-file inventory, compiler metadata production and diagnostics.
The native backend owns portable-program lowering, private IR, slot allocation,
frame and selection descriptors, optimization construction and C emission.
TCC continues to compile and own native mappings. Execution continues to own
executable Methods, Blocks and their lifetime.

```mermaid
flowchart LR
    L[Lang CST] --> S[Source compiler]
    S --> P[Canonical portable programs]
    S --> M[Compiler metadata in Store]
    P --> B[Native backend]
    B --> C[Generated C and descriptors]
    C --> T[TCC owned mappings]
    T --> E[Execution Methods and Blocks]
```

The existing canonical portable-program format provides the main seam. The
backend must accept every supported compilation mode without receiving a Rust
`Program`, CST, IR or reference to Codegen's aggregate state. Original source and
filename provenance, root-binding persistence, capture semantics and factory
selection must survive the extraction.

### Implementation sequence

- [x] Identify the existing portable-program seam and its production consumers.
- [x] Inventory ordinary source compilation/evaluation, emitted C, source
  artifacts, named adapters, synchronous/resumable materialization, candidate
  factories and image bundles. Record each mode's inputs and observable behavior.
- [x] Finalize the smallest value/byte interface covering those real consumers.
  Preserve the existing canonical formats; keep backend IR private.
- [x] Extract one complete native backend owner with descriptor, construction,
  rooting and error handling. Retain current emission algorithms.
- [x] Route every native emission consumer through Id. Move typed Rust helper
  dependencies behind the owner and remove production bypasses.
- [x] Update Bootstrap/build/install/export inventories and verify the actual
  installed library and executable-relative loading.
- [x] Complete acceptance and record delivery evidence.
- [ ] Convert source lowering and metadata production to Birnam while retaining
  the native backend. A backend rewrite can follow independently.

This first extraction does not redesign generation publication or replace the
compiler frontend simultaneously. It establishes the seam those later migrations
will consume.

### Acceptance

Retain canonical portable fixtures and independent codec checks. Exercise source
compilation/evaluation, artifacts, named methods, materialization, candidates and
image code through the installed backend. Preserve nested captures, global versus
lexical bindings, numeric bits, diagnostics/provenance, evaluation effects and
synchronous/resumable capture observation order.

Run the relevant compiler, artifact, portable execution, materialization,
recovery/frame and image integration checks, plus workspace and installed Host
checks. Prove TCC reclamation after the last live executable references disappear.
Keep parser/compiler-disabled compiled startup and malformed-metadata refusal.
Compare canonical programs and behavior; generated C includes provenance, so
byte equality alone is not an adequate equivalence test.

Sources: [source compiler facade](../../rust/birnam-codegen-runtime/src/lib.rs),
[semantic lowering](../../rust/birnam-codegen/src/lib.rs),
[native emission](../../rust/birnam-native-backend/src/native.rs),
[resumable lowering](../../rust/birnam-codegen/src/resumable.rs),
[metadata persistence](../../rust/birnam-codegen-runtime/src/source_metadata.rs).


### Extraction checkpoint: 2026-10-02

Implemented `birnam-native-backend` as a separately loaded native owner. Production
emission and descriptor requests cross Id using canonical program bytes and
ordinary Objects. The existing C emission algorithms were retained. Bootstrap,
installation and export inventories include the new library. This work and the
preceding Host extractions are included in the first delivery batch.

Validation passed: workspace tests, the full native message suite, installed CLI
checks, generation/image publication checks, compiled startup with emission
messages disabled, and a focused backend rooting/error-propagation check. Four
representative emitted C comparisons matched the preceding installed compiler.
The complete authored-language suite and doc generation were not rerun for this
checkpoint.

Evidence logs are retained locally in
`.cargo-target/measurements/native-backend-20261002/`. The clean installed test
prefix is `/tmp/birnam-decomposition/backend-clean-prefix`.

The full native suite was disproportionately expensive for this checkpoint:
individual image checks took 20–48 minutes, including nested restoration checks
that force precise collection after restored slots. Later extractions should
start with focused acceptance and report a checkpoint before broad validation.

## 2 Compilation and generation publication

Extract generation admission, candidate preparation, commit, exact inverse
publication, compensation, rollback, mutation authorization and generation
recovery from Codegen's aggregate state. Keep these transitions together under
one native owner. Codegen supplies compiled code and metadata; Store retains
durable records; TCC and Execution keep their existing resource ownership.

CompiledUnit, DispatchGeneration and EvolutionIntent already express policy in
Birnam. Preserve that policy and reconnect it to the extracted mechanism. Later
Birnam work can replace ledger and transition bookkeeping without changing source
compilation. Native enforcement of idle admission and exact ownership remains
where required until an equivalent replacement is proven.

- [x] Identify complete transition state and interactions with source registration
  and Image; resolve dependencies without exposing Codegen's Rust state.
- [x] Extract one owner and move all publication consumers to its Id interface.
- [x] Preserve the existing authored policy and remove the old mechanism path.
- [x] Validate and record delivery; track subsequent bookkeeping replacement.

Acceptance includes candidate, generation, ledger, transaction, retirement and
Store compensation checks; installed CompiledUnit and console/TCP intent checks;
and composed-image recovery. Preserve refusal during entered execution, stale
preparation refusal, callback/observer failure compensation, exact rollback and
reclamation only after old executable references are gone.

Sources: [candidate state](../../rust/birnam-generation/src/candidate.rs),
[ledger](../../rust/birnam-generation/src/candidate_ledger.rs),
[transactions](../../rust/birnam-generation/src/candidate_transaction.rs),
[authored policy](../../modules/compiled-unit.birnam).

### Extraction checkpoint: 2026-10-02

`birnam-generation` now owns candidate state, preparation, publication, inverse
ledgers, compensation, retirement, running admission and recovery. Source
registration and the staged registry exchange moved with the ledger, preserving
atomic native ownership updates. Codegen retains compilation, canonical metadata
production and candidate inventory lowering. All publication callers use
Generation through Id; the old Codegen publication methods were removed.
CompiledUnit, DispatchGeneration and EvolutionIntent retain their authored policy.

Focused validation passed: 71 pure compiler tests; candidate admission; retirement
in bootstrap/running phases; compiler execution; canonical initial projection;
ledger drift/adoption; generation transaction compensation and exact rollback;
Store compensation; installed console/TCP activation and rollback; 57 CompiledUnit
policy checks; composed-image mapped/rebuilt/resaved recovery and compensation;
and parser/compiler/emission-disabled startup. The native export, install-name
and dependency audit passed for the fresh 44-library prefix. The native focused
checks took seconds each; the composed-image test took about 87 seconds including
its subprocesses. No full native suite, exhaustive per-slot image GC suite,
authored-language suite or doc generation was run for this extraction.

The focused checks caught and corrected a mutation-refusal diagnostic change;
rejection once again leaves Codegen's compiler error untouched. Image test hooks
now target Generation's adoption method.

Changes are included in the first delivery batch. Evidence is retained locally in
`.cargo-target/measurements/generation-20261002/`; the disposable installed prefix
is `/tmp/birnam-publication/prefix`. Subsequent Birnam bookkeeping replacement
remains planned.

## 3 Project observation and project build

Keep project observation responsible for read-only discovery, manifest/source
context and dependency observations. Extract project build as one owner for
compilation sequencing, reuse validation, generated module/configuration/application
construction, staging and publication. ProjectPolicy continues to decide provider
selection, dependency order and compile-versus-reuse policy.

Published application entry should use existing HostEntry. LSP owns editor entry
and asks Project for observations through messages. Prefer removing the scoped
LSP callback/startup detour over introducing another permanent native LSP owner.
Dependency acquisition remains an explicit fetch operation; observation stays
offline and does not execute native build commands.

- [x] Separate observation from build/publication state and effects.
- [x] Extract the build owner and reconnect CLI, test and installation consumers.
- [x] Remove LSP startup from Project while preserving project-aware diagnostics.
- [x] Validate and record delivery.
- [ ] Move build sequencing and generated publication construction to Birnam,
  retaining the necessary filesystem/process effects for atomic publication.

Acceptance covers check/build/run/fetch/install/test, dependency/provider errors,
compile-versus-reuse identity, a failed stage preserving the previous publication,
published execution after deleting source, and LSP workspace/UTF-16/diagnostic
behavior. Preserve precise collection and callback revocation while removing the
old bridge, then test the new project-query interface directly.

Sources: [project operations](../../rust/birnam-host-project/src/project/operations.rs),
[publication construction](../../rust/birnam-project-build/src/project/data.rs),
[filesystem effects](../../rust/birnam-host-project/src/project/files.rs),
[ProjectPolicy](../../modules/project-policy.birnam),
[LSP](../../modules/lsp.birnam).

### Extraction checkpoint: 2026-10-02

`ProjectBuild` now owns compilation checks, build/reuse sequencing, generated
publication construction, staging, explicit dependency fetch, install and project
run/test orchestration. `HostProject` retains read-only discovery, manifest/source
observations, dependency/cache inspection and policy resolution. Build consumes
owned snapshots, manifests, paths and source metadata through current Id queries;
no observation implementation or private state crosses that boundary.

The original Birnam ProjectPolicy remains in charge of provider selection,
ordering and compile-versus-reuse decisions. Shared local Rust value contracts,
policy projections and bounded Git utilities live in `birnam-project-support`,
which has no native owner or mutable project state. Publication construction and
write effects remain private to ProjectBuild. Published execution still uses
HostEntry and existing HostInteraction.

LSP now calls HostProject's permanent `pathForUri:` and `snapshotForUri:` messages.
The old borrowed callback bridge, temporary Project global, revocation guard,
HostProject `lsp:` entry and HostInteraction `cliDirect:` detour were removed.
The replacement query test checks precise collection, retained receiver use,
malformed URI/arity handling, no source evaluation and no project build/cache
writes. No stack-borrowed callback remains to revoke.

Focused validation passed: 30 scoped unit tests; the native-owner descriptor,
reload and collecting-error checks; source-observation and policy collection
checks; permanent query collection checks; the installed project workflow smoke
(check/build/run/fetch/install/test, reuse/repair, failed-stage preservation,
source-free published execution, path/transitive native dependencies and pinned
Git acquisition/cache validation); and LSP workspace, diagnostics, UTF-16,
versioning/navigation and close behavior. The 45-library native export,
install-name and dependency audit passed. No full native suite, exhaustive image
GC suite or unrelated compiler/generation tests were run.

Native VM fixtures require fresh processes. A combined ignored-test invocation
hit the existing singleton/thread initialization restriction; the permanent query
case passed when run separately, as the validation script requires.

Changes are included in the first delivery batch. Evidence is retained locally in
`.cargo-target/measurements/project-split-20261002/`; the disposable installed
prefix is `/tmp/birnam-project-split/prefix`. Build sequencing/publication
construction in Birnam remains a subsequent migration.

## 4 Alien wrapper compiler and Alien runtime

Extract signature parsing, deterministic ABI classification and wrapper/callback
source generation into an independently loaded wrapper compiler. Keep foreign
addresses, library handles, address membership, callback roots and wrapper
mapping lifetime with Alien runtime. TCC remains the authority for platform ABI
size, alignment and calling convention.

- [x] Extract the wrapper compiler and reconnect runtime and image consumers.
- [x] Validate and record delivery.
- [x] Replace signature parsing and wrapper-source generation with Birnam.

This is the preferred smaller rewrite after the first compiler extraction if an
early conversion is useful. Acceptance includes malformed signatures/identifiers,
scalar and aggregate calls, callbacks in both directions, pointer membership,
collecting callbacks, actual wrapper lifetime and mapped/rebuilt/baked recovery.

Sources: [signature grammar](../../modules/alien-compiler.birnam),
[ABI metadata](../../modules/alien-compiler.birnam),
[source generation](../../modules/alien-compiler.birnam),
[runtime ownership](../../rust/birnam-alien/src/runtime.rs).

### Extraction checkpoint: 2026-10-02

AlienCompiler now owns parsing, ABI classification and wrapper/callback source
generation. Alien performs current Id compiler lookup and retains symbol
resolution order, address/library/callback ownership and TCC mapping lifetime.
Saved wrapper restoration still maps recorded object bytes without compilation.
The parser, classifier, emitter and C prelude moved without algorithm changes.

Pure compiler/registry tests passed (17 compiler tests and 4 registry tests), as
did installed scalar/unsigned/aggregate calls, recursive/collecting callbacks,
address membership and final wrapper reclamation. The live Id seam test covers
malformed signatures/identifiers, collecting compiler replacement and exact native
failure propagation. Source-free foreign wrapper mapped/rebuilt/baked recovery
passed through the combined Image check.

### Birnam replacement: 2026-10-03

`modules/alien-compiler.birnam` owns the signature grammar, deterministic ABI
classification and complete wrapper/callback C generation. It preserves the
`compile:signature:` contract and existing C algorithms. Parsed signatures are
ordinary vectors and Strings; there is no native compiler IR or Rust state seam.
The C bridge templates are embedded at compilation time.

Bootstrap loads its compiled artifact before Alien and before the standard
library. The installer builds it with the existing Lang grammar seed and excludes
the former Rust compiler library from production. Alien resolves the authored
compiler through Globals on each request. Foreign resources and mappings remain
owned by Alien and TCC.

The old pure Rust implementation is retained only as a standalone test oracle in
`tests/reference/alien-compiler`, outside the production Cargo workspace. Focused
acceptance is `scripts/alien-compiler-test.py PREFIX`, the native compiler seam,
the production Alien FFI test, and `scripts/alien-image-test.py PREFIX`.

Validated with a fresh release prefix: 46 native libraries, 3 early authored
artifacts, 4 source bootstrap modules and 23 authored tools. Workspace compilation,
17 frozen-reference tests and 4 Alien loader tests pass. Installed authored output
matches the frozen Rust compiler byte for byte in 64 C/diagnostic cases, with 8
additional ABI cases, all without stdlib or source loading.

The live compiler seam passes malformed inputs, collection inside the parser,
replacement and exact native-failure propagation. Production FFI passes scalar,
unsigned and aggregate calls, collecting/recursive callbacks, memory ownership
and wrapper reclamation. Its collection hook runs during live FFI conversions;
the compiler gets a separate bounded collection check rather than collection on
every Integer allocation. Source-free mapped/rebuilt/baked wrapper recovery,
startup with compilation/parsing denied, exact install receipt and relocation
checks also pass. The bundled compiler source recompiles without the Rust grammar
seed and the newly loaded artifact emits byte-identical aggregate C. No exhaustive
Image collection suite or performance benchmark was run for this rewrite.

Reproducible commands: `mise exec -- python3 scripts/alien-compiler-test.py PREFIX`,
`mise exec -- python3 scripts/alien-image-test.py PREFIX`,
`cargo test -p birnam-host --test native_compilers --offline --locked -- --ignored`,
and `cargo test -p birnam-siphon --test alien --offline --locked -- --ignored`
(the native tests require `BIRNAM_TEST_MODULE_DIRECTORY=PREFIX/lib` and the Alien
fixture built by `scripts/native-message-test.sh`). Local logs are preserved under
`.cargo-target/measurements/alien-compiler-rewrite-20261003/`.

## 5 Image format and image restoration

Extract encoding and structural validation as one complete format owner. Keep
live heap restoration, Store exchange, source/binding publication, generation
adoption and compensation as one restoration transaction. Code staging continues
to consume the compiler/backend and generation-publication owners.

- [x] Define the complete pure format responsibility and its validated data seam.
- [x] Extract it without changing frozen bytes, bounds or admission rules.
- [x] Validate and record delivery.
- [ ] Replace format implementation with Birnam and measure startup/load cost.

Acceptance includes frozen v8 fixtures, malformed/truncated records, references
and checksums; forced collection during restoration; mapped/rebuilt loading;
partial failure compensation; and stable repeated resaves. A Birnam replacement
must preserve bounded parsing and must be measured through usable startup as well
as isolated format operations.

Sources: [format proof](../../rust/birnam-image-codec/src/v8.rs),
[snapshot and restoration](../../rust/birnam-image/src/native.rs),
[retained facts](../../rust/birnam-image-codec/src/retained_facts.rs).

### Extraction checkpoint: 2026-10-02

ImageFormat owns full container encoding and structural validation through
ordinary value/byte messages. Image retains live object projection/restoration,
atomic file publication, Store/source exchange, generation adoption and exact
compensation. Frozen field/record helpers are shared local codecs, not an owner
state interface. No complete-format encode/decode bypass remains in native Image.

The 17 codec tests passed, including frozen v8 bytes, bounded malformed/truncated
records, reference validity and checksum/authority cases. The native seam test
roundtrips frozen fixture bytes exactly, forces collection, preserves public nil
refusal and proves native dependency failures propagate. Installed composed-image
mapped/rebuilt/resaved recovery, failure compensation, fixed resave ownership and
parser/compiler/emission-disabled startup passed. Saved foreign wrappers survived
source deletion and mapped/rebuilt/baked entry. The 47-library export, install-name
and dependency audit and workspace compilation check passed.

Evidence is retained locally in `.cargo-target/measurements/last-splits-20261002/`;
the disposable installed prefix is `/tmp/birnam-last-splits/prefix`. Validation
excluded the exhaustive per-restored-slot GC suite, the full native suite,
authored-language suite and doc generation. The standalone native seam check took
under a second; live Alien validation took about 12 seconds. No Birnam implementation
rewrite or startup performance benchmark is claimed for this extraction.

## Responsibilities to keep together

Execution's frame arenas, Methods, continuations, drive stops, guards and recovery
share exact ownership and lifetime. Improve private Rust structure while retaining
one native owner. Splitting these into separately loaded modules would make the
interface larger and move private capabilities across it.

Keep Artifact's related format codecs cohesive. Do not create one native module
per record, cache or helper. After backend extraction, materialization orchestration
and cache policy are a plausible small Birnam conversion; a separate materialization
library is not a prerequisite for this roadmap.

## Delivery record

For each delivered extraction or rewrite, append its commit, installed prefix or
artifact, acceptance evidence, remaining limitations and removal audit here. Keep
the order/status table current. - First batch: `9aace02e`, committed and pushed on main; Host owners and
  extractions 1–3. Acceptance and local evidence are recorded above.
- Final batch: the commit containing this record, on main; AlienCompiler and
  ImageFormat. Acceptance and local evidence are recorded above. All five native
  extractions are complete; their subsequent Birnam rewrites remain planned.
