# 54 — Relocatable Bundles and Runtime Distribution

> **Status:** Implemented through P3, 2026-07-22. P0 and the
> compiled-production-module P1 landed 2026-07-21; self-describing inspection,
> pre-activation closure validation, and source-free project roots landed
> 2026-07-22. The 2026-07-23 Rust-floor cutover extends the same fixed layout
> with complete Rust floor sources and the separately named Zig oracle.
> This document completes the distribution
> side of [51 — Project Manifests and Dependency
> Resolution](51-project-manifests-and-dependency-resolution.md), [52 — Project
> Check, Build, and Run](52-project-check-build-and-run.md), and [53 —
> Project-Aware Language Tools](53-project-aware-language-tools.md).

Birnam uses one conventional executable-relative runtime layout. Every
executable artifact is self-describing through a read-only ID protocol. Every
production Birnam module ships as a compiled-unit dylib containing its identity,
requirements, source, and reflection metadata. Loose `.birnam` files are
repository build inputs, not runtime resources. The layout is code, not an
external configuration format: it does not vary between the local install prefix
and a release archive, so a resource manifest would add another interface and
failure mode without adding information.

## 1. The question

Birnam's runtime is intentionally layered. A usable installation needs:

- the siphon executable;
- kernel and self-hosted dylibs;
- compiled-unit CLI, REPL, LSP, documentation, and library modules;
- Rust-floor, Zig-oracle, and guide sources for documentation/recovery;
- project-built modules, images, or isolate baselines supplied by a project.

The goal is a bundle that can be unpacked or moved anywhere and still support
the documented CLI, editor server, project commands, images, and workers without
reaching into the source checkout.

## 2. Governing invariants

### 2.1 Artifacts are self-describing

`birnam.toml` is the authored project declaration. Resolution combines its
selected roots and origins with the `Module named:requires:` declarations in
source. Compilation projects that information into the artifacts themselves.
An installed runtime or application does not carry or consult a second TOML
manifest.

A compiled module exposes an inert descriptor containing at least:

~~~text
kind                #module
identity            canonical module identity
artifactAbi         Birnam compiled-artifact ABI
target              architecture and operating system
roots               identities by which this artifact may be entered
requires            canonical compiled-module dependencies
nativeRequires      native libraries required by the module
capabilities        interfaces the artifact can provide after activation
sourceDigest        identity of its embedded authored source
revisionDigest      identity of its executable module revision
~~~

The descriptor is read through an ID message and returned as ID values. Reading
it must not run the compiled unit initializer, evaluate its entry point, install
methods, load its dependencies, or mutate the image. Images expose the
corresponding image/runtime/base identity through the same conceptual protocol;
their existing checked header remains the physical representation.

The ID-floor interface exposes scalar fields directly and list fields through
`rootsCount`/`rootAt:`, `requiresCount`/`requireAt:`,
`nativeRequiresCount`/`nativeRequireAt:`, and
`capabilitiesCount`/`capabilityAt:`. These messages require only `libid` and the
artifact inspector. Once Collection is active, `roots`, `requires`,
`nativeRequires`, and `capabilities` provide Vector conveniences over the same
data. The siphon therefore walks and rejects the complete closure without first
loading Store, Codegen, Module, or artifact code.

The descriptor is not a checksum manifest. Installed integrity comes from the
platform artifact and release-signing boundary. During development,
`artifacts.toml` remains an internal, generated build receipt used for
incremental reuse and source-to-output validation. It is not installed, is not
runtime authority, and is not an input to module loading.

### 2.2 The siphon loads mechanism on demand

The executable is a siphon, not a launcher with knowledge of every artifact
kind. `birnam <whatever>` first loads only the ID floor and the smallest
inspector needed to ask the input what it is and what it requires. It then
recursively inspects the requirements, rejects an incompatible closure, and
activates the dependency-first plan. CLI, REPL, LSP, project tools, images, and
applications are ordinary artifact roots and bring in only their declared
requirements.

The governing sequence is:

~~~text
locate root
  -> inspect through ID without activation
  -> reject wrong kind, identity, target, ABI, or unavailable requirements
  -> recursively inspect the minimal dependency closure
  -> activate dependency-first
  -> hand control to the selected root
~~~

The siphon may recognize transport-level facts needed to reach the protocol,
such as an image magic header or a dynamic-library container. Artifact-specific
policy belongs behind ID messages, not in a growing host-side switch.

