# The image — the system, made durable

> **Exact-current amendment (2026-08-27):** Design 60H retires the historical
> v4-v7 readers and the `replayMethods` / `deriveImageMethodTargets` source
> recovery path described below. Production accepts image-v8 only and restores
> portable meaning plus its exact stored selection; this record remains the
> history of the earlier slices, not a compatibility promise. Design 60O owns
> v8 boxed identity and the `Word` / `ForeignAddress` split.

> **Status:** Design accepted (2026-07-01). **Slice 1 SHIPPED (2026-07-01):**
> relocatable code — the `birnam_*` shims are dynamic symbols of the runtime,
> codegen has an artifact emission mode whose `__birnam_unit_init` re-derives
> baked global handles by name at load, and `[Codegen compile: src output:
> path]` / `[Codegen load: path]` round-trip a birnam program through a
> `.dylib` **across processes** (`examples/aot.birnam`).
> **Slice 2 SHIPPED (2026-07-01):** exact heap — per-vtable layout descriptors
> (`setLayout`: word-kind prefix + tail kind) on every pointer-bearing type,
> block vtables included (the `birnam_layoutAllOops` shim in emitted code), and
> a precise trace mode (`gcSetPrecise` / `[System preciseGc: true]` /
> `BIRNAM_GC_PRECISE=1`) validated against the conservative collector: the
> full test suite + every example runs precise, and precision demonstrably
> frees raw-word-aliased garbage conservative retains (docs/34 §F3). The fix
> sweep also rooted the grammar and gave peg's pattern/grammar/CST externals
> trace hooks — `[System gc]` mid-REPL used to segfault the next parse.
> **Slice 3 SHIPPED (2026-07-02):** the image itself. New `image` module + a
> record-stream format (header — magic/format/arch/ABI-validity hash — then a
> record stream read to EOF, heap before facts). `[System saveImage:]` /
> `birnam foo.image` (entry convention: global `main` → `value`, else REPL).
> Layering (approach A): the store yields/accepts facts RAW (`imageFactsInto:` /
> imageLoadBegin+AddFact+LoadEnd); the floor gained 3 IdApi primitives
> (`objectPayloadSize`, `exportedNameOf`, `slotKind`); image.zig owns the unified
> codec (leaf/symbol/heap-ref), the reachability walk, and swizzle (pass-1 alloc
> + relink-by-name, pass-2 patch — forward/cyclic refs OK). VERIFIED cross-
> process: leaf globals, vector tries, nested vectors, and HAMTs all round-trip.
> Fixed a latent bug it exposed — `keyHash` hashed symbols by ADDRESS (per-process),
> so HAMT bitmaps didn't survive; symbols now hash by NAME (also makes map order
> deterministic). Method SOURCE now lands in the store (codegen lowers
> `defineMethod:on:as:` → `…:source:`, recorded on `#__methods__`) and survives an
> image as recovery material. The following slice made compiled code primary and
> made unsupported durable values explicit save failures.
> Commits `bb366ba`→`f850d49`.
>
> **Slice 4 SHIPPED (2026-07-02):** mapped code. An image now carries the whole
> compiled library and BOOTS WITHOUT RECOMPILING (the user's directive: load the
> binary, replay only on request). At save, `[Codegen compileImageCode:]` gathers
> every recorded method source (the `__methods__` facts) into one relocatable,
> INSTALLABLE artifact — in artifact mode `defineMethod` lowers to a static
> trampoline + a direct `birnam_method` install (not the compiling runtime send) —
> and image.zig embeds it as the `.code` record. On boot loadImage writes it to a
> temp `.dylib` and `[Codegen load:]`s it: dlopen + install, no recompile. The
> header's arch tag + shim ABI-validity hash arbitrate: on a match the code is
> MAPPED (default); on a mismatch — or `birnam --rebuild foo.image` /
> `[System loadImage:rebuild:]` — it REPLAYS (`[Codegen replayMethods]` re-evals
> the `__methods__` source, subsuming the stdlib). Since the stdlib is itself all
> `defineMethod`, the `.code` section subsumes it — image boot no longer re-evals
> the stdlib. `[System loadImage:]` answers 3-state (nil/true/false) so runImage
> knows whether to fall back. VERIFIED cross-process + in a clean in-process test:
> a user method's code survives an image and runs from mapped code (no recompile),
> and rebuilds from source under `--rebuild`. Images grow to ~77KB (they carry the
> compiled library). Commits `d20b12b`→`44cfe16`.
>
> **Slice 4 follow-up — block-valued globals SHIPPED (2026-07-02):** the
> entry-point `main` closure (and any block-valued global) now survives an image
> (docs/40 §1/§6). A closure's VALUE still can't be serialized (image-born vtable +
> code pointer), so its fact is deferred at save as before — but a top-level `let`
> now also records the binding's SOURCE onto a reserved `__globalsrc__` entity
> (a String leaf, via a new `birnam_record_global_src` shim emitted alongside
> `birnam_global_set`). On boot, after code is resolved (mapped or replayed),
> `[Codegen deriveImageGlobals]` re-evals `let <name> := <src>` for every recorded
> global that came back UNBOUND (its value deferred) — leaf globals that mapped via
> the fact stream stay bound and are skipped. The re-derived closure sees restored
> captures (data loads before derivation). To make this reachable from a filed
> script, `runScript` now evaluates with `evalPersisting:` (top-level `let`s persist
> to Globals, as in the REPL) — so `let main := [| … ]` in a script leaves `main` in
> the store for `[System saveImage:]` and boots auto-run it. VERIFIED cross-process
> (mapped + `--rebuild`) and in-process (image_test); `examples/app.birnam` is the
> `main`-entry demo.
>
> **Slice 4 follow-up — the exe bake SHIPPED (2026-07-02):** a "runnable app" is
> a COPY of the birnam runtime with an image appended after a 16-byte trailer
> (`[image][u64 len][magic "BIRNMEXE"]`). Trailing bytes are ignored by both the
> Mach-O and ELF loaders, so the copy still executes as birnam; on startup it
> reads its own file, notices the trailer, and boots the embedded image via the
> ordinary boot path (mapped code, replay, global re-derivation, `main`-entry).
> Two ways to bake: `birnam --bake <image> <out-exe>` (an existing image) or
> `[System bakeExe: path]` (the running system, one step from a script).
> `resolveLibDir` now finds the runtime dylibs exe-relative (`<exe>`, then
> `<exe>/../lib`, then `$BIRNAM_LIB_DIR`, then `zig-out/lib`) and the raw id-dylib
> load uses it too — so a baked exe placed beside its dylibs boots from any cwd
> with no env. VERIFIED: `./myapp` from both bake paths, env-free from `/tmp`,
> mapped + `--rebuild`; a plain birnam has no trailer and is unaffected. No
> OS-level dual-format file (dyld/glibc fight it, §1) — "runnable" is this bake
> step over the same content, as designed.
>
> **Current persistence boundary (2026-07-17):** the earlier incremental-append
> prototype was withdrawn. The public image API supports atomic full replacement
> through `[System saveImage: path]` only; `[System appendImage: path]` is not a
> runtime selector and the old journal example has been removed. The record stream
> remains an implementation detail of the full image format, not an append journal.
>
> **Compiled-unit authority and integrity (2026-07-10; current v4 contract
> reconciled 2026-07-23):** the `.code` dylib is
> the image's primary executable layer, not a cache over source or store facts.
> Format v4 records exactly one current-generation authority digest followed by
> exactly one code unit bound to it, and preflights every record before a load
> mutates the live store; a corrupt or structurally ambiguous stream rejects the
> entire load. A full
> save captures code before classifying dependent meta-facts, then publishes the
> completed image with one rename. It rejects any durable value without an
> encoding or declared restoration path instead of silently dropping it. The
> fact store remains a durable data and reflection API: method facts mirror the
> compiled dispatch dictionary and are accepted only when the primary unit was
> captured. `[System imageSaveReport]` exposes saved/recovered/rejected counts,
> code-unit count, and a reason. The current implementation accepts and emits
> v4 only; v2/v3 are rejected. `--rebuild` and ABI/architecture fallback visibly enter source
> recovery rather than pretending to be normal boot. The next live replacement
> boundary is [45 — Compiled Units](../2026-10-runtime-refactors/45-compiled-units.md). Its accepted successor
> uses one sealed runtime-generation authority; the file-backed evolution workflow
> planned in [49 — Live Module Evolution](49-live-module-evolution.md) was
> withdrawn after the lower-level candidate boundary shipped.
>
> **Floor implementation update (2026-07-23):** images continue to depend only
> on the frozen `IdApi`, not its implementation language. The default floor is
> Rust; `image.zig` remains a Zig higher rung, and the separately named Zig floor
> is retained as the compatibility oracle used by the cross-floor image matrix.
>
> **Stackless image-v5 design boundary (2026-08-15; not implemented):**
> [60F](../2026-10-runtime-refactors/60f-resumable-compiler-lowering.md) owns the semantic requirement for a
> `Resumables` record, generation selection provenance, and tagged generated
> block-vtable references. [60I](../../design/60i-stackless-artifact-byte-grammar.md) is the
> sole authority for their exact v5 record tags, payload bytes, manifest-index
> encoding, limits, and fixtures. The current implementation remains v4 until
> those pre-implementation gates close and S2 lands; this note does not make v5
> readable or writable today.
>
> Builds on the
> [running system](../work-breakdowns/36-the-running-system.md) (globals are cells, method source
> is mirrored into the store) and the codegen/Tcc stack
> ([rung 3](../work-breakdowns/35-rung-3-front-end.md)). Supersedes the old kernel's
> `2e-images-aot.md` framing and closes out ADR-image-1's direction on the id
> stack. The old kernel implementation is preserved only in git history; the
> active image contract below is implemented by the current native
> `src/id/image.zig` image rung.

