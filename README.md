# Birnam

Birnam is a late-bound, message-oriented language system built on a Piumarta-style
object floor. It combines a small reflective runtime, a relational store, PEG
parsing, native compilation through libtcc, self-hosted library code, and durable
images.

It is an active pre-alpha system. The architecture and its bootstrap are the
product; compatibility and language surface can and will still change.

## Contents

- [Support](#support)
- [Install](#install)
- [Quick Start](#quick-start)
  - [First session](#first-session)
  - [Start a project](#start-a-project)
  - [Project workflow](#project-workflow)
- [Running Birnam](#running-birnam)
  - [Execution modes](#execution-modes)
  - [Terminal REPL](#terminal-repl)
  - [Socket REPL](#socket-repl)
  - [Language server](#language-server)
- [Images and Executables](#images-and-executables)
  - [Save an image](#save-an-image)
  - [Boot an image](#boot-an-image)
  - [Create an image application](#create-an-image-application)
  - [Bake an executable](#bake-an-executable)
- [Staged Code Changes](#staged-code-changes)
- [Isolated Workers](#isolated-workers)
- [Concurrency](#concurrency)
- [Wrapping a Dynamic Library with Alien](#wrapping-a-dynamic-library-with-alien)
  - [Scaffold and build the library](#scaffold-and-build-the-library)
  - [Define the wrapper](#define-the-wrapper)
  - [Signature and ownership rules](#signature-and-ownership-rules)
  - [Check, run, and install](#check-run-and-install)
- [Developer Tools](#developer-tools)
- [Verification](#verification)
- [Benchmarks](#benchmarks)
- [Architecture](#architecture)
- [Repository Guide](#repository-guide)
- [Design Notes](#design-notes)

## Support

Birnam currently supports **arm64 macOS** with the pinned Rust toolchain and
system libtcc build prerequisites. The runtime, build, install, release, and
verification paths are Rust-only.

Linux support is planned.

See [Platform support](docs/misc/PLATFORM-SUPPORT.md) for the porting boundary
and acceptance criteria.

## Install

From a checkout:

```sh
scripts/rust-install.sh install --root . --prefix ~/.local --profile debug
export PATH="$HOME/.local/bin:$PATH"
birnam
```

The installed executable finds its runtime dylibs in the sibling `lib/`
directory. `mise run build` uses the same Rust installer for a debug prefix.

[`mise`](https://mise.jdx.dev) is optional; [mise.toml](mise.toml) pins the Rust
toolchain and exposes the Rust tasks for `mise run build`, `mise run install`,
`mise run verify`, `mise run dist`, and `mise run release`.

## Quick Start

### First session

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

For the two pipeline forms and their stdlib vocabulary, run
[`examples/pipeline.birnam`](examples/pipeline.birnam) and
[`examples/combinators.birnam`](examples/combinators.birnam).

### Start a project

Create a directory and ask Birnam for a starter manifest:

```sh
mkdir hello-birnam
cd hello-birnam
birnam init
mkdir -p src tests
```

`birnam init` writes a commented, self-documenting `birnam.toml` and refuses to
overwrite an existing one. A minimal application can use:

```toml
name = "hello-birnam"
version = "0.1.0"
main = "main.birnam"
```

Create `main.birnam` beside the manifest:

```birnam
[System println: "hello from Birnam"]
```

Then build and run it:

```sh
birnam check
birnam run --build
```

Larger projects normally put declared modules below `src/` and load them before
the entry source:

```toml
source_roots = ["src"]
load = ["app"]
main = "main.birnam"
```

The entry file for that module begins with a declaration such as
`[Module named: #app requires: #[#stdlib]]`. Birnam discovers the nearest
`birnam.toml` from the requested directory upward, so commands may be run from
nested project directories.

### Project workflow

The normal source-project loop is:

```sh
birnam fetch                         # explicitly acquire pinned Git dependencies
birnam check                         # compile-check without publishing
birnam build                         # publish the debug artifact graph
birnam run                           # run the published main and exit
birnam run --build -- arg1 arg2      # rebuild, run, and set ProjectArgs
birnam test --build                  # build, then test in fresh processes
birnam install                       # copy the publication into the runtime lib/
```

Dependencies may name either a project-relative path or an immutable Git
commit:

```toml
[dependencies]
shared = { path = "../birnam-shared" }
twin = { git = "https://github.com/chazu/birnam-alien-twin.git", rev = "<full-commit-id>" }
```

`birnam fetch` recursively acquires Git dependencies into the owning project's
content-addressed `.birnam/dependencies/git/` cache. The revision must be a
full 40- or 64-digit hexadecimal object id. All other project commands,
including LSP workspace discovery, remain offline and fail if an exact, clean
cached checkout is unavailable.

`check` compiles every resolved module in dependency-first order in a private
temporary directory. It does not load, evaluate, or publish project code;
`--format=json` emits the stable `birnam-check-v1` result for tools.

`build` publishes the graph atomically beneath
`.birnam/build/<target>/<profile>` as self-describing module, configuration, and
application artifacts. There is no generated build receipt. Build, run,
install, and test accept `--profile debug|release`; `debug` is the default and
each profile has an independent publication.

`run` does not re-read project source or `birnam.toml`: it preflights the last
successful application publication, applies its compiled run policy, invokes
its captured `main` exactly once, and exits by default. Use `run --build` to
refresh it from current source. Arguments after `--` are available as
`[Globals at: #ProjectArgs]`. A bare `birnam` inside a project instead runs the
latest publication and then opens the terminal REPL.

`test` runs each configured `[test].sources` entry in a fresh process. Without
that list, it discovers `.birnam` files beneath `test/` and `tests/`, preventing
mutable image and Store state from leaking between test sources.

A logical module may contain several ordered source files while still producing
one `libbirnam_<name>.dylib`:

```toml
[[module]]
entry = "src/json.birnam"
files = ["src/json/parser.birnam", "src/json/render.birnam"]
```

Projects may also declare `[[wrapped]]` native libraries and embed
project-relative files at compile time with
`[System embedFile: "assets/schema.json"]`. See
[Project check, build, and run](docs/design/52-project-check-build-and-run.md)
for the complete manifest and publication contract.

## Running Birnam

### Execution modes

The same executable selects its mode from the first argument:

| Command | Behavior |
| --- | --- |
| `birnam` | Discover project startup when present, then open the terminal REPL. |
| `birnam --repl` | Explicitly open the terminal REPL. `-i` and `repl` are aliases. |
| `birnam file.birnam` | Compile and evaluate one source file, then exit. |
| `birnam fetch [directory]` | Explicitly acquire a project's commit-pinned Git dependency closure. |
| `birnam run [directory]` | Run a project's published entry source; no REPL by default. |
| `birnam artifact.dylib` | Inspect a self-described compiled root and activate its sibling closure. |
| `birnam file.image` | Validate and boot an image, mapping its compatible compiled generation. |
| `birnam --rebuild file.image` | Boot the same image by recompiling its recorded source and selection. |
| `birnam --listen PORT [file.birnam]` | Optionally run a script, then serve the socket REPL. |
| `birnam --lsp` | Serve the project-aware language server over stdio. |
| `birnam --bake IMAGE EXECUTABLE` | Append an image to a runtime executable. |

`birnam --help` is the authoritative top-level reference. The project commands
also provide `check --help`, `build --help`, `install --help`, `run --help`, and
`test --help`.

An installed compiled root can be selected without its source project or build
receipt:

```sh
birnam /path/to/libbirnam_app.dylib
```

Before activation, the artifact inspector validates its identity, ABI, target,
native requirements, and complete sibling dependency closure. See
[Shipping compiled artifacts](docs/guide/09-shipping-artifacts.md).

### Terminal REPL

Start the console explicitly with `birnam --repl`. It reads complete bracketed
forms, so multiline messages and blocks may be pasted directly. Bindings persist
for the life of the process. The console commands are `:help`, `:globals`, and
`:quit`, with `:h`, `:g`, and `:q` aliases.

For a project entry followed by an interactive console, use:

```sh
birnam run --build --repl=tty
```

The equivalent manifest setting is:

```toml
[run]
repl = "tty"
```

### Socket REPL

The socket REPL evaluates forms against the same live image and preserves
bindings across requests. For a project, bind it explicitly to loopback:

```sh
birnam run --build --repl=socket \
  --repl-bind=127.0.0.1 --repl-port=4400
```

Connect from another terminal:

```sh
nc 127.0.0.1 4400
```

The manifest equivalent is:

```toml
[run]
repl = "socket"
socket_bind = "127.0.0.1"
socket_port = 4400
```

For a standalone runtime rather than a project run, `birnam --listen 5555`
starts the socket REPL, and `birnam --listen 5555 file.birnam` evaluates the file
before listening. This shorthand binds to `0.0.0.0`. The protocol has no
authentication or encryption, so prefer the project form bound to `127.0.0.1`
and do not expose it to an untrusted network. The current server is blocking and
serves one client at a time; `:quit` closes the client session.

### Language server

Run the LSP server over stdio from the project root:

```sh
cd /path/to/project
birnam --lsp
```

Editors should launch that command directly with the workspace root as its
working directory; stdout is reserved for LSP framing. The server provides
syntax and safe selector-arity diagnostics, hover, completion, signature help,
definitions, references, and document/workspace symbols across authored project
dependencies. It does not evaluate project code or use generated artifacts as
source locations.

The bundled [Emacs mode](tools/emacs/README.md) configures `lsp-mode`
automatically and uses the nearest `birnam.toml` as the project root. Other LSP
clients should use command `birnam`, arguments `["--lsp"]`, and the project root
as the working directory.

## Images and Executables

An image makes a live system durable. Its Store, globals, accepted dispatch
generation, source, and selected compiled units travel together. Compatible
boot maps the recorded native generation; `--rebuild` deliberately reconstructs
the same accepted generation from its recorded source and selection.

### Save an image

Save from a script or REPL with:

```birnam
[System saveImage: "/tmp/birnam-demo.image"]
```

The save uses atomic full-file replacement and answers the receiver on success
or `nil` on failure. After a rejected save, inspect:

```birnam
[[System imageSaveReport] detail]
```

Persistence does not imply that every process value is serializable. Durable
values need either a portable encoding or a registered source/compiled recovery
path; process capabilities such as isolate handles are rejected.

[`examples/image.birnam`](examples/image.birnam) creates an image containing
data and a user-defined method:

```sh
birnam examples/image.birnam
```

### Boot an image

Boot the compatible compiled generation, or deliberately rebuild it from the
recorded source:

```sh
birnam /tmp/birnam-demo.image
birnam --rebuild /tmp/birnam-demo.image
```

An image without a global `main` opens the terminal REPL after recovery, where
its restored globals and methods are immediately available.

### Create an image application

Bind a zero-argument block to global `main` before saving:

```birnam
let banner := "hello from an image";
let main := [| [System println: banner]];
[System saveImage: "/tmp/app.image"]
```

Booting that image runs `main` and exits instead of opening a REPL:

```sh
birnam /tmp/app.image
```

[`examples/app.birnam`](examples/app.birnam) is a complete runnable example.

### Bake an executable

Bake an existing image into a copy of the Birnam runtime, then run it directly:

```sh
birnam --bake /tmp/app.image ./myapp
BIRNAM_LIB_DIR="$HOME/.local/lib" ./myapp
```

From inside a running system, `[System bakeExe: "./myapp"]` saves and bakes the
current image in one operation. A baked executable is self-booting but not a
single-file static binary: it still resolves the runtime dylib closure from
`<executable>/../lib` or `$BIRNAM_LIB_DIR`.

See [Images: save, boot, bake](docs/guide/04-images.md) for the full durability,
recovery, and executable-layout contract.

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

For a [multicore prime-counting demonstration](examples/multicore/README.md)
that compares sequential and parallel execution:

```sh
birnam examples/multicore/main.birnam
```

See the [isolate guide](docs/guide/06-isolates.md). Generated API reference is
available through `birnam doc isolate`.

## Concurrency

Local tasks share a domain and take automatic turns. Independent worker
processes run concurrently on multiple CPU cores, with separate heaps and copied
messages. `Concurrency load:` prepares a batch of named source modules;
`program worker:` creates a serial service with an ordinary `handle:` method.
Requests return owned tasks, with explicit outcomes, cancellation, and cleanup.

Start with the [concurrency guide](docs/guide/07-concurrency.md) for a complete
program, the API reference, and the rules for waiting and shutdown. The
[multicore example](examples/multicore/README.md) demonstrates parallel CPU work.

```sh
birnam doc concurrency
birnam doc worker 'request:timeout:'
birnam doc taskControl await
birnam doc taskExecutor
```

For interactive object and fact-store wayfinding, see the
[image exploration guide](docs/guide/08-exploring-the-image.md).

## Wrapping a Dynamic Library with Alien

Birnam calls its native FFI layer `Alien`. A wrapper is an ordinary Birnam
module that opens a dynamic library and installs selected C functions as Birnam
methods. The project manifest supplies a repeatable native build and stages the
resulting dylib beside the compiled wrapper.

This example wraps two functions from a small `answer` library.

### Scaffold and build the library

From a Birnam project, generate the wrapper skeleton and create the native
source directory:

```sh
birnam wrap answer
mkdir -p native
```

`birnam wrap` creates `wrapped/answer/answer.birnam` and never overwrites an
existing wrapper. It only scaffolds the Birnam source; it does not compile the
native library.

Create `native/answer.c`:

```c
int answer_version(void) {
    return 1;
}

long answer_add(long left, long right) {
    return left + right;
}
```

Add the wrapper source root, startup module, and native build to `birnam.toml`:

```toml
source_roots = ["src", "wrapped"]
load = ["answer"]

[[wrapped]]
name = "answer"
build = [["cc", "-dynamiclib", "native/answer.c", "-o", "{output}"]]
```

Each `build` command is an argv array executed from the project root, not a
shell command. `{output}` is required; Birnam replaces it with a private staging
path and publishes the result as `lib/libanswer.dylib`. The additional
`{project}`, `{build}`, and `{lib}` path placeholders are available for more
involved builds.

Path dependencies propagate their `[[wrapped]]` artifacts transitively. Birnam
builds them dependency-first, runs each recipe from the dependency that declares
it, and publishes the complete native closure into the root build's `lib/`.
Wrapped names must therefore be unique across the project closure.

### Define the wrapper

Replace the scaffolded `wrapped/answer/answer.birnam` with:

```birnam
[Module named: #answer requires: #[]];
[Alien dlopen: "libanswer.dylib"];

[Globals at: #Answer put: [Object clone]];

[Alien wrap: "answer_version" signature: "i" as: #version on: Answer];
[Alien wrap: "answer_add" signature: "l l l" as: #answerAdd: on: Integer]
```

`Alien dlopen:` uses the bare staged filename. During a project run, Birnam
resolves that name from the publication's `lib/` directory before the wrapper
initializer installs its methods.

The four parts of a wrap declaration are the exported C symbol, its Alien
signature, the Birnam selector, and the object or prototype that receives the
method. The first signature type is the return type; all following types are C
arguments.

Alien maps the first C argument from the Birnam receiver and maps the remaining
C arguments from the message arguments. A C function with `K` arguments must
therefore use a selector with `K - 1` arguments. A zero-argument C function
ignores the receiver, so `answer_version(void)` can be installed on the
`Answer` singleton and called as `[Answer version]`. For
`answer_add(long, long)`, the receiver supplies `left` and `answerAdd:` supplies
`right`:

```birnam
[System println: [Answer version]];       // 1
[System println: [20 answerAdd: 22]]      // 42
```

This receiver convention is especially useful for handle APIs: install a
function whose first C parameter is `FILE *` or another opaque pointer on
`ForeignAddress`, then send the operation directly to the handle.

### Signature and ownership rules

Whitespace in a signature is optional. The scalar codes are:

| Code | C type | Birnam value |
| --- | --- | --- |
| `i` | `int` | Integer |
| `l` | `long` | Integer |
| `u` | `unsigned long` | unsigned Integer or Word |
| `d` | `double` | Float |
| `f` | `float` | Float |
| `s` | `const char *` | String |
| `p` | `void *` | ForeignAddress |
| `v` | `void` | return type only; the call answers `nil` |

For example, `"p s i"` describes a function returning `void *` and taking a
string receiver plus one integer message argument. Pointer values are
`ForeignAddress` objects, never integer addresses. Use `[Alien alloc: size]` for
an owned native buffer and the `peek*At:`, `poke*At:`, and `free` messages to
manage it.

An `s` argument is copied to a temporary NUL-terminated C string and released
as soon as the call returns; native code must not retain that pointer. An `s`
return is copied into a Birnam String, with a null pointer becoming `nil`. If a
library returns caller-owned memory, wrap it as `p` and expose the library's
matching release function so ownership remains explicit.

By-value structs use braces and map to Vectors: `{ll}` is a two-`long` struct.
Fixed arrays are allowed inside structs, so `{4i}` is a struct containing one
four-element `int` array. Struct wrappers require the collection module. Match
the library's header exactly: Alien generates an ABI shim from the declaration,
but it cannot detect an incorrect C type, layout, calling convention, or
lifetime promise.

### Check, run, and install

Use the normal project workflow:

```sh
birnam check
birnam run --build
birnam test --build
birnam install --build
```

`check` validates the wrapper source and manifest without executing native build
commands. `build` and the `--build` forms compile the dylib into private staging,
compile the wrapper module, and atomically publish both. A later `run` validates
their recorded checksums before loading them. The same workflow includes
wrapped artifacts declared by transitive path or commit-pinned Git
dependencies.

If loading fails, first confirm that the `[[wrapped]].name`, the bare
`Alien dlopen:` filename, and the exported symbol spellings agree. For larger
examples, see the libc methods in [`modules/file.birnam`](modules/file.birnam)
and the first-party adapter in
[`wrapped/termbox2/termbox2.birnam`](wrapped/termbox2/termbox2.birnam).

## Developer Tools

The compiled CLI, REPL, LSP, and documentation modules are included by
the Rust installer and in release bundles. Their source is embedded in the
installed compiled units. In a development checkout, run `mise run build` and
use the installed prefix (by default `$HOME/.local/bin/birnam`).

```sh
BIRNAM_BIN="${BIRNAM_PREFIX:-$HOME/.local}/bin/birnam"
"$BIRNAM_BIN" doc time nowMillis
"$BIRNAM_BIN" docgen /tmp/birnam-docs.html
```

- `doc` prints module or selector documentation.
- `docgen` writes an HTML reference for birnam modules and native floor APIs.
- `wrap` scaffolds an Alien wrapper without overwriting existing source; see
  [Wrapping a Dynamic Library with Alien](#wrapping-a-dynamic-library-with-alien).

The terminal REPL, socket REPL, and LSP launch forms are documented under
[Running Birnam](#running-birnam).

The generated reference opens with the authored language, idioms, fact-store,
and image guide, followed by the API reference extracted from current module
facts and native-floor documentation.

## Verification

Run everyday correctness with `mise run verify`. Preview the steps with
`mise run verify-plan`. Before a production release, run:

```sh
mise run verify-release
```

`mise run rust-cutover-verify` is the equivalent explicitly named release gate.
It runs the workspace and exhaustive semantic checks, an installed-prefix check,
and two independent cold-source builds with relocated execution. See the
[current validation guide](docs/validation.md) for coverage, selective execution,
evidence reuse and performance checks.

## Benchmarks

Performance is tracked per tagged release. The suite measures the send
constant and the native-vs-derived boundaries that gate self-hosting
migrations, the datalog read/write path, GC, images, boot paths, and
whole-process workloads — always against a ReleaseSafe build.

```sh
BIRNAM_BIN="${BIRNAM_PREFIX:-$HOME/.local}/bin/birnam"
BENCH_QUICK=1 "$BIRNAM_BIN" bench/micro.birnam        # tight dev loop (~5s)
mise run bench                                          # isolated Rust-system run
mise run bench-compare <old-floor.json> <new-floor.json>
```

When to run: the affected suite before and after any perf-relevant change
(dispatch, collections, strings, store, codegen); the full `mise run bench` +
a compare against the last release snapshot before merging bigger work.
`bench-compare` exits non-zero when a case regresses past its threshold
(10% micro, 5% elsewhere), so it can gate scripts.

**As part of the release process:** after `mise run release`, on the tagged
commit and a quiet machine, run the Rust-system snapshot:

```sh
mise run bench-release
git add bench/results/v<X.Y.Z>-rust-*.json && git commit
```

The committed Rust-system snapshots are the baselines future runs compare
against; filenames and JSON stamps include the commit and selected dylib digest.
`docs/misc/BENCHMARKS.md` is the full operational guide;
[docs/design/46-benchmarks.md](docs/design/46-benchmarks.md) records the
methodology and the measured constants.

## Architecture

The runtime starts as a Rust host that loads a set of reloadable dynamic
libraries through a C ABI. The `libid` floor and all native rungs are Rust.
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
artifact required. The Rust host owns the complete runtime and build closure.

## Repository Guide

- `.beads/` — active issue tracker; run `br ready` in this checkout. See
  [work tracker conventions](docs/misc/TASKS.md).
- `rust/birnam-id/` — Rust `libid` and remaining Machine/runtime components behind IdApi v20 (18 fields / 144 bytes).
- `rust/birnam-bootstrap/` — sequential native loading through Id and ownership of initialization results; see [the native contract](abi/fixtures/bootstrap-v1/README.md).
- `rust/birnam-root-registry/` — native root ownership, initialized through Id before Runtime.
- `rust/birnam-collect/` — [collection records and controls](abi/collection-state-v1.md), initialized before Runtime and shared with its collector, plus the collection coordinator.
- [Native Stack factory v3](abi/native-stack-v3.md): native register capture and root scanning before Runtime.
- `rust/birnam-trace/` — [live graph traversal](abi/trace-graph-v1.md), constructed before Runtime from Id storage facts and completed lifetime owners.
- `rust/birnam-lifetime/` — payload descriptions, trace hooks and finalizer records, initialized through Id and adopted by Runtime; see [the native contract](abi/fixtures/lifetime-v1/README.md).
- `rust/birnam-namespace/` — early publication/import, reverse aliases and native enumeration, adopted by Runtime; see [the native owner](abi/fixtures/namespace-owner-v1/README.md).
- `rust/birnam-object-model/` — early private-behavior ownership/construction and native enumeration, adopted by Runtime; see [the native owner](abi/fixtures/object-model-owner-v1/README.md).
- `rust/birnam-error-value/` — shared lazy Error factory dispatch and exact recognition for native System, Block and numeric methods.
- `rust/birnam-source-loader/` — System source reading, staging, evaluation and diagnostics.
- `rust/birnam-native-loader/` — System direct native-module loading, installed by Prelude.
- `rust/birnam-block-loop/` — native Block while methods, installed by Prelude before authored stackless control.
- `rust/birnam-file-io/` — System binary file reads and writes, installed by Prelude.
- `rust/birnam-environment/` — System environment lookup, installed by Prelude with shared text/error policy.
- `rust/birnam-system-input/` — System stdin method, installed by Prelude using String and Error Value.
- `rust/birnam-system-output/` — System stdout methods, installed by Prelude using shared text views.
- `rust/birnam-text-view/` — shared borrowed String/printable byte views for Prelude consumers.
- `rust/birnam-system-gc/` — System collector queries and policy controls, installed by Prelude.
- `rust/birnam-printing/` — Object/Symbol printing defaults and live `asString` dispatch.
- `rust/birnam-collection-value/` — shared native value comparison and key hashing for Vector and Map.
- `rust/birnam-vector/` — independently loaded Vector storage, trie helpers, canonical objects, methods and sorting roots.
- `rust/birnam-map/` — independently loaded Map storage, canonical objects, methods, traversal and printing.
- `rust/birnam-range/` — independently loaded Range representation, construction and all Range methods; uses native Vector storage.
- `rust/birnam-string-builder/` — independently loaded StringBuilder buffer ownership and methods.
- `rust/birnam-string/` — [String byte storage](abi/fixtures/string-v1/README.md) before Error/numbers, followed by native text and numeric methods before Runtime.
- `rust/birnam-error/` — [Error construction and propagation](abi/error-value-v3.md#production-error-construction), initialized before numbers and Runtime.
- `rust/birnam-string-collection/` — late String split adapter using native Vector construction.
- `rust/birnam-nil/` — early Object/Nil predicates and native String printing before Runtime.
- `rust/birnam-boolean/` — early Nil/Boolean identities, basic operations and printing, followed by Block control with Execution; see [base values](abi/fixtures/base-values-v1/README.md).
- `rust/birnam-word/` — Word construction, printing and checked Integer conversion through Id before Runtime;
  [native dependency contract](abi/word-v1.md).
- `rust/birnam-float/` — Float construction and operations before Runtime, followed by Integer's mixed-number extension;
  [native dependency contract](abi/float-v1.md).
- `rust/birnam-integer/` — Integer construction and arithmetic through Id before Float/Runtime, followed by mixed-number and Block extensions;
  [native dependency contract](abi/integer-v1.md).
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
- [Throughput recovery](docs/design/66-stackless-throughput-recovery.md) records
  current profiles and the proposed optimization sequence.
- [The image](docs/design/40-the-image.md) defines persistence and mapped-code
  behavior.
- [Minimal production Id](docs/design/82-minimal-production-id.md) records why
  the Rust extractions failed to decouple Id, the required Siphon/Id boundary,
  and the production migration and acceptance criteria. This is the current
  runtime refactoring direction.
- [Production construction and lifetimes](docs/design/83-production-id-construction-and-lifetimes.md)
  addresses the adversarial review: simpler dispatch ownership, native loading
  results, base/extension load order and safe collector activation. The production
  [Siphon](rust/birnam-host/src/siphon.rs) now loads Id through the
  [typed native protocol](abi/fixtures/id-protocol-v1/README.md); upper construction
  and ownership migration remain in progress.
- [Production extraction status](docs/design/81-production-extraction-status.md)
  summarizes the completed Rust modularization through 81CA, current ownership,
  validation, and the remaining Birnam implementation work. This phase is closed;
  further extraction requires a subsystem-sized objective.
- [Current bootstrap and runtime layers](docs/design/81a-current-bootstrap-and-runtime-layers.md)
  explains the installed Rust host, 53 native libraries and 28 compiled Birnam
  modules, including startup preparation and image recovery.
- [Self-hosting roadmap](docs/design/81-toward-a-birnam-implementation.md) defines
  the eventual Birnam implementation and retained independent bootstrap.
  [IdApi v20](abi/idapi-v20.json), [native initialization](abi/native-loader-v12.md),
  and the [boundary reference](docs/design/81-production-extraction-status.md#subsystems-and-contracts)
  describe the current interfaces.
- [Compiled units](docs/design/45-compiled-units.md) defines executable
  authority, image integrity, recovery, and the next live-reload boundary.
- [Concurrency](docs/design/48-concurrency-reactors-and-isolates.md) and
  [live module evolution](docs/design/49-live-module-evolution.md) document
  withdrawn live-reload policy experiments; only the blocking single-worker
  isolate substrate is currently shipped.
- [Blocking process isolates](docs/guide/06-isolates.md) documents the shipped
  trusted-worker API, copied-value boundary, structured failures, and deadlines.
- [Concurrency domains and structured tasks](docs/design/80-concurrency-domains-and-structured-tasks.md)
  proposes the long-term model: parallel owned Runtimes, local task scheduling,
  explicit messages, and restart-based worker code updates. Detailed semantics
  remain under discussion; the [cleanup-control compatibility review](docs/design/80s-cleanup-control-review-synthesis.md)
  records the latest mechanism findings under the unchanged ABI/image constraints.
  [Cleanup-control evidence](docs/design/80t-cleanup-control-implementation-evidence.md)
  and [structured task implementation](docs/design/80u-birnam-task-cleanup-slice.md)
  track the implemented mechanisms, validation, and remaining concurrency work.
  [Bounded Machine drives](docs/design/80v-bounded-machine-drives.md) add the
  runtime support for budget stops and owner-side cleanup entry.
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
  source manifest, self-describing local publication, installed descriptor, and
  direct dylib-root workflow.
- [Embedded files](docs/design/56-embedded-files.md) documents the
  compile-time `System embedFile:` intrinsic and its project-build contract.
- [Designs 55, 58, and 59](docs/design/59-digla-adversarial-architecture-review.md)
  preserve the retired Digla application experiment and its architectural
  lessons. Digla is not part of the current runtime, verification graph, or
  product roadmap.
- [Stackless native execution](docs/design/60-stackless-native-execution.md)
  implements the S1 substrate and the S2 selected named, method-owned-block,
  and projected nonmethod-owned-block lowering, registration, candidate AOT
  delivery, and nonempty image-recovery path for explicit
  per-fiber frames and resumable native methods. Its private Runtime substrate
  uses one protected target resolver shared by ordinary sends and Machine calls
  without growing the public IdApi. Its
  [artifact byte grammar](docs/design/60i-stackless-artifact-byte-grammar.md)
  freezes the implemented descriptor, manifest, selection, registration, and
  image-v5 formats. Arbitrary effectful initializers, suspension, and durable
  continuations remain later stages.
- [The Rust libid rewrite plan](docs/design/61-rust-libid-rewrite-plan.md)
  records the floor boundary, unsafe/reentrancy discipline, and performance
  gates for the completed Rust implementation.
- [The Rust native-rung migration plan](docs/design/62-rust-native-rungs-migration-plan.md)
  records the completed migration of the higher dynamic rungs, compiler, host,
  project workflow, and image execution path to Rust.
