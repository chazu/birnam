# 54 — Relocatable Bundles and Runtime Distribution

> **Status:** Proposed, 2026-07-14. This document makes installed and release
> Birnam behave like the checkout used to develop it. It completes the project
> workflow with [51 — Project Manifests and Dependency
> Resolution](51-project-manifests-and-dependency-resolution.md), [52 — Project
> Check, Build, and Run](52-project-check-build-and-run.md), and [53 —
> Project-Aware Language Tools](53-project-aware-language-tools.md).

Current release staging is defined by scripts/dist.sh: a launcher, kernel and
self-hosted dylibs, and source-backed tools under share/birnam. This proposal
turns that convention into an explicit resource contract.

## 1. The question

Birnam's runtime is intentionally layered. A usable installation may need:

- the siphon executable;
- kernel and self-hosted dylibs;
- source-backed CLI, REPL, LSP, documentation, and test modules;
- Zig-floor and guide sources for documentation/recovery;
- optional wrapped C libraries and their Birnam wrapper sources;
- project-built modules, images, or isolate baselines.

Today the release script stages most of this correctly, but resolution still
depends on environment variables, executable-relative probes, and optional
checkout paths. The goal is a bundle that can be unpacked anywhere and still
support the documented CLI, editor server, project commands, images, and live
worker examples without accidentally reaching back into the source checkout.

## 2. Governing invariants

### 2.1 A bundle is self-describing

Every runtime resource used by a supported installed command is either inside
the bundle or explicitly supplied by the user. The bundle records its runtime
root, resource schema/version, target identity, and artifact checksums.

### 2.2 Resolution is deterministic and visible

Resource lookup follows one documented precedence order. A development checkout
may opt into checkout-relative paths; an installed or unpacked bundle must not
silently fall through to the current working directory or a sibling checkout.

### 2.3 Runtime and project resources are separate

The Birnam runtime bundle contains the language and its supported tools. A user
project owns its manifest, source, dependencies, build artifacts, images, and
baselines. Installing Birnam does not copy a project's dependencies into the
global runtime or make a global module name satisfy a project dependency without
resolution proof.

### 2.4 Compatible compiled code remains primary

Bundling does not change image or compiled-unit authority. A compatible image
maps its compiled code; a missing/incompatible resource produces an explicit
error or invokes the documented source-recovery path. The launcher never silently
recompiles an image or swaps in arbitrary checkout code.

### 2.5 Optional integrations are truly optional

LSP, documentation, REPL, and core source-backed tools are part of the supported
bundle. Optional C wrappers such as linenoise are either fully staged with their
artifacts and source or clearly reported unavailable. A config entry must not
select an implementation that was not shipped.

### 2.6 Bundle publication is atomic

Release staging, installation, and future self-updates publish complete resource
sets. A failed copy, code-sign, checksum, or archive operation cannot leave a
bundle that looks complete but boots against mixed-generation artifacts.

## 3. Bundle layout

The proposed logical layout retains the current paths:

