# 56 — Embedded Files

> **Status:** Reconnected through Id messages, 2026-09-26. Compiled native code
> returns Bytes; native-library self-containment is tested. Image round-trip and
> project/Host integration remain later migration work.

## Contract

```birnam
let data := [System embedFile: "assets/schema.json"];
[data size];
[data at: 0];
[data asString]
```

The result is an ordinary, clonable **Bytes prototype object**. It contains the
exact file contents, including NUL and non-UTF-8 bytes. Bytes has its own behavior;
its [message contract](../../rust/birnam-string/README.md#bytes-prototype) documents
cloning, indexing, conversions and the separate native borrowed Bytes operand.

The compile-time form requires a literal, non-empty relative String path without
`.` or `..` components. NUL and non-UTF-8 paths are rejected. Each payload is
limited to 16 MiB. Empty files are valid. `=>` threading remains disallowed for
this source-sensitive form. Globs and dynamic compile-time paths are unsupported.

## Message path and lifetime

While compiling, Codegen resolves the path and sends `embedFile:` to the current
System receiver, using Globals-first lookup with Namespace fallback. File IO owns
the bounded native file read and sends `Bytes fromBytes:`. Codegen sends `bytes`
to that result and copies the borrow before any further send. Replacing System's
method, its global binding or the returned object's byte method affects compilation.
A lexical receiver named System remains an ordinary runtime receiver; the compiler
does not substitute the global System for it.

The copied payload goes into the existing portable String literal encoding,
followed by an ordinary `asBytes` send. Generated native code contains the payload
and constructs the result through current String/Bytes methods. No new literal
codec, resource catalog, native callback or cached method entry is introduced.
The String is an internal binary-safe carrier; the default result is Bytes.
Neither executing compiled code nor loading its dylib rereads the source asset.
The runtime Bytes owns a copy, independent of the code mapping's lifetime.

Compile-time embedding is distinct from sending `embedFile:` dynamically: the
latter invokes the current method at that time, like other ordinary sends.

## Path context and errors

`System load:` and `Codegen evalPersisting:from:` resolve literal paths relative
to the source file's directory. Context-free `emit:`, `compile:`, `eval:` and
`evalPersisting:` resolve from the process working directory. No mutable global
compiler path context is used.

Ordinary compiler entrypoints return validation/read Errors. Loader-facing
persisting evaluation preserves nil plus `lastError`; System load reports its
usual compilation failure. Native send failures propagate unchanged.

## Validation and remaining migration

`birnam-siphon/tests/embedding.rs` covers binary/empty files, path rejection,
16 MiB refusal, clones, MOP publication, collection and replaced methods. It loads
a source-relative embedded Block, removes the source and asset, then executes it.
It also emits C, compiles a real dylib with the system compiler, removes the asset
and C, loads the dylib through Id and executes its Method on a Machine. A retained
Bytes value remains valid after its producing TCC Unit is reclaimed.

Project-root compilation and asset-content build invalidation remain contracts
for the later project-build reconnection; this slice does not restore those APIs.
Historically Image source replay still needed the original assets. Image migration
must persist materialized payloads inside the image, without replaying filesystem
reads or requiring sidecar resources. A working image round-trip is explicitly
outside this slice's acceptance boundary.
