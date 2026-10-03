# Birnam

Birnam is a late-bound, message-oriented language system built on a Piumarta-style
object floor. It combines a small reflective runtime, a relational store, PEG
parsing, native compilation through libtcc, self-hosted library code, and durable
images.

It is an active pre-alpha system. The architecture and its bootstrap are the
product; compatibility and language surface can and will still change.

**Current development status:** the production runtime, installed CLI and tools
use Id messages between functional owners, following
[Design 90](docs/design/90-functional-modules.md). Source compilation, project
commands, images, REPLs, LSP, process workers and generation activation/rollback
are integrated. The native execution model supports resumable Methods, Blocks,
Machines and continuations.

Structured tasks and process workers are available today. Thread-hosted domains,
channels and general asynchronous timer/I/O integration remain future work.
Narrow [discarded-clone](docs/design/90-discarded-clone.md) and
[numeric-predicate](docs/design/90-numeric-predicate.md) optimizations are implemented;
broader optimizer work and a Birnam-authored runtime/compiler remain unfinished.

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

From a checkout with [`mise`](https://mise.jdx.dev):

```sh
mise install
mise run install
export PATH="$HOME/.local/bin:$PATH"
birnam
```

The default prefix is `$HOME/.local/opt/birnam`. `mise run install` creates the
command link at `$HOME/.local/bin/birnam` when `BIRNAM_PREFIX` is unset. Setting
`BIRNAM_PREFIX` selects another prefix; add its `bin/` directory to `PATH`.
The installed executable finds its runtime dylibs in the sibling `lib/` directory.

Build and install an optimized runtime with:

```sh
mise run install release
```

Both install forms build stage 0 (the Rust CLI/native runtime), use it to compile
stage 1 (the authored Birnam modules), then publish the complete prefix. Stage 1
retains the Rust runtime. Omitting the profile selects `debug`; `release` selects
Rust's optimized build and replaces the same prefix, so the command link runs it.

Without mise, use the pinned Rust toolchain and run the installer directly:

```sh
BIRNAM_PREFIX="${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}"
scripts/rust-install.sh install --root . --prefix "$BIRNAM_PREFIX" --profile debug
export PATH="$BIRNAM_PREFIX/bin:$PATH"
birnam
```

`mise run build` builds both stages with Cargo caching at the same default
location without creating the command link. Use `mise run build release` for
optimized development builds. [mise.toml](mise.toml) pins the toolchain and exposes
build, install, verification, distribution and release tasks.

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
`[System embedFile: "assets/schema.json"]`, which returns a clonable Bytes object.
See [embedded files](docs/design/56-embedded-files.md), and
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
| `birnam --rebuild file.image` | Boot the image by recompiling its saved canonical programs. |
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
native requirements, and complete sibling dependency closure. Generated module
and application artifacts retain the `.dylib` suffix for resolution, but contain
relocatable TCC objects with the `id-object-v2` contract. The runtime loads these
through TCC; native runtime and Alien libraries are ordinary OS dynamic libraries.
See [Shipping compiled artifacts](docs/guide/09-shipping-artifacts.md) and the
[current artifact contract](rust/birnam-codegen-runtime/README.md#durable-module-artifacts).

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

An image makes a live system durable. The current image-v8 format preserves the
Store and its history, globals, object identity, accepted method bindings,
canonical programs, Block captures and source provenance. Compatible boot loads
the saved relocatable native objects through TCC. `--rebuild` compiles the saved
canonical programs to restore the same accepted generation. Neither path
evaluates the saved source or reruns its initializers.

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

Shared and cyclic references, boxed values, Bytes, persistent collections and
synchronous/resumable Blocks retain their identity and captures. Live Machines
and process capabilities such as isolate and task handles are rejected. Saved
Alien wrapper objects are mapped during Image publication and bound to native
addresses in the new process. See the [current Image contract](rust/birnam-image/README.md) for supported
values, validation and recovery compensation.

[`examples/image.birnam`](examples/image.birnam) creates an image containing
data and a user-defined method:

```sh
birnam examples/image.birnam
```

### Boot an image

Boot the compatible saved native code, or explicitly rebuild it from canonical
programs:

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
BIRNAM_LIB_DIR="${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}/lib" ./myapp
```

From inside a running system, `[System bakeExe: "./myapp"]` saves and bakes the
current image in one operation. A baked executable is self-booting but not a
single-file static binary: it still resolves the runtime dylib closure from
`../lib` relative to its executable directory, or from `$BIRNAM_LIB_DIR`.

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
Worker hosting currently uses processes; thread-hosted domains, channels and
general asynchronous timer/I/O integration remain future work. Native calls are
not forcibly preempted, so a blocking native call can block local tasks.

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
It also covers [retaining and invoking Methods](docs/guide/08-exploring-the-image.md#retain-and-invoke-a-method)
across binding replacement and removal.

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
use the prefix executable (by default `$HOME/.local/opt/birnam/bin/birnam`).
`mise run install` also creates the default `$HOME/.local/bin/birnam` command link.

```sh
BIRNAM_BIN="${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}/bin/birnam"
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
whole-process workloads. `mise run bench` builds an isolated prefix with Rust's
`release` profile. Benchmark JSON retains the historical `ReleaseSafe` optimize
label for continuity with earlier receipts.

```sh
BIRNAM_BIN="${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}/bin/birnam"
BENCH_QUICK=1 "$BIRNAM_BIN" bench/micro.birnam  # quick checks with the selected prefix
mise run bench                               # isolated Rust release-profile run
mise run bench-compare old-floor.json new-floor.json
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
BIRNAM_VERSION="$(cat VERSION)"
git add "bench/results/v${BIRNAM_VERSION}"-rust-*.json && git commit
```

The committed Rust-system snapshots are the baselines future runs compare
against; filenames and JSON stamps include the commit and selected dylib digest.
[The benchmark guide](docs/misc/BENCHMARKS.md) covers operation and thresholds;
[Design 46](docs/design/46-benchmarks.md) records methodology. Measurements describe
their recorded release, machine and workload; historical constants are not current
runtime guarantees. Release-specific acceptance records remain in
[the archive](docs/archive/).

## Architecture

[Design 90](docs/design/90-functional-modules.md) defines functional ownership,
minimal dependencies and Id messages between native modules. The
[native module decomposition roadmap](docs/design/native-module-decomposition-roadmap.md)
tracks the next extractions and Birnam replacements, starting with the source
compiler and native backend.

Host starts the runtime by asking Siphon to load Id, then sends Id a request to
load Bootstrap:

```text
CLI / Host
  -> Siphon -> Id message protocol
  -> Id loads Bootstrap -> native owners and Namespace
  -> compiled bootstrap image: Module manager, stdlib, lifecycle policy and tools
  -> source, artifact, project or image entry
```

Id owns allocation records, Symbols, behavior dictionaries, dispatch and native
loading. [Bootstrap](rust/birnam-bootstrap/src/lib.rs) owns the native load order;
Siphon and the shared ABI contain no catalog of upper modules. Bootstrap retains
initializer arguments and results through
[Roots](rust/birnam-root-registry/README.md), and Namespace publishes completed
objects. Modules loaded by Bootstrap return an opaque Object or Unit; consumers
use their messages without decoding another owner's payload.

[GC](rust/birnam-collect/README.md) owns tracing and reclamation;
[ObjectModel](rust/birnam-object-model/README.md) owns ordinary slots, cloning and
private behaviors. Value and collection owners implement their own representation,
equality and hashing. [Store](rust/birnam-store/README.md) owns facts, history,
Cells and method-publication rules, while
[Relational](rust/birnam-relational/README.md) supplies queries, rules and aggregates.

[Execution](rust/birnam-execution/README.md) owns Methods, Blocks, frames, bounded
Machines and continuations. [Codegen](rust/birnam-codegen-runtime/README.md) connects
PEG/Lang compilation and TCC code ownership to Execution. Generated calls use Id
messages. Blocks, Methods and suspended frames retain their executable owners;
code is reclaimed after the final executable reference is released and an idle
checkpoint is reached.
[Image](rust/birnam-image/README.md) restores durable state through those same
owners. [Host](rust/birnam-host/README.md) connects source, artifact, project,
image and worker entry to the installed CLI and tooling.

The standard library, task scheduling/lifecycle policy and most application-facing
modules are Birnam source. The native runtime/compiler implementation remains
Rust with C/libtcc support. The installed runtime stays on its creating thread;
parallel workers use independent processes. A Birnam-authored runtime/compiler
is the longer-term [self-hosting goal](docs/design/81-toward-a-birnam-implementation.md).
Lang's grammar and parsing facade are authored in `modules/lang.birnam`.
Bootstrap loads precompiled Module and Lang artifacts before source parsing.
The installer uses `birnam-lang-seed` to compile those artifacts, then excludes
that Rust grammar seed from the published runtime. PEG matching and Codegen
remain Rust implementations.

The installer prepares `lib/birnam-bootstrap.image` for ordinary and console
startup, plus `lib/birnam-tools.image` for CLI tooling. Both include the required
authored methods and foreign-function wrappers. Normal startup maps compiled code
without recompiling the bootstrap or tool adapters. Compilation also records
source metadata in Store; compiled startup restores these facts rather than
parsing source or traversing syntax trees. A missing or incompatible image is an
error; reinstall to regenerate it. `BIRNAM_FORCE_SOURCE=1 birnam ...` explicitly
requests source bootstrap for development or recovery and can be slow.

Birnam retains an independent cold source bootstrap through declared non-Birnam
toolchains, without requiring a prebuilt Birnam artifact.

`mise run native-messages` checks the native owner boundaries, including live
method replacement, nested loading, collection and finalization. Installed Host,
image and generation lifecycle checks are part of the
[validation workflow](docs/validation.md).

## Repository Guide

- `rust/birnam-siphon/` — the small ad-hoc Id loader. It depends only on
  `birnam-abi`, loads the real Id, and returns its message protocol.
- `rust/birnam-cli/` — the `birnam` executable, argument parsing, command
  dispatch and diagnostics.
- `rust/birnam-build/` — repository build/install tooling: builds Birnam, stages
  its artifacts and publishes an installation. Independent of the CLI and host.
- `rust/birnam-host/` — runtime startup, resource discovery, project operations
  and execution bridges used by the CLI. No argument parser or Siphon implementation.
- `rust/birnam-abi/` — opaque objects, the common Id protocol, borrowed buffers
  and the stateless OS loader shared by Siphon and Id.
- `rust/birnam-id/` — native Id allocation, Symbols, dictionaries, message
  dispatch and module loading; depends only on the ABI crate.
- `rust/birnam-bootstrap/` — native load order and initializer retention;
  returns the completed Namespace to Host.
- `rust/birnam-collect/` — instance-owned collection, tracing, finalization and
  native root capture above Id and Roots. No foreign payload decoding.
- `rust/birnam-object-model/` — ordinary object storage and cloning, private
  behaviors and exact slot metadata; reflection uses existing Id messages.
- Other `rust/birnam-*/` crates — native values, compiler/runtime,
  storage, images, workers and language services. Owner READMEs describe their
  message protocols and lifetime rules.
- `modules/` — all modules written in Birnam: core language and startup policy,
  libraries, CLI/LSP tools, documentation and tests. Core sources are embedded
  for explicit source bootstrap; the installer lists modules for compilation and
  prepares the default startup image.
- `tests/fixtures/` — shared portable-code, recovery and image format evidence.
  [Native protocol checks](rust/birnam-abi/README.md) live beside the ABI crate;
  each native crate keeps its exported-symbol inventory in `exports.json`.
- `wrapped/` — native libraries used by optional applications.
- `examples/` — programs, including images and AOT artifacts.
- `docs/guide/` — language and user workflow guides.
- `docs/design/` — current contracts and unfinished designs;
  `docs/archive/` holds historical decisions, experiments and completed plans.
- `docs/measurements/` — benchmark evidence and payload-loading instructions.

## Design Notes

The active design documents describe contracts and remaining work:

- The [module design](docs/design/90-functional-modules.md) defines ownership,
  opaque native messages, minimal dependencies and safety rules. The Id-message
  migration is complete; its checkpoint history is not a current work queue.
- [Object authority](docs/design/42-object-canonical-relational-derived.md) and
  [relational semantics](docs/design/37-the-relational-reading.md) define the
  relationship between objects, Store facts and queries.
- [Compiled units](docs/design/45-compiled-units.md) and
  [their lifecycle](docs/design/47-compiled-unit-lifecycle.md) define executable
  authority, staged activation, rollback and active-code lifetimes.
- [Stackless execution](docs/design/60-stackless-native-execution.md) and
  [V2 control](docs/design/60p-v2-control-completion.md) define resumable execution,
  suspension and completion. [Artifact framing](docs/design/60i-stackless-artifact-byte-grammar.md)
  and [PortableProgram grammar](docs/design/60l-portable-program-and-image-v7-byte-grammar.md)
  remain format references; [image-v8](docs/design/60o-image-v8-identity-preservation.md)
  and the [Image owner](rust/birnam-image/README.md) describe current persistence.
  Live Machines and task capabilities are not durable image values.
- [Project manifests](docs/design/51-project-manifests-and-dependency-resolution.md),
  [command workflows](docs/design/52-project-check-build-and-run.md) and
  [self-describing publications](docs/design/65-self-describing-project-publications.md)
  define dependency resolution, build/run/install and execution without source.
  [Language tooling](docs/design/53-project-aware-language-tools.md),
  [relocatable bundles](docs/design/54-relocatable-bundles.md) and
  [embedded files](docs/design/56-embedded-files.md) cover shipped integration.
- [Concurrency domains](docs/design/80-concurrency-domains-and-structured-tasks.md)
  and [cleanup transitions](docs/design/80n-cancellation-to-cleanup-transition.md)
  define task and worker lifecycle. Structured tasks and process workers are
  shipped; thread hosting, channels and general asynchronous timer/I/O remain
  unfinished. Start with the [user guide](docs/guide/07-concurrency.md).
- [Discarded-clone](docs/design/90-discarded-clone.md) and
  [numeric-predicate](docs/design/90-numeric-predicate.md) optimizations are current
  bounded allocation-elision contracts. Broader optimization remains separate.
- [Map deletion](docs/design/70-persistent-map-deletion.md) remains a proposal;
  [scalable ordering](docs/design/73-scalable-ordering-contract.md) has accepted
  semantics with implementation work still open.
- [The self-hosting roadmap](docs/design/81-toward-a-birnam-implementation.md)
  retains the Birnam-authored runtime/compiler objective and independent bootstrap.

Earlier architectures, retired Digla experiments, completed extraction plans and
dated performance acceptance are in [docs/archive](docs/archive/). Those documents
preserve rationale and evidence; the current owners and contracts above describe
the implementation.
