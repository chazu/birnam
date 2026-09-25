# Images: save, boot, bake

During Design 90, full image save/restore remains unavailable. Store's
[row-transfer protocol](../../rust/birnam-store/README.md#image-transfer-and-remaining-integration)
is reconnected, including Store-backed globals, but Image still needs migration.
The workflows below describe the behavior to restore, not current end-to-end
acceptance.

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
then includes ordered source-backed method operations recorded by direct scripts
and REPL forms. It binds the resulting code record to the generation digest and
publishes only after all durable values have an encoding or recovery recipe. A
rejected save preserves the prior image.

Live `StringBuilder` buffers are not portable image values. Materialize them
with `[builder asString]` before saving, and construct fresh builders inside
`main` when the image needs mutable text assembly.

Retained `Method` instances are also process-local. Saving one in durable state
answers `#methodNotImageable`, including when it has a private behavior. Save
the information needed to find the method, and call `methodFor:` after boot.
The canonical `Method` prototype is a normal named native reference.

Portable Blocks and CodeObjects retain their code and captures in the image.
Source-backed global restoration uses the saved compiled generation. Process-local
native resources need an explicit restoration recipe.

`System saveImage:` is deliberately the only image-save workflow. It persists the
current runtime state through the low-level atomic full-save primitive; source
editing and candidate acceptance are intentionally not hidden behind another
workflow facade.

## Booting

The native runtime loads its tracer and collection coordinator before restoring
an image. Saved and baked images use the same collection path as ordinary
startup; no collector state or callback addresses are added to the image format.


```text
birnam /tmp/app.image
birnam --rebuild /tmp/app.image
```

Normal boot validates the image format, source-map authority, generation digest,
code checksum, and code-to-generation binding, then maps the saved compiled code.
Images save both the portable Block/binding unit and the compiled source-generation
unit. Source-defined methods are compiled when saving, not on every startup.
An earlier v8 image missing the source-generation unit uses embedded-source
recovery and reports that outcome; there is no old-format migration path.

`--rebuild` skips the captured native unit, recompiles the same accepted
GenerationProjection from durable source and selection, and maps that fresh
unit before replaying external capabilities. This includes methods defined by
a standalone script or REPL session. Both paths restore the same generation
digest and selected carriers.

What comes back:

- globals, entities, facts, and cells from the durable Store;
- the single current `DispatchGeneration` and its sealed source map;
- methods from the mapped unit or freshly compiled projection, including
  source-backed methods defined directly by a script or REPL;
- module singletons and Alien FFI wrappers, with external capabilities replayed;
- projected block globals, rederived by the mapped or freshly rebuilt unit.

## Main convention

If global `main` is a block, image boot runs it instead of opening the REPL:

```text
let banner := "my app";
let main := [| [System println: banner]];
[System saveImage: "/tmp/app.image"]
```

`CompiledUnit` remains the lower-level candidate mechanism; selection-bearing
candidate AOT produces the same candidate envelope and activation path. Ordinary
direct method edits are rejected while the runtime is running.

## Baking

```text
birnam --bake /tmp/app.image ./myapp
[System bakeExe: "./myapp"]
```

Baking appends the image to a copy of the runtime. The resulting executable still
resolves runtime dylibs from `<exe>/../lib` or `$BIRNAM_LIB_DIR`.

## AOT modules and source fallback

The Rust installer compiles the boot modules ahead of time. These artifacts
stage their exact canonical documents for G0; embedded source remains the floor.
The Rust workspace tests prove both routes publish the same G0 digest.

Ordinary AOT `Module load:` is a bootstrap operation. After G0 it answers
`#moduleIntroductionNotAvailable`; existing modules evolve through checked
`CompiledUnit` successors. Later phases add module introduction and top-level
migration without changing the universal module representation.
