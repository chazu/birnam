# 51 — Project Manifests and Dependency Resolution

> **Status:** Source resolver, materialized artifact records, conservative
> artifact reuse, and the first project-run consumer implemented,
> 2026-07-15. This is the first design in the developer experience sequence.
> It defines the project graph consumed by [52 — Project
> Check, Build, and Run](52-project-check-build-and-run.md), [53 — Project-Aware
> Language Tools](53-project-aware-language-tools.md), and [54 — Relocatable
> Bundles](54-relocatable-bundles.md).

Builds on [43 — Modules and the Self-Hosted Layer](43-modules-and-config.md),
[45 — Compiled Units Are Primary](45-compiled-units.md), and [50 — Five-Phase
Implementation Plan](50-five-phase-live-modules-and-isolates-implementation-plan.md).

## 1. The question

Birnam already has three useful pieces, but they do not yet form a project
model:

- Config discovers the nearest birnam.toml and exposes main and load.
- Module stores declared requires edges and computes dependency-first load order.
- Module load: resolves compiled modules by name from one library directory.

The missing piece is a read-only project resolver that answers:

~~~text
Which project am I in?
Which source provides module M?
Which modules are reachable from the requested roots?
In what deterministic order should they be checked, built, or loaded?
Which exact source identities did that answer use?
~~~

The resolver is the shared foundation for a pleasant workflow:

~~~text
birnam.toml
  → project graph
  → check / build / run
  → LSP workspace model
  → installed or bundled runtime resources
~~~

The resolver implementation is deliberately narrower than the eventual project
workflow. It resolves source modules only; the project-run consumer performs the
explicit runtime loading step after resolution and artifact validation. The
materialized build consumes these records, reuses validated artifacts when
possible, and writes artifact identity alongside the compiled dylibs; build
itself does not activate artifacts. This document does not introduce a registry,
network fetches, or a new runtime module representation.

## 2. Governing invariants

### 2.1 Project identity and module identity are different

birnam.toml identifies a project. A source declaration identifies a module.
The canonical module name remains the declared Module named: value. A file
path, package name, artifact filename, or dependency-table key is an origin or
lookup label; it does not rename the module.

The resolver rejects a source whose declared name does not match the module
name it was selected to provide. Two selected sources cannot provide the same
canonical module name.

### 2.2 The manifest selects origins; source declares semantics

The manifest says where a module may be found and which roots the project wants
loaded. The source declaration remains authoritative for the module's semantic
dependencies:

~~~text
manifest dependency entry:  #http → ../shared/http
source declaration:         #http requires #[#socket #file]
~~~

The resolver computes the closure from both inputs. It never treats a manifest
list as a replacement for Module named:requires:.

### 2.3 One graph, one ordering

Every consumer receives the same planned graph and deterministic dependency-
first order. The planned graph is a read-only value produced before runtime
loading; the verified runtime graph remains the Store-backed Module fact graph
after modules load. They are related by the same canonical module names and
requires edges, but the resolver does not create a competing runtime graph.
Store enumeration, filesystem order, and TOML table order have no execution
semantics. Missing modules, duplicate providers, invalid declarations, and
cycles are typed resolution failures with the path through the graph that made
them visible.

### 2.4 Resolution is read-only

Discovering a project, parsing its manifest, indexing source declarations, and
computing the graph do not load a dylib, mutate the Store, evaluate a top-level
program, or activate a compiled unit. A build or run operation explicitly
crosses the corresponding effect boundary after resolution succeeds.

### 2.5 Static source declarations are the first-pass boundary

A source module entry is indexable by the first resolver only when a full-CST
scan can find exactly one literal declaration of this shape, without evaluating
the file:

