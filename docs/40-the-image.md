# The image — the system, made durable

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
> **Slice 3 SHIPPED (2026-07-02):** the image itself. New `image` module + an
> append-ready format (header — magic/format/arch/ABI-validity hash — then a
> record stream read to EOF, heap before facts). `[System saveImage:]` /
> `birnam-id foo.image` (entry convention: global `main` → `value`, else REPL).
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
> image — the portable truth slice 4's replay needs. DEFERRED (counted, reported):
> compiled code + block-valued globals (image-born vtables) — slice 4's job.
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
> `main`-entry demo. REMAINING (follow-ups, not core): the exe bake; incremental
> append + compact (§7.4).
>
> Builds on the
> [running system](36-the-running-system.md) (globals are cells, method source
> is mirrored into the store) and the codegen/Tcc stack
> ([rung 3](35-rung-3-front-end.md)). Supersedes the old kernel's
> `2e-images-aot.md` framing and closes out ADR-image-1's direction on the id
> stack. Reference-only salvage from the old kernel lives in `src/attic/`
> (`image.zig`, `freeze.zig`, `loader.zig`, `macho.zig`, `elf.zig`).

Today every `birnam-id` run starts from nothing: REPL bindings, defined
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
  artifact; "runnable" is provided by the runtime (`birnam-id foo.image`), and
  a standalone executable is a *bake* step over the same content
  (`tcc_output_file`, already proven for AOT).
- **Durability line = store-reachable.** What survives is what the system can
  name: the store's facts, the globals cells, and everything transitively
  reachable from their values (plus the vtables and compiled code those
  objects need). Not the whole heap — anything unnamed is garbage by
  definition, and the GC agrees. This forces the right discipline: persistent
  state is state the store can see.
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
*by construction* and are not in the store. What the store contains is exactly
the persistable delta: user facts, globals, method source. Serialize the whole
store, no filter — the persistence boundary IS the store boundary.

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
(the durable anchor: everything relinks by interned name; see
`attic/image.zig`). Loading maps the segment; incremental save appends new
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

Pole A vs Pole B stops being two designs: the compiled sections are the
default, and their absence (stripped or invalidated) *is* the replay path.
One format; the code sections are per-arch and droppable, the data + source
sections are portable truth.

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
  stored in a global whose code exists only as a JIT unit *with* baked
  handles: v1 persists blocks only where their unit is relocatable or their
  source is in the store. Documented, not silent — the save reports what it
  skipped (the attic rule: counted, never dropped).
- **Alien callbacks / FFI state.** `g_callbacks` registry entries, dlopen'd
  library handles, and out-param cells re-establish on load by re-running
  their defining code (source-backed) or are reported skipped.
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
3. **The image** — append-only segment format (facts + interner + counters +
   heap section), save = serialize store-reachable graph via the descriptors,
   load = map + swizzle + relink vtables by name. **Ships:**
   `[System saveImage: path]`, `birnam-id foo.image`, `main`-or-REPL entry.
4. **Unify + harden** — compiled units as image sections with the validity
   hash; replay fallback wired; incremental append + compact; the exe bake.

Slice 1 first: it is the smallest, it unblocks nothing-else-depends-on-me
value immediately, and every later slice needs its de-baking anyway.
