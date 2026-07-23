# Birnam

Birnam is a late-bound, message-oriented language system built on a Piumarta-style
object floor. It combines a small reflective runtime, a relational store, PEG
parsing, native compilation through libtcc, self-hosted library code, and durable
images.

It is an active pre-alpha system. The architecture and its bootstrap are the
product; compatibility and language surface can and will still change.

## Support

Birnam currently supports **arm64 macOS** with **Zig 0.15.1**. This applies to
development builds, installed binaries, and release bundles. Other targets fail
early rather than producing an artifact that cannot link or boot.

Linux support is planned.

See [Platform support](docs/misc/PLATFORM-SUPPORT.md) for the porting boundary
and acceptance criteria.

## Install

From a checkout:

```sh
zig build install --prefix ~/.local
export PATH="$HOME/.local/bin:$PATH"
birnam
```

The installed executable finds its runtime dylibs in the sibling `lib/`
directory. For development, `zig build` produces `zig-out/bin/birnam` and the
runtime modules under `zig-out/lib/`.

[`mise`](https://mise.jdx.dev) is optional; [mise.toml](mise.toml) pins the same
Zig version and exposes `mise run clean`, `mise run build`, `mise run verify`,
and `mise run install`. Use `mise run clean && mise run build` when you need a
fresh local build, including fresh project artifacts.

## First Session

Run `birnam` with no arguments to enter the console REPL:

```text
birnam> [3 + 4]
7
birnam> let n := 41
41
birnam> [n + 1]
42
```

Top-level message sends use brackets. Bindings and method definitions persist
for the life of the running image. The console commands are deliberately small:
`:help`, `:globals`, and `:quit` (with `:h`, `:g`, and `:q` aliases).

Run a source file directly:

```sh
birnam examples/fizzbuzz.birnam
```

For a project, `birnam` discovers the nearest `birnam.toml` from the current
directory upward and validates its latest successful project build. Project
modules are loaded from their exact published artifact paths; the runtime's own
modules remain rooted in the installed runtime `lib/`, while project-native
libraries resolve beside the project artifacts. It loads the
`load = ["module"]` list dependency-first, then runs a root-relative
`main = "main.birnam"` before the REPL opens. For an explicit project run, use
`birnam run`: it evaluates `main` once and exits by default, or can start a
tty/socket REPL through the manifest's `[run]` table or command flags.
`birnam init` writes a commented starter file.

`birnam check [directory]` resolves the nearest source project and compiles each
module into a private temporary directory without loading, evaluating, or
publishing it. Human output includes the deterministic dependency-first order
and compiler diagnostics. `--format=json` emits the stable
`birnam-check-v1` result for tools. The directory defaults to the current
directory; use `birnam check --help` for the full contract.

`birnam build [directory]` compiles that same graph dependency-first into
`.birnam/build/<target>/<profile>`. It publishes `lib/`, `resolution.toml`, and
`artifacts.toml` together after a successful build; it does not run or activate
the project. The TOML files are internal build receipts for freshness, reuse,
and source-to-output validation; they are not installed and are not runtime
module metadata. A manifest can also declare `[[wrapped]]` C libraries; each
declaration runs its argv build commands with `{output}` expanded to a private
staging path, then publishes `lib/lib<name>.dylib` beside the compiled Birnam
wrapper. A logical Birnam module can be composed from multiple ordered source
files and is still emitted as one `libbirnam_<name>.dylib`:

```toml
[[module]]
entry = "src/json.birnam"
files = ["src/json/parser.birnam", "src/json/render.birnam"]
```

The entry owns the module declaration; the additional files are ordinary
Birnam source and are concatenated in the listed order. Use
`birnam build --help` for the current build contract.

Build, run, install, and test accept `--profile debug|release`; `debug` is the
default. Profiles have independent publications and validation records. The
current TCC backend emits the same native compilation mode for both; `release`
establishes a separate reproducible publication identity without claiming an
optimizer that Birnam does not yet have.

Source can also embed a project-relative file directly into the compiled
dylib:

```birnam
[System embedFile: "assets/schema.json"]
```

The path must be a literal and the result is a binary-safe `String`; the file
is read during compilation and is not needed when the dylib is loaded. Asset
contents participate in build invalidation, so changing the file recompiles
its owning module.

`birnam install [directory]` validates that project-local publication and copies
all of its `libbirnam_<name>.dylib` modules and `[[wrapped]]` native libraries
into the runtime library directory. The destination is `$BIRNAM_LIB_DIR` when
set, otherwise the installed runtime's sibling `lib/`; pass `--build` to build
before installing. It copies no `birnam.toml`, `resolution.toml`, or
`artifacts.toml`: each installed module carries its own identity, dependency,
ABI, target, native-requirement, capability, and source/revision description.

An installed compiled root can be selected directly, without its source project
or build receipt:

```sh
birnam /path/to/libbirnam_app.dylib
```

The siphon inspects the complete sibling closure through the ID-floor artifact
protocol before it loads the activation stack. See [Shipping compiled
artifacts](docs/guide/09-shipping-artifacts.md).

`birnam run [directory] [options] [-- args...]` consumes the last successful
build; add `--build` to rebuild first. The default is no REPL. Use
`--repl=tty` or `--repl=socket`, with `--repl-bind` and `--repl-port` for the
socket endpoint. Arguments after `--` are available to the entry source as
`Globals at: #ProjectArgs`.

`birnam test [directory]` validates the selected publication and runs every
project test source in a fresh Birnam process. Configure an explicit ordered set
with `[test].sources`, or omit it to discover `.birnam` files beneath `test/`
and `tests/`. Each source registers one or more `TestSuite` values; the worker
runs only those suites with `ConsoleRunner`, so mutable image and Store state
cannot leak between test sources. Pass `--build` to publish before testing and
`--profile` to select the build.

`birnam --help` is the authoritative command reference. The primary entry
points are:

```text
birnam <file.birnam>       Compile and run a script.
birnam <file.image>        Boot a saved image.
birnam <artifact.dylib>    Inspect and activate a self-described compiled root.
birnam --rebuild <image>   Boot an image by replaying source.
birnam --bake <image> <executable>
                             Bake an image into a self-booting executable.
birnam check [directory]     Resolve and compile-check a source project.
birnam check --help          Show check command documentation.
birnam build [directory]     Compile and publish a project-local build.
birnam build --help          Show build command documentation.
birnam install [directory]   Install a project's published modules into the
                             runtime library directory.
birnam install --help        Show install command documentation.
birnam run [directory]       Run a project main; default is no REPL.
birnam run --help            Show run, REPL, socket, and argument documentation.
birnam test [directory]      Run discovered project tests in fresh processes.
birnam test --help           Show test discovery and profile documentation.
birnam wrap <name>         Scaffold a C-library wrapper.
```

## Images

An image makes a live system durable. Its compiled dylib units are the primary
executable layer; store facts and source are the reflective/meta-level surface
and explicit recovery material. A normal boot maps compatible compiled code;
`--rebuild` deliberately recovers from recorded source instead.

```sh
birnam examples/image.birnam
birnam /tmp/birnam-demo.image
birnam --rebuild /tmp/birnam-demo.image
```

Inside birnam, save with `[System saveImage: path]`. A global `main` block makes
an image an application entry point; [examples/app.birnam](examples/app.birnam)
shows saving and baking one.

Use `[System saveImage: path]` to create a worker image. The same image can be
passed directly to `Isolate start:selector:image:`. See the
[image guide](docs/guide/04-images.md).

Persistence is intentionally not a claim that every process value can be
serialized. Saving is atomic and rejects durable values without either a
portable encoding or a registered recovery path; inspect
`[System imageSaveReport]` after a rejected save. See
[The Image](docs/design/40-the-image.md) for the exact durability contract and
recovery paths.

## Staged Code Changes

`CompiledUnit` stages a native method patch so it can be inspected and mapped
without changing live dispatch, then activated as one generation and rolled back
exactly while its process-local rollback capability remains valid. Candidate
source is deliberately declarative: module metadata plus method definitions and
explicit removals. Ordinary `Module load:` remains available for initializers,
globals, FFI, and other immediate effects.

See [Staged compiled units](docs/guide/05-compiled-units.md) for the lifecycle,
failure and drift behavior, dependency boundary, and image semantics. The live API
is also available through `birnam doc compiledUnit` and selector lookups such as
`birnam doc compiledUnit activate`.

## Isolated workers

Run the blocking worker examples with:

```sh
bash examples/isolates/run.sh
```

See the [isolate guide](docs/guide/06-isolates.md). Generated API reference is
available through `birnam doc isolate`.

For interactive object and fact-store wayfinding, see the
[image exploration guide](docs/guide/08-exploring-the-image.md).

## Developer Tools

The compiled CLI, REPL, LSP, and documentation modules are included by
`zig build install` and in release bundles. Their source is embedded in the
installed compiled units. In a development checkout, use `zig-out/bin/birnam`.

```sh
./zig-out/bin/birnam doc time nowMillis
./zig-out/bin/birnam docgen /tmp/birnam-docs.html
./zig-out/bin/birnam --lsp
./zig-out/bin/birnam --listen 5555 examples/fizzbuzz.birnam
```

- `doc` prints module or selector documentation.
- `docgen` writes an HTML reference for birnam modules and native floor APIs.
- `--lsp` serves the language server over stdio.
- `--listen` starts a socket REPL, optionally after running a script.

The generated reference opens with the authored language, idioms, fact-store,
and image guide, followed by the API reference extracted from current module
facts and Zig source documentation.

`wrap` is a binary-only scaffolder and works from an installed executable:

```sh
birnam wrap mylib
```

It creates an Alien-based C-library wrapper under `wrapped/`. Add that source to
the project's `source_roots` and pair it with a `[[wrapped]]` build declaration
to have `birnam build` produce and stage its native dylib.

## Verification

Run the complete gate before a release or cross-layer change:

```sh
zig build verify
```

It runs the Zig unit/integration suites, rebuilds self-hosted artifacts with
`stage1`, executes the birnam-level suites, and checks REPL transcripts. The
narrower commands are `zig build test`, `zig build test-birnam`, and `zig build
test-repl`. GitHub Actions runs `verify` on the supported arm64 macOS target.

## Benchmarks

Performance is tracked per tagged release. The suite measures the send
constant and the native-vs-derived boundaries that gate self-hosting
migrations, the datalog read/write path, GC, images, boot paths, and
whole-process workloads — always against a ReleaseSafe build.

```sh
BENCH_QUICK=1 ./zig-out/bin/birnam bench/micro.birnam   # tight dev loop (~5s)
mise run bench                                          # isolated Zig-oracle run
mise run bench-rust                                     # isolated production-floor run
mise run bench-compare <old-floor.json> <new-floor.json>
```

When to run: the affected suite before and after any perf-relevant change
(dispatch, collections, strings, store, codegen); the full `mise run bench` +
a compare against the last release snapshot before merging bigger work.
`bench-compare` exits non-zero when a case regresses past its threshold
(10% micro, 5% elsewhere), so it can gate scripts.

**As part of the release process:** after `mise run release`, on the tagged
commit and a quiet machine, run

```sh
mise run bench-release
mise run bench-release-rust
git add bench/results/v<X.Y.Z>-*.json && git commit
```

The committed floor-labelled snapshots are the baselines future runs compare
against; filenames and JSON stamps include the commit and selected dylib digest.
(This step is intentionally outside `release.sh` — bench noise must never
block a release.) `docs/misc/BENCHMARKS.md` is the full operational guide;
[docs/design/46-benchmarks.md](docs/design/46-benchmarks.md) records the
methodology and the measured constants.

## Architecture

The runtime starts as a small Zig siphon that loads a set of reloadable dynamic
libraries through a C ABI. The production `libid` floor is Rust; a separately
named Zig oracle remains available for conformance and developer rollback.
Artifact selection first adds only the ID-floor inspector; activation then
brings up the required language rungs:

```text
libid -> Artifact inspector
      -> Store -> Peg -> Lang -> Tcc -> Codegen -> Prelude
      -> Collection -> Modules -> Error -> Relational -> Alien -> Image
```

`libid` provides the object model, allocation, dispatch, interning, and GC.
Higher rungs add the relational substrate, parser, compiler, native-code
backend, collections, FFI, and image format. The standard library and most
application-facing modules are Birnam source. Birnam retains a cold source
bootstrap through declared non-Birnam toolchains, with no prebuilt Birnam
artifact required; Design 61 has moved `libid` to Rust while Zig remains the
host for the siphon, higher native rungs, and cold compiler.

## Repository Guide

- `rust/birnam-id/` — authoritative Rust `libid` floor behind the frozen `IdApi`.
- `src/id/` — retained Zig `libid` oracle plus the Zig runtime rungs, siphon,
  compiler, image, FFI, and test suites above the floor.
- `src/id/*.birnam` — bootstrapped standard library, TOML, and configuration
  (`Config` for project manifests, `AppConfig` for application startup).
- `modules/` — authored library, CLI, LSP, documentation, and test sources;
  production modules compile into self-describing dylibs before distribution.
- `wrapped/` — first-party native libraries used by optional applications,
  including the Alien-backed Termbox2 adapter used by Logue.
- `examples/` — runnable programs, including images and AOT artifacts.
- `docs/design/` — current design decisions and implementation status.
- `docs/archive/` — superseded kernel plans and historical material.

## Design Notes

- [The destination](docs/design/30-the-destination.md) defines the architectural
  invariants and layering.
- [The image](docs/design/40-the-image.md) defines persistence and mapped-code
  behavior.
- [Compiled units](docs/design/45-compiled-units.md) defines executable
  authority, image integrity, recovery, and the next live-reload boundary.
- [Concurrency](docs/design/48-concurrency-reactors-and-isolates.md) and
  [live module evolution](docs/design/49-live-module-evolution.md) document
  withdrawn live-reload policy experiments; only the blocking single-worker
  isolate substrate is currently shipped.
- [Blocking process isolates](docs/guide/06-isolates.md) documents the shipped
  trusted-worker API, copied-value boundary, structured failures, and deadlines.
- [Modules and the self-hosted layer](docs/design/43-modules-and-config.md)
  explains source, artifacts, and loading.
- [Toward self-hosting](docs/design/44-toward-self-hosting.md) records the
  migration boundary: retain an independent cold source bootstrap and move only
  appropriate library surface into Birnam.
- [Project manifests and dependency resolution](docs/design/51-project-manifests-and-dependency-resolution.md)
  and the [project workflow](docs/design/52-project-check-build-and-run.md)
  document the implemented project workflow. The project-aware LSP has shipped
  its workspace/coordinate, safe static-diagnostics, and first source-navigation
  slices in [design 53](docs/design/53-project-aware-language-tools.md);
  dependency-wide indexing and richer semantic assistance are implemented.
  [Relocatable bundles](docs/design/54-relocatable-bundles.md) now ship a fixed
  executable-relative runtime closure with relocated project, image, and isolate
  acceptance plus self-describing, source-free project/application roots.
- [Shipping compiled artifacts](docs/guide/09-shipping-artifacts.md) explains the
  source manifest, disposable build receipt, installed descriptor, and direct
  dylib-root workflow.
- [Embedded files](docs/design/56-embedded-files.md) documents the
  compile-time `System embedFile:` intrinsic and its project-build contract.
- [Designs 55, 58, and 59](docs/design/59-digla-adversarial-architecture-review.md)
  preserve the retired Digla application experiment and its architectural
  lessons. Digla is not part of the current runtime, verification graph, or
  product roadmap.
- [Stackless native execution](docs/design/60-stackless-native-execution.md)
  assesses a parallel resumable-method ABI, explicit per-fiber frames,
  trampolining, and delimited continuations. It records a migration path and
  first proof, not an active implementation commitment.
- [The Rust libid rewrite plan](docs/design/61-rust-libid-rewrite-plan.md)
  specifies the drop-in floor boundary, unsafe/reentrancy discipline,
  commit-granular initial phases, dual-floor verification, performance gates,
  completed default cutover, and delayed Zig-oracle retirement.
- [The Rust native-rung migration plan](docs/design/62-rust-native-rungs-migration-plan.md)
  extends that work through the higher dynamic rungs, independent cold compiler,
  host, project workflow, and eventual Zig-toolchain retirement while preserving
  Birnam's bootstrap, MOP, artifact, image-v4, and execution-model invariants.
