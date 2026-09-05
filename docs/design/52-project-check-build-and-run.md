# 52 — Project Check, Build, and Run

> **Status:** Complete through the defined check/build/run/install/test workflow,
> 2026-07-22; installed artifact semantics are reconciled with Design 54. The
> shipped surface includes read-only compile checking, stable machine output,
> debug/release publication identities, editor profile selection, and isolated
> project tests. This document
> defines the command and artifact workflow over [51 — Project Manifests and Dependency
> Resolution](51-project-manifests-and-dependency-resolution.md). It is a
> product-facing layer over the existing --compile-module, Module load:,
> image, and source-fallback primitives.
>
> **Publication amendment (2026-09-03):** Design 65 replaces the receipt-based
> build/run contract below. Ordinary run now consumes compiled publication
> roots without source observation; receipt-specific text is historical.

## 1. The question

Birnam can already run a source file, load configured compiled modules, compile
one module to a dylib, boot an image, and enter the REPL. A project user still
has to know which of those modes to compose and how to refresh artifacts.

The command model makes the normal loop explicit:

~~~text
birnam init
birnam check
birnam build
birnam run
~~~

The existing commands remain valid. These commands add a stable project
workflow rather than replacing the live image and compiled-unit model.

## 2. Governing invariants

### 2.1 Resolve before effect

Every command first obtains a ProjectResolution from design 51. A failed
resolution performs no module load, source evaluation, artifact replacement,
or image save.

### 2.2 Check is read-only

`check` resolves the source graph, validates static module declarations and
dependencies, and invokes the compile-only backend for every resolved module in
a private temporary directory. Compile-check remains read-only: it must not run module
initializers, install dispatch, mutate the user's image, or publish artifacts.

It may reuse the existing compile-only backend, but its observable contract must
remain read-only even if the compiler internally needs a temporary artifact.

### 2.3 Build is staged and atomic

build writes into a fresh staging directory, verifies every artifact and the
complete resolution record, then publishes the project build directory as one
atomic replacement. A failed build leaves the previous successful build intact.

No command may report success while leaving a partially compiled dependency
graph that a later run could accidentally consume.

The semantic build policy is implemented in Birnam. The host passes
`ProjectPolicy` a snapshot containing current module metadata and prior records
with observed checksums. Birnam selects reuse or compile actions, propagates
dirty dependencies, constructs the artifact-record value, and authorizes
publication after staging. Zig remains the adapter for compiler/process launch,
hashing, file copies, TOML serialization, and the atomic rename.

### 2.4 Run names its runtime inputs

run executes against the selected project resolution and an explicit artifact
policy. The default developer policy uses the last successful build; a missing
or stale artifact produces an actionable error. Rebuilding is explicit
(`run --build`).

The existing birnam <file> path remains a direct script mode and does not
silently acquire project build semantics.

### 2.5 Runtime authority is unchanged

Build output is a deployment input. Once loaded, compatible compiled dylibs and
images remain the executable authority described by design 45. The project
commands do not make source files or the manifest a competing runtime dispatch
authority.

### 2.6 One output directory per target identity

Build artifacts are separated by target, ABI, optimization/profile, and project
resolution identity. An arm64 macOS artifact cannot silently satisfy a different
target or source revision.

### 2.7 Build receipts stop at the source boundary

`resolution.toml` and `artifacts.toml` describe one local source build. They let
build/run/install prove freshness, checksums, and the exact publication being
selected; neither file is installed or consulted by the runtime module loader.
Compilation embeds runtime identity, requirements, target/ABI, capabilities,
and source/revision identity in every module artifact. After selection,
inspection and activation use that descriptor through the ID interface.

## 3. Command contract

### 3.1 birnam check [PROJECT] — implemented

The command finds or explicitly selects a project, resolves its source graph,
validates static module declarations and dependencies, prints the deterministic
dependency-first order, and compile-checks every logical module. Compiler output
is written only beneath an OS temporary directory and removed before exit.