Today every `birnam` run starts from nothing: REPL bindings, defined
methods, and store facts all die with the process. The image is the fix — and
the design question was never *whether* to persist, but *what kind of thing*
the persisted artifact is.

## 1. What an image is (the accepted framing)

**An image is the live system made durable, not a log you re-execute.** By
default it is a natively compiled bunch of code plus the data it operates on,
with the property of being append-only. Booting an image means: map it in, fix
up addresses, start sending messages. No recompilation, no fact-by-fact replay.

Replay exists — the image carries its own source, so a system can be *rebuilt*
from it — but it is the recovery and portability path (cross-architecture
moves, runtime-ABI breaks, merging into a foreign image, repair), never the
boot path.

Three decisions bound the design (2026-07-01):

- **No OS-level dual-format file.** The original vision — one file that is
  simultaneously a dlopen-able library and an executable — fights the platform
  loaders (macOS dyld refuses to `dlopen` an `MH_EXECUTE`; glibc ≥ 2.30
  refuses PIE executables) and is given up. The image is one canonical
  artifact; "runnable" is provided by the runtime (`birnam foo.image`), and
  a standalone executable is a *bake* step over the same content
  (`tcc_output_file`, already proven for AOT).
- **Durability line = named live state.** The store reaches durable data and
  exposes reflection, but it is not the authority for executable behavior:
  compiled units own dispatch. Facts, globals, and their reachable values
  survive only when they serialize directly or have a declared reconstruction
  path through the compiled image or recorded source. Not the whole heap —
  anything unnamed is garbage by definition, and the GC agrees.