~~~text
birnam-<version>-<target>/
  bin/
    birnam              # launcher
    birnam.bin          # real executable
  lib/
    libbirnam_*.dylib
    optional wrapped dylibs
  share/birnam/
    runtime.toml        # resource manifest and schema version
    modules/*.birnam    # CLI, LSP, docs, REPL, libraries
    src/id/*             # floor/source-backed recovery and docs input
    docs/guide/*.md
    wrapped/*            # optional wrapper sources, when shipped
~~~

runtime.toml is a bundle description, not a project manifest. It records:

~~~toml
schema = 1
version = "0.10.0"
target = "aarch64-macos"
modules_dir = "modules"
lib_dir = "../../lib"
source_dir = "src/id"
guide_dir = "docs/guide"
~~~

The exact format can be generated rather than hand-authored. Paths are relative
to the bundle resource root and are normalized before use.

## 4. Resource lookup

The proposed precedence is:

1. an explicit command argument or resource override;
2. an explicit BIRNAM_HOME bundle/runtime root;
3. the executable's adjacent bundle/resource manifest;
4. an installed prefix discovered from the executable;
5. checkout-relative development paths, only when the executable is identified
   as a development build or the user explicitly opts in.

Existing BIRNAM_LIB_DIR, BIRNAM_MODULES_DIR, and BIRNAM_ID_DYLIB remain
supported escape hatches for tests and advanced embedding. When they override a
bundle resource, diagnostics should say so.

The launcher is responsible for establishing the runtime root before the real
binary starts. The Birnam host and source-backed modules then consume resolved
paths rather than independently probing for modules/ or lib/.

## 5. What is shipped

### Required runtime closure

The release must include every resource needed for:

~~~text
birnam --help / --version
birnam
birnam <file.birnam>
birnam --rebuild <image>
birnam doc / docgen
birnam --lsp
birnam init
birnam check / build / run
~~~

That includes the source-backed cli, repl, lsp, docs, file, JSON, and other
modules those commands load, plus the compiled dylibs and the floor sources used
by documentation and explicit recovery.

### Optional closure

An optional integration is a named closure:

~~~text
implementation name
wrapper source
compiled wrapper artifact
native library dependencies
target/checksum metadata
~~~

For example, a linenoise implementation is either staged as a complete closure
or the config resolver reports that it is unavailable and uses the supported
fallback. The release script must not copy a wrapper source while omitting the
library it opens.

### Project closure

Project dependencies are not part of the Birnam runtime bundle unless explicitly
vendored by the project. A built project can point its BIRNAM_LIB_DIR or
resolver output at its own complete artifact directory, while the runtime bundle
supplies the language floor and source-backed tools.

## 6. Images and isolate baselines

An image or isolate baseline is portable only relative to the runtime contract
it records. The save/build record should identify:

- Birnam image format and generation format;
- target/ABI/platform;
- required runtime/base digest;
- embedded compiled-unit checksums;
- whether mapped boot or source recovery is expected.

Launching an image from another bundle validates this identity before mapping
code. --rebuild remains an explicit recovery choice, not a hidden fallback for
an incompatible bundle. Isolate start and rolling replacement consume the image
and runtime resources from the same resolved environment, with failures reported
before a worker is selected.

## 7. Release and install workflow

### P0 — resource manifest and clean-root tests

Generate runtime.toml, centralize path resolution, and run all installed smoke
tests from an empty working directory with BIRNAM_* overrides unset.

### P1 — complete source-backed closure

Verify that the installed/release tree contains the actual REPL, LSP, docs, and
project-command sources. Add tests that run each feature after relocating the
bundle and after removing the checkout from the filesystem.

### P2 — optional integrations and checksums

Stage optional wrappers as complete closures, record checksums, and make config
selection fail or fall back explicitly. Sign/check every dylib before archive
publication.

### P3 — project/image distribution

Add a documented project bundle or application layout that carries a project
build record and optional image/baseline. Reuse the runtime resolver and image
identity checks; do not create a second launcher convention.

## 8. Acceptance scenarios

- Unpacking a release tarball into /tmp/a and then moving it to /tmp/b does not
  change birnam --lsp, docgen, REPL, or project command behavior.
- Those commands work from an unrelated cwd with an empty environment except for
  PATH.
- Removing the checkout after staging does not break any required installed
  command.
- A missing required resource names the resource class and resolved search path;
  it does not emit a confusing parser or DNU failure.
- Selecting an unavailable optional module implementation produces an explicit
  fallback or diagnostic.
- A bundle containing a stale or target-incompatible dylib refuses it before
  image/module activation.
- A compatible image maps from the relocated bundle; --rebuild explicitly
  recovers from its recorded source.
- An isolate baseline starts and rolls back using the bundle's runtime resources
  without depending on the build checkout.
- A failed release staging or signing step leaves no archive claiming to be a
  valid release.

## 9. Explicit non-goals

- no installer daemon or auto-update service;
- no universal multi-platform bundle before the platform support boundary moves;
- no implicit download of project or runtime dependencies;
- no checkout probing from an installed release;
- no promise that arbitrary third-party native libraries are portable merely
  because their wrapper source is present.
