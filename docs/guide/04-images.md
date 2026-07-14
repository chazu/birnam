# Images: create, save, bake

A birnam session is an **image** — the live store plus compiled code. Saving
one writes the system to disk; booting one maps it back, methods and all, with
no recompilation. Persistence is a property of the system, not a feature you
add to a program.

## Saving

```
[System saveImage: "/tmp/app.image"];       // a fresh, fully-compacted base
[System appendImage: "/tmp/app.image"];     // ONLY what changed since last save/append
```

`appendImage:` is the cheap checkpoint: a long-running program can append its
evolving data repeatedly and `saveImage:` occasionally to compact. A method
redefined after the base save is re-emitted in the append as a composing code
record, so the image stays self-consistent without a rebuild.

> **Current boundary:** the paragraph above describes the implemented pre-L1a
> behavior. L1a keeps data-only append when the saved and live generation digests
> match, but requires a full atomic save after a dispatch/source generation change.
> See [plan 50](../design/50-five-phase-live-modules-and-isolates-implementation-plan.md#6-l1a--generation-authority-and-image-coherence).

`saveImage:` answers self on success and **nil on failure** — gate on `isNil`
and read `[[System imageSaveReport] detail]` when it refuses. The classic
refusal: a **block stored in a durable cell** ("unserializable durable store
fact"). Closures can't be serialized as values — keep shared blocks in methods
that answer fresh ones, or in top-level `let` globals (those survive by a
different mechanism, below).

## Booting

```
birnam /tmp/app.image             // boot: REPL over the image's bindings
birnam --rebuild /tmp/app.image   // same, via the source-replay path
```

What comes back, and how:

- **Data** — globals, entities, facts, cells — maps directly.
- **Methods** — the compiled `.code` section maps in and installs with no
  recompile; `--rebuild` ignores it and re-evaluates the recorded sources
  instead (the recovery path — every method's source is a fact).
- **Module singletons and their Alien FFI wraps** — replayed from recorded,
  replayable source at boot (`dlopen`s rerun; a missing library warns and
  continues rather than killing the boot).
- **Block-valued globals** — a closure's value can't serialize, but a
  top-level `let` records its SOURCE, and the boot re-derives the block after
  code and data are restored. Captured globals resolve because data comes
  back first.

## An app image: the main convention

If the global `main` is bound to a block, booting the image **runs it**
instead of opening the REPL:

```
let banner := "my app v1";
[Reflect defineMethod: #total on: Integer as: [:x | [x + x]]];

let main := [|
  [System println: banner];
  [System println: [[10 total] printString]]];

[System saveImage: "/tmp/app.image"]
```

`birnam /tmp/app.image` now prints and exits — a packaged program.

## Baking: a self-booting executable

Baking appends the image to a copy of the runtime, producing one file that
boots itself — no `birnam` invocation, no `.image` argument:

```
birnam --bake /tmp/app.image ./myapp     // then just:  ./myapp
[System bakeExe: "./myapp"]              // or bake the RUNNING system in one step
```

A baked executable still resolves the runtime dylibs the normal way — keep
them in `<exe>/../lib`, or point `$BIRNAM_LIB_DIR` at them.

## AOT artifacts and compiled modules

Below whole-image persistence sits the same machinery à la carte:

```
let p := [Codegen compile: src output: "/tmp/unit.dylib"];   // program -> relocatable dylib
[Codegen load: p]                                            // dlopen + run, ANY process
```

The artifact carries no addresses from the compiling process — runtime calls
stay dynamic symbols, global references re-derive by name at load. Library
modules use the same path with a manager on top:

```
birnam --compile-module modules/greeter.birnam zig-out/lib/libbirnam_greeter.dylib
[Module load: #greeter]      // runtime load, dependencies first, cycle-safe
```

The build's `zig build stage1` bakes the standard modules this way; from-source
loading remains the floor every compiled path can fall back to.