- **Entry point convention.** If the global `main` is bound, running the image
  sends it `value`; otherwise the image opens into the REPL with every binding
  live.

## 2. No changes file — dissolved, not worked around

Smalltalk needs runtime + image + changes because its image is opaque frozen
memory: source cannot usefully live inside it, so it is bolted alongside, in a
second format. The changes file does three jobs, and the id stack already does
two of them in the store itself:

1. **Source for tools** — `[Reflect defineMethod:]` mirrors method source into
   the store as facts. Source lives *in* the image, first-class and queryable.
2. **Audit trail** — the store has per-attribute opt-in tx history with time
   travel (`asOf:`). Strictly better than a linear `.changes` log.
3. **Crash recovery** — the one job a snapshot alone doesn't do. The answer is
   *not* a second format: because facts are immutable assertions, the store's
   natural physical layout is an append-only stream, and a crash journal is
   simply the image file being written incrementally. **The image is a
   compacted journal; the journal is an image in progress.** One format, two
   write modes (the Datomic/AOF shape). v1 may ship save-on-demand only, but
   the on-disk format must be a prefix-compatible append stream from day one
   so the journal is never a bolt-on.

A second dissolution came for free with the refounding: the old kernel's
M_USER agony ("which facts are user state vs reconstructed bootstrap?" —
ADR-image-1's whole fight) no longer exists. Native modules (prelude,
collection, error, relational, alien, …) install their methods at module load
*by construction* and are not in the store. The store holds durable data plus
the introspection mirror: user facts, globals, and method source. It travels
with the image only as encoded data or declared recovery material; it does not
supersede the compiled dispatch dictionary or silently authorize dropped state.

