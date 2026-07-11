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
Zig version and exposes `mise run build`, `mise run verify`, and `mise run
install`.

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

`birnam --help` is the authoritative command reference. The primary entry
points are:

```text
birnam <file.birnam>       Compile and run a script.
birnam <file.image>        Boot a saved image.
birnam --rebuild <image>   Boot an image by replaying source.
birnam --bake <image> <executable>
                             Bake an image into a self-booting executable.
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

Persistence is intentionally not a claim that every process value can be
serialized. Saving is atomic and rejects durable values without either a
portable encoding or a registered recovery path; inspect
`[System imageSaveReport]` after a rejected save. See
[The Image](docs/design/40-the-image.md) for the exact durability contract and
recovery paths.

## Developer Tools

The source-backed tools below currently require a checkout containing
`modules/`; they are not yet staged by `zig build install` or release bundles.

```sh
./zig-out/bin/birnam doc time nowMillis
./zig-out/bin/birnam docgen /tmp/birnam-docs.html
./zig-out/bin/birnam --lsp
./zig-out/bin/birnam --listen 5555 examples/fizzbuzz.birnam
```

- `doc` prints module or selector documentation.
- `docgen` writes an HTML reference for birnam modules and Zig floor APIs.
- `--lsp` serves the language server over stdio.
- `--listen` starts a socket REPL, optionally after running a script.

`wrap` is a binary-only scaffolder and works from an installed executable:

```sh
birnam wrap mylib
```

It creates an Alien-based C-library wrapper under `wrapped/`.

## Verification

Run the complete gate before a release or cross-layer change:

```sh
zig build verify
```

It runs the Zig unit/integration suites, rebuilds self-hosted artifacts with
`stage1`, executes the birnam-level suites, and checks REPL transcripts. The
narrower commands are `zig build test`, `zig build test-birnam`, and `zig build
test-repl`. GitHub Actions runs `verify` on the supported arm64 macOS target.

## Architecture

The runtime starts as a small Zig siphon that loads a set of reloadable dynamic
libraries through a C ABI. The active rungs are:

```text
libid -> Store -> Peg -> Lang -> Tcc -> Codegen -> Prelude
      -> Collection -> Modules -> Error -> Relational -> Alien -> Image
```

`libid` provides the object model, allocation, dispatch, interning, and GC.
Higher rungs add the relational substrate, parser, compiler, native-code
backend, collections, FFI, and image format. The standard library and most
application-facing modules are birnam source; Zig remains the permanent
from-zero bootstrap compiler and performance floor.

## Repository Guide

- `src/id/` — Zig floor, compiler, image, FFI, and test suites.
- `src/id/*.birnam` — bootstrapped standard library, TOML, and configuration.
- `modules/` — source-backed libraries, CLI, LSP, documentation, and tests.
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
- [Modules and the self-hosted layer](docs/design/43-modules-and-config.md)
  explains source, artifacts, and loading.
- [Toward self-hosting](docs/design/44-toward-self-hosting.md) records the
  migration boundary: retain the Zig bootstrap and move only appropriate
  library surface into birnam.