### 2.3 The runtime has one layout

Every required runtime resource is at a fixed path relative to the executable.
The build install, release archive, host resolver, and acceptance tests share
that contract. Production module identity, dependency metadata, executable code,
and source/reflection metadata travel together in one compiled unit. There is no
launcher, runtime resource manifest, loose production-module source tree, cwd
lookup, or checkout fallback.

### 2.4 Resolution is deterministic and visible

Ordinary lookup derives one prefix from the running executable. The narrow
`BIRNAM_LIB_DIR` and `BIRNAM_ID_DYLIB` overrides remain for tests and advanced
embedding. A missing required resource reports its class and exact resolved
path. There is no module-source-directory override because shipped modules are
resolved through the compiled-unit loader.

### 2.5 Runtime and project resources are separate

The Birnam runtime contains the language and supported tools. A project owns its
manifest, source, dependencies, build artifacts, images, baselines, and native
wrappers. Installing Birnam does not make a global module name satisfy a project
dependency without resolution proof.

### 2.6 Compatible compiled code remains primary

Bundling does not change image or compiled-unit authority. A compatible image
maps its compiled code. `--rebuild` is the explicit source-recovery path; the
host never silently recompiles an image or substitutes checkout code.

### 2.7 Optional integrations are complete or absent

LSP, documentation, REPL, and the core compiled tools are required. An optional
C wrapper is not copied merely because a developer happened to build a dylib in
`wrapped/`. Its compiled Birnam wrapper is the named root: it embeds its source,
declares the native library it requires, and participates in the same inspection
and activation protocol as any other module. The native library must be present
and compatible before the wrapper is activated. A selected but unavailable
implementation produces an explicit fallback or diagnostic. Project-owned
`[[wrapped]]` build recipes remain authored in `birnam.toml`; their generated
native products are described by the compiled wrapper that consumes them.

### 2.8 Bundle publication is atomic

Release staging uses a fresh temporary prefix and staging directory. Only a
fully built, signed, relocated-smoke-tested archive moves into `zig-out/dist`.
A failed build, copy, sign, or smoke cannot leave a mixed-generation archive.

## 3. Bundle layout