`--format=human` is the default. `--format=json` emits one versioned
`birnam-check-v1` object containing `ok`, project/manifest/resolution identity,
the checked-module count, and ordered diagnostics. Each compiler diagnostic
contains a stable kind, message, complete detail, source path, and one-based
line/column when the compiler provides them. Resolver failures use the same
schema and diagnostic vocabulary.

It does not write build artifacts, load a project module into the current
process, or run main.

### 3.2 birnam build [PROJECT] — implemented

Resolve and check the project, execute every `[[wrapped]]` build argv selected
from the local project-dependency closure in dependency-first order and private
staging, compose each selected module entry and ordered source parts, and
compile each logical module once to an installable
libbirnam_<name>.dylib, record native and Birnam artifact checksums, write the
resolution record, and publish the complete result. Wrapped commands run from
their declaring project root and must use `{output}` for the expected
`lib<name>.dylib` path; they are not passed through a shell. `{lib}` exposes
already-staged dependency libraries to later recipes. Duplicate wrapped names
across projects are rejected before command execution.

The layout is:

~~~text
.birnam/
  build/
    aarch64-macos/
      <profile>/
        lib/
        resolution.toml
        artifacts.toml
~~~

The implementation supports the current target and `debug` and `release`
publication profiles. It
builds resolved source modules in dependency-first order, reusing only
Birnam artifacts whose target/profile/tool/ABI, ordered source-path metadata,
source digest, dependency metadata, expected path, and on-disk checksum all match
the previous successful
publication. A changed or tampered module invalidates its dependents; unrelated
matching modules may be copied into the new staging directory. It writes both
records before publication and retains the previous published directory when
resolution or compilation fails. Publication uses a unique staging directory
and rename/rollback transaction. `run` and `install` now validate and select
from this publication; build itself never activates it.

The directory name is an implementation detail; the invariant is that the
published directory contains a complete set of self-describing module artifacts
and any native siblings they declare. Native outputs are published as
`lib/lib<name>.dylib`; project modules are loaded from their exact selected paths
while Alien resolves their native requirements in the project `lib/`. Runtime
modules remain rooted in the Birnam installation. Native commands currently run
on each explicit build, while their published checksums are validated before a
run.

The current TCC backend emits the same compilation mode for both profiles.
`release` is nevertheless a distinct publication/cache/validation identity, so
future optimizer work cannot accidentally reuse debug outputs. The CLI and
records do not claim optimization that the backend does not yet perform.

### 3.3 `birnam install [PROJECT]` — implemented

Install consumes the selected successful project publication, or performs an
explicit build first with `--build`. It validates the artifact record and copies
every `lib/libbirnam_<name>.dylib` and `lib/lib<name>.dylib` from that publication
into the runtime library directory. The destination is `$BIRNAM_LIB_DIR` when
set, otherwise the installed executable's sibling `lib/` directory. Staging is
complete before installed filenames are replaced, so a failed copy does not
leave a half-written artifact. Install copies no `birnam.toml`,
`resolution.toml`, or `artifacts.toml`; installed identity and closure come from
each dylib's descriptor. `--profile debug|release` selects the publication;
debug is the default.

### 3.4 birnam run [PROJECT] [-- args...] — implemented

Resolve the project, validate the requested profile's build, load its compiled artifacts
in dependency order, evaluate the project entry source exactly once, and exit
by default. `run --build` performs the staged build first. A project run does
not enter a REPL unless its manifest or command line explicitly requests one.

The manifest may select the post-main interaction mode:

~~~toml
[run]
repl = "none"       # none, tty, or socket; none is the default
socket_bind = "127.0.0.1"
socket_port = 4400
~~~