~~~birnam
[Module named: #http requires: #[#socket #file]];
~~~

The receiver must be `Module`, the selector must be `named:requires:`, the name
must be one symbol literal, and the requires value must be one vector literal
containing only symbol literals. The declaration may occur anywhere in the
file; it is not required to be the first statement. A source with no such
declaration, a computed declaration, or runtime-only dependency loads is a
legacy/non-indexable source and stays on the existing explicit-loading path.
The first resolver does not claim to account for those runtime-only edges.

This boundary is about source metadata, not dynamic libraries. A dylib is a
compiled artifact and is outside the source-only first pass.

### 2.6 Explicit multi-file module composition

A manifest may group an entry source with an ordered list of ordinary Birnam
source files:

~~~toml
[[module]]
entry = "src/json.birnam"
files = ["src/json/parser.birnam", "src/json/render.birnam"]
~~~

The entry must contain the module declaration and owns the canonical module
name and dependencies. The listed files are source parts: they must not contain
another module declaration, are read in the listed order, and are composed into
one source input before compilation. The resolver retains the complete path
list and combined source digest, so one logical module still produces exactly
one `libbirnam_<name>.dylib` and changes to any part invalidate that artifact.
Explicitly composed paths are removed from the generic source scan, preventing
them from becoming duplicate module providers.

### 2.7 Artifact identity is explicit

A compiled artifact is usable for a selected source revision only when its
recorded module identity, dependency metadata, target identity, and source
revision/content digest agree with the resolution record. A merely present but
stale dylib must not silently shadow an edited source file.

The normal runtime authority remains the compatible compiled unit. Source is the
authoring and recovery representation, consistent with design 45 and the live
module generation model.

### 2.8 Local-first means no hidden network behavior

The first resolver handles the local filesystem and explicitly named path
dependencies. Normal check, build, run, and LSP startup do not contact a
registry or mutate a global cache. Registry and remote transport can be added
behind an explicit later policy without changing the graph contract.

### 2.9 One resolver serves every front end

The CLI, LSP, Emacs commands, stage-1 build, image/baseline workflows, and
future package tooling consume the same project-resolution records. None grows a
second parser, source index, or dependency algorithm.

## 3. Manifest shape

The current top-level keys remain accepted:

~~~toml
name = "hello-birnam"
version = "0.1.0"
main = "app.birnam"
load = ["app_support"]

[[module]]
entry = "src/json.birnam"
files = ["src/json/parser.birnam", "src/json/render.birnam"]

[run]
# main exits without starting a REPL unless explicitly configured.
repl = "none"
socket_bind = "127.0.0.1"
socket_port = 4400
~~~

The project layer adds source roots and explicit local dependencies:

~~~toml
name = "hello-birnam"
version = "0.1.0"
main = "app.birnam"
load = ["app_support"]

source_roots = ["src", "modules"]

[dependencies]
http = { path = "../birnam-http" }
shared = { path = "vendor/shared" }

[[wrapped]]
name = "foo"
build = [["cc", "-dynamiclib", "native/foo.c", "-o", "{output}"]]
~~~

The wrapped declaration is implemented as the first project-native C-library
build slice. Its semantics are:

- name and version describe the project and do not rename modules.
- main is a project-root-relative entry source, not a module dependency.
- load names startup module roots for the existing bare/configured run path.
- each `[[module]]` groups one entry and optional ordered source parts into one
  logical compiled module. The entry owns the `Module named:requires:`
  declaration; parts do not declare modules.
- run selects the explicit post-main interaction mode; `repl` is `none`, `tty`,
  or `socket`, with socket bind-address/port settings consumed by design 52.
- source_roots are ordered search roots for project-owned source modules.
- dependencies maps canonical module names to standalone local project roots
  in the first slice. Each path dependency has its own birnam.toml.
- wrapped names a logical C library. Each `build` entry is an argv array, not a
  shell string; `{output}` is required and is replaced with a private staging
  path. `{project}`, `{build}`, and `{lib}` are also available as path
  placeholders.
- the first slice executes wrapped commands from the root project's directory,
  stages the resulting `lib<name>.dylib` under the project's published `lib/`,
  and exposes it to a wrapper's bare `[Alien dlopen: "lib<name>.dylib"]` during
  `birnam run`. `birnam check` validates the declaration but never executes it.

One dependency entry initially selects one canonical module from the dependency
project. The dependency project may contain other source modules for its own
transitive closure; a package-wide multi-module dependency declaration is not
part of this slice.

If source_roots is absent, the first implementation uses `["src"]`. The path
is always resolved relative to the owning manifest; absolute dependency paths
and registry/URL origins are not accepted.

Existing projects without a manifest continue to work. Existing main and load
behavior remains compatible while the new resolver is introduced behind the
explicit project commands.

## 4. Resolution model

The resolver takes:

~~~text
project root
manifest
requested module roots
source roots
dependency origin table
~~~

It returns a value-like ProjectResolution containing:

~~~text
project name and root
manifest identity
module records in deterministic topological order
root modules and entry source
for each module: canonical name, source origin, source digest, dependencies
diagnostics, if resolution failed
~~~

The algorithm is:

1. Discover the nearest manifest from the requested working directory, unless a
   command supplies an explicit project path.
2. Parse and validate manifest keys without loading Birnam modules.
3. Parse explicit `[[module]]` groups and compose each entry plus its ordered
   parts. Reject duplicate paths and module declarations in source parts.
4. Index remaining candidate `.birnam` source files by exactly one statically
   readable canonical Module named:requires: declaration. Legacy/non-indexable
   files are not project modules. File discovery order is sorted by path bytes.
5. Add project load roots and explicit dependency roots. The entry source is
   checked for existence separately and is not silently turned into a module.
6. Read each selected module's declarative dependency metadata and recursively
   resolve its dependencies.
7. Reject missing providers, duplicate providers, declaration/name mismatches,
   malformed source, and cycles with a diagnostic chain.
8. Emit a stable dependency-first order, with module-name bytes as the tie
   breaker, plus a content-addressed resolution digest.

The resolver does not call Module load:. Runtime loading consumes the resolved
records after a command has established its effect policy.

## 5. First-pass source selection

The first resolver has one selection policy:

~~~text
source       require an indexable source module entry or explicit module group
~~~

`birnam check` is read-only. Build uses the materialized artifact record for
staged reuse and invalidation; `birnam run` validates that published record and
the on-disk artifact checksums before loading it. Source fallback remains
outside this first project-run slice.

The first materialized build writes `artifacts.toml` alongside the dylibs. It is
an identity record, not a second runtime graph. Every artifact record contains:

- canonical module name;
- ordered source path list, including the entry;
- dependency metadata;
- target/ABI identity;
- source revision/content digest;
- artifact checksum;
- the build target, profile, and artifact ABI in the record header.

`birnam build` may reuse an artifact from the previous successful publication
only when target, profile, tool version, artifact ABI, source identity,
dependency metadata, expected relative path, and on-disk checksum all match.
Invalidation is dependency-aware: a rebuilt module also rebuilds its
dependents, while unrelated matching modules can be copied into the new
staging directory. A missing, malformed, incompatible, or tampered record is
treated conservatively as a cache miss and causes compilation. `run` consumes
the validated published records; it does not fall back to an older artifact or
source-evaluate a stale module.

The output must remain compatible with the existing
`libbirnam_<name>.dylib` loader and the compiled-unit/image authority contract.

## 6. Failure vocabulary

The first public diagnostic set should distinguish at least:

~~~text
projectNotFound
manifestMalformed
manifestValueInvalid
moduleNotFound
moduleNameMismatch
duplicateModuleProvider
dependencyCycle
dependencyOriginInvalid
sourceMalformed
legacyModuleSource
entrySourceMissing
~~~

Every dependency failure includes the requested module, the origin considered,
and the dependency chain. A missing #file needed by #http should read as a
resolution path, not as a generic loader failure.

## 7. Delivery slices

### P0 — read-only project graph — implemented

Add a resolver API and a diagnostic printer. It supports manifest discovery,
main/load/name/version, source roots, and the exact static source-declaration
boundary. It produces a deterministic source graph without changing runtime
behavior.

### P1 — local path dependencies — implemented

Add standalone-project path entries, canonical-name validation, transitive
source resolution, cycle/missing diagnostics, and a machine-readable
resolution record.

### P2 — artifact identity and reuse — implemented

The project build records source/revision digests, dependency metadata, target,
ABI, and artifact checksums in a complete staged publication. Matching-artifact
reuse and dependency-aware invalidation are implemented for `build`; `run`
validates the published record and on-disk checksums before loading artifacts.
Source fallback remains intentionally outside this project-run slice.

### P3 — lock and remote policy, only if demanded

Add a lockfile and registry/transport policy only after local projects show a
need. A registry is not part of the initial developer-experience milestone.

## 8. Acceptance scenarios

- Running from a nested directory discovers the same nearest project root as
  Config, without changing process cwd.
- A project with main and load produces the same startup order as today.
- A local dependency whose source declares a different module name is rejected
  before any code is loaded.
- Repeating a successful build reuses every unchanged artifact.
- Editing or tampering with a dependency artifact rebuilds that module and its
  dependents while leaving unrelated modules reusable.
- A transitive dependency is emitted once, before its dependent, independent of
  filesystem order.
- A missing dependency reports the complete root-to-missing chain.
- A cycle reports the cycle and terminates without loading or hanging.
- Two projects can be resolved independently without mutating a global registry
  or cache; different revisions in one runtime are not supported by this slice.
- The source-only resolver never loads or selects a dylib.
- A module entry plus multiple source parts compiles once, loads once, and
  publishes one dylib; changing any listed part invalidates that artifact.
- The same resolution digest and module order are observed by check, build,
  run, and LSP workspace initialization.

## 9. Explicit non-goals

- no package registry or implicit network fetch;
- no new runtime module graph competing with Store facts;
- no automatic module rename from a path or package name;
- no multi-module package semantics in P0/P1;
- no implicit image load from a manifest key;
- no source evaluation during resolution.