~~~text
birnam-<version>-<target>/
  bin/
    birnam
  lib/
    libbirnam_*.dylib
  share/birnam/
    src/id/*.zig
    docs/guide/*.md
~~~

The executable's parent is the prefix. `lib/` and `share/birnam/` are therefore
relocatable as a unit. Production `.birnam` source is embedded in its compiled
unit for reflection, documentation, and explicit recovery; it is not duplicated
under `share/`. Version, target, dependency, and capability identity live in the
executable and compiled/image artifact formats where they are actually consumed;
the fixed directory layout does not duplicate them in a sidecar file.

## 4. Resource lookup

The intended precedence is:

1. exact `BIRNAM_ID_DYLIB` for the id-floor dylib;
2. `BIRNAM_LIB_DIR` for compiled units and runtime dylibs;
3. the fixed prefix derived from the running executable.

The host resolves these through `src/id/runtime_resources.zig`. Production
module dependencies resolve by canonical identity and self-description through
the same library directory. The loader asks the artifact for an ID descriptor,
checks that the returned identity matches the requested name, recursively
inspects `requires` and `nativeRequires`, and only then activates it. No ordinary
installed path probes the current working directory or source checkout.

## 5. What is shipped

### Required runtime closure

The release includes everything needed for:

~~~text
birnam --help / --version
birnam
birnam <file.birnam>
birnam <file.image> / --rebuild <file.image>
birnam doc / docgen
birnam --lsp
birnam init
birnam check / build / run
isolate worker startup from an image
~~~

The install smoke derives the production module closure from the build contract
and asserts its compiled-unit dylibs exactly. It also proves that no loose
production `.birnam` tree is installed. Tests, benchmarks, and development
fixtures remain source files in the repository and do not ship.

### Optional closure

No optional first-party wrapper is currently part of the runtime bundle. An
optional implementation becomes distributable only when its compiled wrapper
self-describes a complete, deterministic closure and the corresponding native
artifact is deliberately staged and signed. Release staging never discovers
optional integrations from ignored developer build output.

### Project closure

Project dependencies and native wrappers are not part of the Birnam runtime.
The source project uses `birnam.toml`; its local build uses the generated
`artifacts.toml` receipt to validate source-to-output freshness. A distributable
or installed application does not carry that receipt. A selected compiled root
identifies its module activation entry and requirements; application entry
behavior must be part of that root's top-level program. Alternatively, an image
contains its application entry. A separate `main.birnam` path named only by the
source manifest is intentionally not recoverable from an installed dylib. In
either artifact shape, self-described requirements recursively identify the
installed closure while the runtime supplies the language floor and tools.

This distinction is strict:

~~~text
birnam.toml       authored source intent
artifacts.toml    disposable project-build receipt
embedded descriptor installed runtime authority
~~~

## 6. Images and isolate baselines

The relocated install gate now proves that the same moved runtime can:

- save and mapped-boot an image;
- explicitly recover it through `--rebuild`;
- start an isolate worker from that image and exchange a copied value.

The image header and its mapped-code records already carry structural,
architecture, byte-integrity, and generation-authority checks. Distribution
uses those identities before mapping, then resolves any process-local host
capability through the same compiled-artifact protocol. A baked executable still
depends on adjacent runtime `lib/` and `share/` resources; it is not by itself a
standalone distribution.

## 7. Release and install workflow

### P0 — fixed resolver and clean-root tests — implemented 2026-07-21

Centralize resource resolution behind one executable-relative module, retain
only narrow explicit overrides, and run installed commands from an unrelated
working directory. An early external `runtime.toml` attempt was deleted because
the layout does not vary and the file added no information.

### P1 — compiled production-module closure — implemented 2026-07-21

Compile every production module—including CLI, REPL, LSP, docs, and their
dependencies—into an installable compiled unit containing its source metadata.
Load these tools by canonical module identity, remove loose module installation
and `BIRNAM_MODULES_DIR`, and keep source files only as repository build inputs.
The relocated smoke must run docs, LSP, REPL, project check/build/run/test,
mapped and rebuilt images, and an isolate worker using only the compiled closure. Negative
cases remove a required compiled unit or id dylib and require diagnostics
containing the exact resolved paths.

The ordinary install step builds this closure automatically through a distinct
native bootstrap-install dependency, so `zig build` and `zig build install` do
not produce a host missing its tools. Project artifacts carry their own runtime
description; their local build receipt is used only to decide whether an output
may be reused. They do not replace the runtime module root. An
image that lacks Repl may install the trusted Repl dylib as a process-local host
capability after recovery, without adding it to the image's sealed application
generation.

### P2 — self-describing optional integrations — implemented 2026-07-22

The dependency-only string probe is replaced by the inert ID descriptor. Embed
module identity, ABI, target, roots, module requirements, native requirements,
capabilities, and source/revision identities. Reject a descriptor that does not
match the requested module before activation. Make configuration selection fail
or fall back explicitly when a selected native implementation is absent. Sign
and check every deliberately shipped dylib before archive publication.

### P3 — source-free project/image distribution — implemented 2026-07-22

Keep `birnam.toml` at the source boundary and `artifacts.toml` inside the local
build boundary. Do not install either as runtime authority. A compiled project
root describes its own module activation entry and closure through the artifact
protocol; an image additionally carries an application entry. Reuse the runtime
resolver and image identity checks, without a second launcher, installed
manifest, or resource-layout convention.

## 8. Acceptance scenarios

- Inspecting a compiled unit returns an ID descriptor without running its
  initializer or entry point.
- Loading a root recursively inspects and activates only its declared closure.
- A requested-name/descriptor-identity mismatch, wrong artifact ABI, wrong
  target, or unavailable native requirement is rejected before activation.
- Selecting an unavailable optional implementation produces an explicit
  fallback or diagnostic.
- Installing a project copies no `birnam.toml`, `resolution.toml`, or
  `artifacts.toml`; its compiled modules remain loadable from embedded identity.
- A distributable image rejects an incompatible runtime before mapping code.
- An isolate baseline participates in documented replacement/rollback using
  only its self-described application closure and the fixed runtime layout.

## 9. Explicit non-goals

- no installer daemon or auto-update service;
- no universal multi-platform bundle before the platform support boundary moves;
- no implicit download of project or runtime dependencies;
- no checkout probing from an installed release;
- no external configuration for a runtime layout that does not vary;
- no installed project/build receipt as runtime authority;
- no duplicate loose source for a shipped compiled production module;
- no promise that arbitrary third-party native libraries are portable merely
  because their wrapper source is present.
