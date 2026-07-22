# 56 — Embedded Files

> **Status:** P0 implemented, 2026-07-16. Birnam source can embed one literal
> file into a compiled module dylib. Image self-containment for source replay
> remains a follow-up.

## 1. Contract

Birnam uses a compiler intrinsic rather than a runtime file lookup:

~~~birnam
[System embedFile: "assets/schema.json"]
~~~

The intrinsic returns a normal Birnam `String`. Strings carry an explicit byte
length, so text and binary files—including NUL bytes—use the same value type.

The argument must be a string literal. The path must be non-empty, relative,
and must not contain `.` or `..` path components. Globs, directory walks, and
dynamic paths are deliberately not part of this first slice. Embedded files are
limited to 16 MiB each.

Embedding happens while source is compiled or evaluated. The bytes are emitted
into the generated C as a length-bearing string and therefore into the dylib;
the loaded dylib does not read the original file at runtime.

## 2. Path context

The context is explicit at the compiler boundary:

- direct source evaluation and `System load:` resolve relative to the loaded
  source file's directory;
- project builds resolve relative to the owning project root, so every source
  fragment in a multi-file module has one stable package-like root;
- the old context-free `Codegen eval:` and `Codegen compile:output:` selectors
  remain valid for sources that do not embed files.

The project build entrypoint is `Codegen compile:output:root:`. Source tooling
uses `eval:from:` and `evalPersisting:from:` when it has a document path.

## 3. Build invalidation

The resolver statically finds distinct literal `System embedFile:` inputs and
hashes their path and contents into the module's project source digest. A
changed, removed, or newly unreadable asset therefore invalidates the owning
module before artifact reuse. Dependencies are still invalidated through the
existing module graph.

## 4. Why this surface

This keeps the language grammar unchanged and uses the existing binary-safe
String and dylib compiler seams. A module method can return an embedded value,
and the same source works whether the module is one file or an explicitly
composed set of files. There is deliberately no manifest-level resource catalog:
the bytes belong to the compiled artifact that consumes them, and artifact
identity/requirements are already available through its ID descriptor.

## 5. Image recovery boundary

Compiled artifacts are self-contained with respect to embedded bytes. The
current image source-recovery path records authored module source in
`__modsrc__`; if recovery must re-evaluate that source, it still needs the
original asset path. Normal dylib loading and project builds do not have this
dependency. A future image format may embed the materialized bytes in a checked
image section. It must remain part of the self-describing image—not an external
TOML/resource sidecar—and does not change the source intrinsic.