`tty` keeps the process connected to its terminal and starts the normal
interactive REPL after `main`. `socket` starts the existing socket REPL after
`main`, binding to the configured IP/host and port. For example,
`127.0.0.1` or `localhost` keeps it local, while `0.0.0.0` binds all IPv4
interfaces. Binding beyond loopback exposes the unauthenticated socket REPL
to that network and must be an explicit choice. The first slice reports the
configured bind and port before blocking; reporting the OS-assigned port for
`socket_port = 0` is deferred until the socket layer exposes the bound address.

The equivalent command-line controls are available before the `--` separator:

~~~text
--no-repl
--repl
--repl=tty
--repl=socket
--repl-bind <ip-or-host>
--repl-port <port>
~~~

Command-line settings override the manifest; omitted settings use the manifest
and then the `none` default. Arguments after `--` are installed as the
`ProjectArgs` Vector global before `main` and are not interpreted as Birnam
project options.

`--build` is an explicit convenience that performs a successful build before
running. It does not weaken build atomicity or resolution diagnostics.
`--profile debug|release` selects an independent publication; debug is the
default.

The process receives arguments after -- without interpolation through Birnam
source. The project root and selected build are available to diagnostics and
runtime configuration but do not change the process cwd. In-image language tools
can ask the existing read-only `Project snapshotForUri:` bridge for an empty URI;
during `birnam run` that request materializes the already-resolved active project,
including its selected `buildRoot` and `profile`,
instead of resolving again from the unchanged process cwd. Explicit non-empty URIs
retain ordinary resolver behavior.

### 3.5 birnam test [PROJECT] — implemented

This command resolves the same project and validates the selected publication,
or performs an explicit build with `--build`. Tests may be declared in source:

~~~toml
[test]
sources = ["tests/app-test.birnam", "tests/http-test.birnam"]
~~~

When that table is absent, every `.birnam` file beneath root-project `test/` and
`tests/` directories is discovered in deterministic path order. An explicit
empty list deliberately selects no tests and is reported as
`projectTestsNotFound`.

Each test source runs in a fresh Birnam subprocess. The worker loads the compiled
project closure, evaluates only that test source, requires it to register at
least one `TestSuite`, then runs the registered suites through `ConsoleRunner`.
The process boundary prevents module dispatch, Store facts, globals, and test
runner state from leaking between sources. The parent runs all sources, reports
each result, and exits non-zero if any worker fails. `--profile` selects debug or
release; `--build` publishes that profile before workers begin.

## 4. Local build receipt

Each successful build publishes a value-like record containing:

~~~text
project identity and root
resolver/schema version
target and optimization profile
resolution digest
ordered modules
ordered source paths per module
source revision/content digest per module
artifact path and checksum per module
logical wrapped library name, path, and checksum per native dylib
build timestamp and tool version for diagnostics
~~~

The timestamp is diagnostic metadata and does not affect the resolution digest.
The record is not a second dependency graph or runtime manifest; it is the
materialized source-to-output result of design 51 and stays inside the local
build directory.

An artifact is reusable only when the project resolver can prove that its
recorded source, dependency, target, and checksum identity still match. Build
turns a cache miss into a fresh artifact; `run` turns a missing or stale
selected artifact into an explicit diagnostic. Neither path silently uses an
old program for changed source.

## 5. Boundary with live images and compiled units

Project builds produce normal self-describing compiled modules. A source-project
run uses its receipt to validate/select paths, then the same ID artifact protocol
as an installed root inspects the complete closure before activation. Project
builds do not implicitly mutate a running image. A user may explicitly save an
image or use the retained
lower-level `CompiledUnit` candidate mechanism:

~~~text
build → Module load: / CompiledUnit candidate → save image
~~~

Project commands provide source selection and build validation; they do not
reimplement artifact inspection, candidate activation, or rollback semantics.

Project-specific image/baseline commands are intentionally not duplicated here.
Full-image save and `LiveModules` baselines already have explicit durability
boundaries; project commands remain artifact workflow and test orchestration.

## 6. Diagnostics and exit status

Human output should be concise and actionable:

