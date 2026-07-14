# Images: save, boot, bake

A Birnam image is the durable Store plus one authoritative dispatch generation.
The generation contains the exact complete source map and canonical digest; its
compiled code is an optimization bound to that same digest.

## Saving

```text
[System saveImage: "/tmp/app.image"]
```

Full temporary-file replacement is the only supported write mode. `appendImage:`
is not part of the generation format: source, facts, and executable code cannot be
advanced as independent journals without recreating split authority.

`saveImage:` answers self on success and nil on failure. Inspect
`[[System imageSaveReport] detail]` when it refuses. Save rehashes the current
generation, compiles every complete module document once in dependency order,
binds the resulting code record to the generation digest, and publishes only after
all durable values have an encoding or recovery recipe. A rejected save preserves
the prior image.

Closures still cannot serialize as ordinary values. Keep shared blocks in methods
that answer fresh blocks, or in top-level `let` globals whose source-backed recovery
contract can rederive them.

## Booting

```text
birnam /tmp/app.image
birnam --rebuild /tmp/app.image
```

Normal boot validates the image format, source-map authority, generation digest,
code checksum, and code-to-generation binding before mapping executable bytes.
There is deliberately no old-format migration path.

`--rebuild` skips the mapped unit and evaluates the same complete authored module
documents, each exactly once and in dependency order. Both paths restore the same
generation digest.

What comes back:

- globals, entities, facts, and cells from the durable Store;
- the single current `DispatchGeneration` and its sealed source map;
- methods from the mapped unit or the complete-document rebuild;
- module singletons and Alien FFI wrappers, with external capabilities replayed;
- source-backed block globals, rederived after data and dispatch are restored.

## Main convention

If global `main` is a block, image boot runs it instead of opening the REPL:

```text
let banner := "my app";
let main := [| [System println: banner]];
[System saveImage: "/tmp/app.image"]
```

Module method changes should be activated through `CompiledUnit` before saving;
ordinary direct method edits are rejected while the runtime is running.

## Baking

```text
birnam --bake /tmp/app.image ./myapp
[System bakeExe: "./myapp"]
```

Baking appends the image to a copy of the runtime. The resulting executable still
resolves runtime dylibs from `<exe>/../lib` or `$BIRNAM_LIB_DIR`.

## AOT modules and source fallback

`zig build stage1` compiles the boot modules ahead of time. These artifacts stage
their exact canonical documents for G0; embedded source remains the floor. The
named `zig build test-source-fallback` gate proves both routes publish the same G0
digest.

Ordinary AOT `Module load:` is a bootstrap operation. After G0 it answers
`#moduleIntroductionNotAvailable`; existing modules evolve through checked
`CompiledUnit` successors. Later phases add module introduction and top-level
migration without changing the universal module representation.
