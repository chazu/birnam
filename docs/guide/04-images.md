# Images: save, boot, bake

Images preserve imageable live objects, method code/captures, source metadata and
explicit Store state. Ordinary edits can be saved without an adopted Generation;
managed revision metadata that has drifted remains descriptive.

## Saving

```text
[System saveImage: "/tmp/app.image"]
```

Full temporary-file replacement is the only supported write mode. `appendImage:`
is not part of the generation format: source, facts, and executable code cannot be
advanced as independent journals without recreating split authority.

`saveImage:` answers true on success and nil on rejection; owner errors can also
be returned. Inspect `[[System imageSaveReport] detail]` when it refuses. Saving
captures actual live code and imageable state, including source-backed methods
introduced by scripts and the REPL. It publishes only after validation and complete
encoding. A rejected save preserves the prior image file.

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

Ordinary owners can customize their saved body with `imagePayload`. The default
returns `self`; returning `false` refuses persistence:

```text
[Reflect defineMethod: #imagePayload on: resource as: [:self | false]]
```

An ordinary slot object can instead return a separate ObjectModel-readable payload.
Its slots are serialized under the original object's identity and behavior, so
aliases remain aliases and the live object need not change. This does not persist
any separate native context owned by the returned payload. Native representations
reject replacement payloads. A different Error result propagates; ordinary Error
objects remain imageable through the default `self` result. Task handles reject
persistence even after completion; factory clones and copied outcomes can be saved.
These are current owner messages, not declarations in a global Store schema.

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

Normal boot validates the image and native code before mapping the saved code.
`--rebuild` compiles the saved portable programs instead. Mapped startup does not
reparse source; both paths restore captured objects, aliases, methods and explicit
facts. Foreign wrappers use their restoration recipes for current process addresses.
Startup failure discards the world before application execution.

## Loading into a running world

```text
[System saveImage: "/tmp/before-load.image"]
// Check that saving succeeded before continuing.
[System loadImage: "/tmp/other.image"]
```

Live loading is an advanced operation with **no automatic rollback**. Save first
if you want a recovery snapshot. If applying the image fails, already-applied
changes remain. Inspect `[[System imageSaveReport] status]` and `detail`:
`#partialLoad` means the world may contain a mixture of prior and imported state.
The original error is retained; stale prepared/adoption capabilities are abandoned.

No automatic checkpoint, retry, restart or invocation of main occurs in the loader.
To recover, you can discard the failed world and start a fresh runtime from your
saved image. Work since that snapshot is lost; external effects, connections and
active execution are not rewound. A subsequent live load is another explicit
attempt, not a guaranteed recovery procedure. Validation and code lifetime safety
remain required even though application is not transactional.

## Main convention

If global `main` is a block, image boot runs it instead of opening the REPL:

```text
let banner := "my app";
let main := [| [System println: banner]];
[System saveImage: "/tmp/app.image"]
```

`CompiledUnit` remains the lower-level candidate mechanism; selection-bearing
candidate AOT produces the same candidate envelope and activation path. Ordinary
direct method edits remain available while the runtime is running; explicit
Generation operations retain their own admission rules.

## Baking

```text
birnam --bake /tmp/app.image ./myapp
[System bakeExe: "./myapp"]
```

Baking appends the image to a copy of the runtime. The resulting executable still
resolves runtime dylibs from `<exe>/../lib` or `$BIRNAM_LIB_DIR`.

## AOT modules and source fallback

The Rust installer builds compiled bootstrap modules and prepared startup images
from source. Its grammar seed is build-only and is excluded from the installed
runtime. Compiled artifacts carry the programs and source metadata needed for
activation without reparsing their source.

`Module load:` follows the current module dependencies and artifact admission
protocols. New module introduction is an ordinary loading operation. Explicit
`CompiledUnit` candidate/revision workflows remain available; they are not a
mandatory coordinator around module loading or ordinary method edits.