~~~text
error: module #http requires #socket
  from: src/app.birnam:1
  searched: ../birnam-http, .birnam/build/aarch64-macos/debug/lib
  help: add a local dependency or build the provider module
~~~

Machine output uses `birnam-check-v1` and the diagnostic vocabulary and source
locations from the resolver/compiler. Exit status is non-zero for resolution,
compile, publication, or test failure; usage/help remains successful as it is
today.

Build logs should identify whether each module was compiled, reused from a
matching artifact, or rejected as stale. This makes the stale-dylib authoring
gotcha observable instead of mysterious. For a composed module, the log and
resolution record include the entry followed by its ordered source parts.

## 7. Implementation slices

### P0 — check and materialized build — implemented

Implement check and build for the current source graph and local path dependency
shape. Use the existing Codegen compile-only primitive, stage outputs, write
`resolution.toml` and `artifacts.toml`, reuse validated artifacts with
dependency-aware invalidation, and add failure-preservation tests.

### P1 — project run — implemented

Run consumes a validated build directory, passes project arguments safely,
evaluate main once, and support no-REPL, tty-REPL, and socket-REPL modes from
the manifest and command flags. Direct file invocation and bare-configured REPL
behavior remain independent. Reporting the assigned endpoint for socket port
zero belongs to the socket layer rather than this project workflow.

### P2 — profiles and workspace integration — implemented

Debug and release select independent publications. During a project run, the
read-only `Project snapshotForUri: ""` bridge exposes `buildRoot` and `profile`
beside the already-resolved project root. Emacs build/run/test commands pass the
user-selectable `birnam-project-profile`. Local path dependencies remain part of
the resolver/build slice in design 51.

### P3 — isolated project tests — implemented

Project test discovery, explicit source lists, fresh-process execution, suite
registration, aggregate status, and profile selection are implemented. Images
and live-module baselines retain their existing explicit APIs rather than
acquiring duplicate project subcommands.

## 8. Acceptance scenarios

- birnam check from a nested project directory finds the manifest and never
  creates a published artifact.
- `birnam check --format=json` emits `birnam-check-v1` for both successful
  checks and resolver/compiler failures, including compiler source locations.
- A failed check or build leaves the previous successful build byte-for-byte
  usable.
- A build compiles dependencies before dependents and emits one complete
  resolution record.
- A successful build records each module's source/revision digest and artifact
  checksum.
- A module declared with an entry and multiple source parts produces one dylib
  and executes every part when that dylib is loaded.
- A repeated unchanged build reports all modules as reused.
- Changing a leaf module recompiles the leaf and every dependent while an
  unrelated module is reused; tampering with the leaf artifact has the same
  result.
- birnam run works from outside the project root and does not change cwd.
- birnam run --build is equivalent to a successful build followed by run, with
  no partially published intermediate state.
- Debug and release builds publish and validate independently; selecting one
  never falls back to the other.
- A default project run evaluates main once and exits without opening a REPL.
- Manifest or flag-selected tty/socket REPL modes start only after main; socket
  mode honors localhost, explicit IPv4 bind addresses, and the selected port.
- Arguments after `--` are available through `Globals at: #ProjectArgs` before
  main runs.
- Explicit or conventionally discovered test sources run in deterministic order
  in fresh processes, load the selected compiled project closure, and aggregate
  failures without sharing Store or dispatch state.
- A project can still use the existing birnam <file>, --rebuild, and live worker
  commands without passing through the project workflow.
- Installed and unpacked bundles can execute check, build, run, and isolated
  project tests using their own runtime resources rather than checkout-relative
  paths.

## 9. Explicit non-goals

- no implicit network dependency fetching; only an explicit `birnam fetch`
  may populate commit-pinned Git dependency checkouts;
- no background watch/build daemon;
- no automatic live activation after build;
- no replacement of direct script or image invocation;
- no package registry or lockfile until local dependency resolution is proven;
- no promise that arbitrary top-level initializers become transactional.