## 3. What must survive the trip

"Start up and send messages" decomposes into three survival problems.

### 3a. Compiled code — de-bake the absolute addresses

A Tcc unit today is JIT-relocated into anonymous memory with **absolute
process addresses baked in** — the one thing that cannot cross a save/load
boundary (ASLR alone kills it). Inventory of the baked sites:

| baked symbol | where | what it pins |
|---|---|---|
| `birnam_*` (16 shims) | `tcc.zig` `runtime_syms`, bound per-unit via `bindRuntime` | the runtime's shim fn addresses |
| `__g<i>` | `codegen.zig` `bakeGlobal`/`putGlobalRef` | a global's interned **symbol** address (value stays late-bound through the store) |
| `__cbBlock` | `alien.zig` callbacks | the callback block object |
| `__vecFactory` | `alien.zig` struct marshal | the Vector factory object |

Notably *already* relocatable: the defineMethod trampoline reaches its block
via `birnam_global("__mblk_N")` — late-bound through the store, no baked
address. Dispatch wiring survives; only code bytes and the table above pin.

The fix is to make compiled units **real relocatable objects**: export the
`birnam_*` shims as dynamic symbols of the runtime (so the OS/tcc linker
resolves them at load), and replace the baked object-address handles with a
per-unit init hook (`__birnam_unit_init`) that re-derives them at load time —
`__g<i>` by interning its name, `__cbBlock`/`__vecFactory` through the store
or the id env. Then "loading code" is relocation — milliseconds, done by the
linker — which is exactly the map-and-go boot. The JIT path keeps the baked
fast handles (nothing changes for a live session); the *emitted artifact* form
uses the relocatable spelling.

This is the old deferred "AOT for codegen output" item promoted to
load-bearing — and it ships independent value on its own: **compile birnam to
a dlopen-able shared object.**

### 3b. The store — an append-only mapped segment

Facts are immutable; the store appends. The image's data section is an
append-only segment holding the fact stream, the counters, and the
**interner** — the eid/name bijection was the old kernel's hardest-won lesson
(the durable anchor: everything relinks by interned name). Loading maps the
segment; incremental save appends new
facts and new compiled units; `[System saveImage:]` compacts. The mmap fast
path can come after a first version that reads the segment eagerly — same
logical format either way.

### 3c. The heap — exact layout, then swizzle

Store-reachable durability still drags an object graph along: a global bound
to `#[1 2 3]` is a cell whose value is an Oop into the GC heap (a trie), not
facts. Snapshots of heaps require knowing, for every object, exactly which
payload words are pointers — Smalltalk images work because the VM has exact
layouts.

Our floor is deliberately **conservative — but only about C stacks** (tcc code
has no stack maps; that is Piumarta's reason and it stands). At save time the
stack is quiesced, so stack conservatism is irrelevant; what matters is *heap
payload* exactness, and that is achievable: the GC already tracks every
allocation in `g_blocks`, and the vtable already carries one per-type hook
(`trace_hook`). The missing piece is the deferred F3 **per-vtable layout
descriptor**: a map of each payload — which words are Oops, which are code
pointers, which are raw bytes. Without it, swizzling is unsafe (an integer
that looks like a pointer gets corrupted — conservative marking tolerates
false positives, rewriting does not).

Swizzling must distinguish **three pointer kinds**:

- **Oop → heap object** — rewritten via the relocation map built from
  `g_blocks` at save.
- **fn-ptr → compiled code** — a vtable's method entries point into Tcc units;
  serialized as (unit, offset) and fixed against the relocated unit at load.
- **symbol → interner** — symbols are `.permanent` and identity-bearing;
  they serialize as names and re-intern at load (the durable anchor again).

**Vtables relink by name, not by copy.** An image-heap object whose vtable
belongs to a native module (VectorVt, smallint_vt, …) must be re-pointed at
the *freshly loaded* module's vtable at boot — deserializing a duplicate would
split identity and orphan the object from future method updates (the D5.1
relink-by-name spine, proven in the old kernel's W1). Image-*born* vtables
(block vtables, defineMethod protos) are the opposite: they come back from the
image with their compiled methods intact.

## 4. Replay — the fallback the source pays for

Because the store carries method source (and the fact stream), an image that
cannot be mapped can be **rebuilt**: recompile methods from their source facts
through the normal `defineMethod` path, re-assert facts, re-derive globals.
When it applies:

- moving architectures (arm64 image on x86_64 — code sections are per-arch);
- a runtime whose shim ABI hash no longer matches the image's manifest
  (the old `boot_validity` idea carries over: a validity hash gates trusting
  the compiled sections; mismatch degrades to replay instead of failing);
- loading an image *into* another image (§5);
- repair/migration tooling.

Pole A vs Pole B stops being two designs: compiled sections are the default,
and their absence (stripped or invalidated) *is* the replay path. One format:
code is per-architecture primary behavior; data and source are portable
durable/meta-level recovery material.

## 5. Load-as-library = merge through the surface

Loading image B into running image A is **not** a raw eid/pointer graft (eids
collide; two interners disagree). It goes through the surface: B's globals are
re-asserted via `Globals at:put:`, B's methods re-installed via their source
facts (or their relocatable units where the validity hash matches), B's facts
re-asserted into A's store. Identity re-materializes in the host; B's tx
history does not survive the merge (documented cost, right for v1 — "merge
with history" is a future refinement with real design weight).

## 6. Explicitly out (v1)

- **Blocks-in-globals that never came from source.** A hand-built closure
  stored in a global whose code exists only as a JIT unit with baked handles
  has no durable reconstruction story. Save rejects the image rather than
  silently dropping the binding; source-backed globals remain recoverable.
- **Alien callbacks / FFI state.** `g_callbacks` registry entries, dlopen'd
  library handles, and out-param cells re-establish on load by re-running
  their defining code (source-backed); otherwise save rejects the image.
- **Cross-image history merge** (§5).
- **The standalone-exe bake** — designed for (same content + `tcc_output_file`
  + embedded segment), built after map-and-go boot works.

## 7. Sequencing (plan of record)

Each slice is independently valuable before the image itself lands:

1. **Relocatable code** — export `birnam_*` as dynamic symbols; per-unit init
   hook replaces baked handles in emitted artifacts. **Ships:** compile birnam
   to a working `.dylib` (dlopen → send messages). Closes the "JIT-only for
   codegen output" caveat.
2. **Exact heap** — per-vtable layout descriptors (F3 completed); a precise
   trace mode validated against the conservative collector in tests.
   **Ships:** precise heap knowledge (also a GC improvement on its own).
3. **The image** — record-stream format (facts + interner + counters +
   heap section), save = serialize store-reachable graph via the descriptors,
   load = map + swizzle + relink vtables by name. **Ships:**
   `[System saveImage: path]`, `birnam foo.image`, `main`-or-REPL entry.
4. **Unify + harden** — compiled units as image sections with the validity
   hash; replay fallback wired; full-save integrity; the exe bake.

Slice 1 first: it is the smallest, it unblocks nothing-else-depends-on-me
value immediately, and every later slice needs its de-baking anyway.

## 8. Design note: the mmap fast-path for load (deferred)

> **Status:** design SKETCH only (2026-07-02). Not built. An adversarial review
> (same day, §8.9) found three load-bearing flaws in the specifics below — the
> oop-word encoding (§8.2), the treatment of strings (§8.2/§8.4), and the
> id-vs-offset addressing of facts (§8.3/§8.5). **Read §8.9 first.** The sketch's
> *spirit* — a per-arch, droppable fast heap segment over portable truth,
> mirroring the `.code` pattern — survives; the encoding and format details do
> not, and are rewritten there. §3b anticipated the deferral: "the mmap fast path
> can come after a first version that reads the segment eagerly."

### 8.1 What load does today, and the target

`loadImage` reads the *entire* file into a heap buffer, then walks the `.heap`
record and **re-materializes** every object: pass 1 `alloc`s a fresh GC object
per entry, pass 2 decodes each field (leaf inline, symbol re-interned by name,
export-ref resolved to the LIVE exported object of that name — format v2: a
fact holding a prototype/vtable/facade keeps identity instead of deferring or
materializing a shadow copy, the §3c relink spine applied to field values —
heap-ref resolved through the relocation table) into it. Boot cost = one full
file read + one full graph rebuild, every word allocated and copied, paid up
front before the first send.

The fast path replaces *read-all → alloc-per-object → decode-tagged-stream*
with *mmap → one swizzle pass*: objects live in the mapped pages and "load" is
pointer fix-up. **The real win is eliminating per-object `alloc` and tagged-field
decode — NOT lazy paging.** An earlier draft claimed "the OS demand-pages only
what boot touches"; that is false for the relocating swizzle (§8.6), which writes
every object's vtable slot and every oop word up front, faulting and COW-copying
the whole segment before the first send. Lazy paging only materializes under a
fixed-base map (no swizzle) or a read-barriered lazy swizzle — neither is v1.
Facts are **not** unchanged either (§8.3/§8.9-#3). This paragraph is corrected
from the original sketch per §8.9.

### 8.2 A layout-faithful segment: a new `.mheap` record

Today's `.heap` is a *tagged* stream (a type byte per field). The fast segment
is instead **byte-for-byte the in-memory object image**, so a mapped page *is*
the heap. The `-1` trick fixes the layout — an object is `[vtable ptr]` at
`payload[-1]`, then payload words — so each object is stored:

```
   ... [ nwords ] [ vtable-index ] [ w0 ] [ w1 ] ... [ w_{n-1} ] ...
                    ^payload[-1]     ^payload starts here
   live object pointer  =  &vtable-index + WORD   (i.e. &w0)
```

- `payload[-1]` holds a **vtable-index** pre-swizzle, a live **vtable pointer**
  post-swizzle — exactly where dispatch reads it. The segment carries the vtable
  slot for free.
- `payload[-2]` = `nwords`, used only to walk object→object; a "gap" the live
  object never points at (invisible to a precise trace).
- payload words are raw, in-memory width, `WORD`-aligned.

**Oop-word encoding**, driven by `slotKind` (already available — no per-field
tag bytes, unlike the tagged stream):

| word (`slotKind`) | on disk | swizzle |
|---|---|---|
| `slot_raw` | verbatim | untouched |
| `slot_code` | 0 | stays 0 (re-derived, as today) |
| `slot_oop` = SmallInt | verbatim (low-bit tagged → position-independent) | untouched |
| `slot_oop` = nil/true/false | reserved sentinel (0/1/2) | → live singleton |
| `slot_oop` = symbol | `SYMTAG \| sym-table index` | → interned-by-name pointer |
| `slot_oop` = heap ref | **byte offset** of target's payload in the segment | → `base + offset` |

Two deduped side tables: a **symbol table** (names → interned once → index→ptr)
and a **vtable table** (`exportedNameOf` names → `import`ed by name → index→ptr).
Only native/exported vtables appear — the same rule `internObj` already enforces
by deferring image-born (block) vtables. The `.mheap` payload is **page-aligned**
in the file (pad the record header) so `mmap` maps it directly.

> ⚠️ **Broken as written (§8.9-#1, #2, #6).** The tagless single-word encoding
> above does not decode: sentinel `1` (`true`) equals the word `mkSmallInt(0)`,
> and `0` is ambiguous across `nil` / null-slot / offset-0 — it needs a real tag
> scheme. And **strings/floats are inline leaves today, not heap objects**
> (`encodeField` intercepts them before `internObj`); forcing a String into a
> layout-faithful heap object exposes its `slot_raw` `ptr` word as an *out-of-line*
> `c_allocator` pointer whose bytes are never in the segment — `slot_raw` means
> "don't trace," not "not a pointer." Both must be redesigned before an emitter
> can be written. "Pad the record header" also breaks the self-delimiting framing
> and page size is arch-dependent (16K/4K) — an extra gate dimension.

### 8.3 Load: mmap + two-pass swizzle

```
1. mmap(.mheap range, MAP_PRIVATE | PROT_READ|PROT_WRITE) → base B
2. intern the symbol table  → sym[]   (index → live symbol)
3. import the vtable table  → vt[]    (index → live vtable; relink-by-name)
4. pass A: for each object, payload[-1] = vt[ stored vtable-index ]
5. pass B: for each object, walk payload by slotKind(vt, w); rewrite each oop
        word (immediate | sentinel | SYMTAG idx | offset) to a live pointer in place
6. register [B, B+len) as a GC arena: precise-traced (setLayout), PERMANENT
        (image objects are not freed — like symbols)
```

Pass A precedes pass B because pass B needs each object's live vtable to read
`slotKind`. The win: no per-object `alloc`, no tagged decode.

> ⚠️ **Two problems here (§8.9-#3, #4).** (a) Facts and journaled deltas reference
> heap objects by a **u32 id** into `g_load_reloc` (built in id order by the eager
> loader), not by byte offset — so an offset-addressed `.mheap` that "drops ids for
> offsets" (§8.4) leaves facts and delta→base refs unresolvable. The loader must
> still reconstruct the id-ordered pointer array, which means `.mheap` cannot
> actually discard the saver's id numbering. (b) Step 6's PERMANENT arena is both a
> leak regression (today's objects are `.collectable`) and unsafe: the GC traces
> via `g_blocks` and has no way to trace *into* a foreign mmap'd region, so storing
> a fresh collectable Oop into a mutable mapped slot yields a dangling pointer on
> the next collection. Honest arena tracing (walk by layout, treat as roots) is
> required — more than "one hook."

### 8.4 Save: a binary-packed `drainHeap`

The same graph walk and `slotKind` logic as `drainHeapFrom`, but it assigns each
object a **byte offset** (not an integer id) and packs
`nwords · vtable-index · payload` with oop words rewritten to
offsets/sentinels/symtags. A sibling emitter over the identical walk.

### 8.5 Arbitration — the `.code` pattern, applied to the heap

The mapped segment is **per-(arch, word-size, layout-ABI)**: add a `layoutHash()`
gate beside `abiHash()`. On load, `.mheap` present *and* layout-compatible →
mmap fast path; else fall back to the portable `.heap` (eager), or replay if
absent. A saver emits `.mheap` for local/fast images and `.heap` for
portable/archival ones (or both; compaction chooses) — the same "per-arch,
droppable fast section over portable truth" duality the `.code` section
established. No new conceptual machinery; map-vs-replay applied to the heap.

### 8.6 The one real trade-off: relocate vs. fixed-base

- **Relocating swizzle** (above): robust against ASLR, but writing every pointer
  word dirties pages, so `MAP_PRIVATE` copy-on-write sharing breaks on touched
  pages. Still wins on alloc/decode; loses cross-process page sharing.
- **Fixed-base map** (classic Smalltalk): store real addresses relative to a
  fixed load base, `mmap(MAP_FIXED)` there, so internal oop pointers need *no*
  swizzle — only vtables/symbols get patched and unwritten pages stay shared.
  But `MAP_FIXED` at a hardcoded base fights ASLR and can collide.

v1 takes the relocating path; fixed-base is a later "many processes share one big
image" optimization.

### 8.7 Scope, risks, interactions

- **GC arena** is the fiddly bit: the mapped region must be registered so tracing
  follows into it and never frees it. Non-moving GC helps (addresses stay put);
  precise mode already exists.
- **Lazy swizzle** (swizzle-on-first-touch via a read barrier) is rejected — our
  dispatch has no read barrier and the GC is conservative-by-default; too
  invasive for the payoff.
- **Journaling (§7.4) composes**: mmap the base generation's `.mheap`; appended
  deltas stay eager `.heap` (small); compaction re-emits one fresh `.mheap`.
- **Touch points**: `image.zig` (emitter + loader path + `layoutHash`), one
  GC-arena registration hook in the floor, header/arbitration. Codegen, the
  store, and boot orchestration are untouched.

### 8.8 Cost / benefit

Self-contained, concentrated in `image.zig` plus GC-arena tracing. The payoff is
**~zero at today's sizes** (tens–hundreds of KB, where the `.code` dlopen
dominates) and only shows with multi-MB persisted heaps — which is why it is
parked as depth, not shipped. Given §8.9, do NOT prototype the emitter until the
encoding, string, and addressing problems below are resolved on paper first.

### 8.9 Open problems (adversarial review, 2026-07-02)

A review against the actual floor (`id.zig`, `image.zig`) found the §8.1–§8.8
sketch has load-bearing flaws, not just gaps. Gate items before any build:

1. **Oop-word encoding is undecodable (blocker).** The four-way single-word scheme
   (§8.2) has no room for its cases: `mkSmallInt(0)` is the word `1` = the `true`
   sentinel; `0` is ambiguous across `nil` / null-slot / offset-0. SmallInt already
   owns bit0 across 63 bits (`id.zig` `mkSmallInt`), and offsets are 8-aligned, so
   a real tag scheme (or a per-slot side-encoding) is needed. The current tagged
   codec keeps `nil` (tag 0) and null (tag 8) distinct (`image.zig` `Tag`) — mheap
   must too.
2. **Strings/floats are inline leaves, not heap objects (blocker).** `encodeField`
   serializes String/Float/int/singleton inline and only routes *compounds* to
   `internObj`. A String's `{ptr,len}` payload is `slot_raw`, and `ptr` is an
   out-of-line `c_allocator` pointer — "layout-faithful, one word per slot" dangles
   it and never stores the bytes. Needs an out-of-line-bytes section + rebasing, or
   strings stay variable-width (abandoning the pure-mmap premise for them).
3. **Facts/deltas are id-addressed, mheap is offset-addressed (blocker).** `.facts`
   and journaled `.heap` deltas use u32 ids into `g_load_reloc`. `.mheap` must
   preserve the id numbering (or migrate facts + deltas to offsets, a format
   change). "Facts unchanged" (§8.1) and "offset→pointer map" (§8.3) contradict.
4. **Permanent arena leaks + GC can't trace it (regression + UAF).** Today's loaded
   objects are `.collectable`; a permanent mapped arena retains every loaded image
   forever. And the collector is keyed on `g_blocks` with no range/arena tracing,
   so a fresh collectable Oop stored into a mutable mapped slot is unreachable to
   the marker → freed → dangling. Needs genuine arena tracing, not "one hook."
5. **Lazy-paging was oversold.** The relocating swizzle faults + COW-copies every
   page up front; the real win is alloc + decode elimination only (§8.1 corrected).
6. **Framing + page size.** Page-aligning the payload breaks the self-delimiting
   record framing unless a pad field is added and *both* loaders skip it; and page
   size (16K Apple / 4K x86) is an unkeyed dimension in `layoutHash` (§8.5).
7. **The `payload[-2]` nwords word** is a layout the floor doesn't model (one
   pre-payload word, `VT`), and is only trace-safe in the non-default precise mode.

**Net:** the *pattern* (droppable per-arch fast heap segment over portable truth,
via `slotKind` / `exportedNameOf` / relink-by-name) is sound and those primitives
work as claimed. The *encoding and format* in §8.2–§8.5 do not, and must be
redesigned — resolve #1–#3 on paper before writing a line of emitter.
