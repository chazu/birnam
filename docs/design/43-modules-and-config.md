# 43 — Modules, the Self-Hosted Layer, and Runtime Configuration

> **Status:** implemented 2026-07-07 (commits `e8616b6`..`ada506c`). Builds on
> [40 — The Image](40-the-image.md) (the relocatable `.code`/artifact machinery this
> reuses), [42 — Object Canonical](42-object-canonical-relational-derived.md) (the
> store as MOP + relational data), and the retired `2c-modules.md`/`2d-ffi.md`
> (legacy-kernel module/FFI notes — superseded).
>
> Background: `src/id/modules.zig`, `src/id/config.birnam`, `src/id/toml.birnam`,
> `src/id/stdlib.birnam`, `loadStdlib`/`loadStack` in `src/id/main.zig`,
> `mCompileOutput` in `src/id/codegen.zig`.

## The through-line

birnam-source libraries are **compiled to installable dylibs and loaded that way by
default** — with **eval-from-embedded-source kept as the floor/fallback**. On top of
that: a **store-native module manager** whose dependency graph is ordinary facts, a
**runtime `[Module load:]`** that resolves and loads dependencies on demand, and a
**`birnam.toml`** config format parsed by birnam's own self-hosted TOML module. The
language configures and extends itself in itself.

## Two senses of "module"

- **Kernel modules** — the Zig primitive dylibs (`store`, `peg`, `lang`, `tcc`,
  `codegen`, `prelude`, `collection`, `error`, `relational`, `alien`,
  `image`). Hand-ordered in `script_modules` (`main.zig`), `dlopen`+`module_init`.
  They *are* the language; nothing above bootstraps them. (`modules` left this
  tier 2026-07-10 — the manager is birnam now, docs/44 Phase 5b.)
- **Self-hosted modules** — birnam source (`modules`, `stdlib`, `toml`, `config`,
  and any user module). Compiled to `libbirnam_<name>.dylib` and installed by
  loading, or eval'd from source as the floor. `modules` (the manager itself) is
  the special first one: loaded by name before the rest, since it implements the
  sequencing the rest load through; it self-registers at its END (the method has
  to exist before the declaration can run).

## Compile-to-dylib; source is the floor

A birnam-source module compiles to an **installable** dylib via the existing
artifact path (`[Codegen compile:output:]`): artifact-mode `defineMethod` lowering
pre-compiles each method body and installs its trampoline statically at load, and
the unit's top-level (`__ev`) runs on load to bind globals and register itself — so
loading installs with **no recompile**. No new codegen was needed for this.

- `birnam --compile-module <src.birnam> <out.dylib>` drives it (brings up the stack
  *without* the stdlib — compiling a body's late-bound sends needs no target method
  installed, which is what lets the stdlib compile itself).
- `zig build stage1` is the **two-stage build**: stage 0 ships a binary that embeds
  each self-hosted source; stage 1 *runs that binary* to AOT-compile the sources into
  `libbirnam_{modules,stdlib,toml,config}.dylib`.
- `loadStdlib` **prefers the dylib** (`[Codegen load:]`, no recompile) and **falls
  back to the embedded source** when the dylib is absent, stale, or ABI-mismatched.
  `BIRNAM_NO_STDLIB=1` skips the whole self-hosted layer (the bare Zig floor).

> ⚠️ **Authoring gotcha.** Because `loadStdlib` prefers the dylib, **editing a
> self-hosted source (`stdlib`/`toml`/`config`) has no effect until you refresh the
> dylib** — a stale one silently shadows your change and the runtime keeps running the
> old code (tests pass against it). After editing, either `rm
> zig-out/lib/libbirnam_stdlib.dylib` (force the source path) or `zig build stage1`
> (regenerate the dylib). A plain `zig build` re-embeds the source but does **not**
> refresh or invalidate the dylib, so in practice the stale dylib wins until you `rm`
> it or run `stage1` (observed 2026-07-09 — the fallback triggers on an *absent* or
> *ABI-mismatched* dylib, not a merely out-of-date one).

### The `__amblk` salt

Artifact-mode method blocks are stashed under `birnam_global("__amblk<N>")` — a
process-wide string namespace whose counter resets per compile. Two separately
compiled dylibs both wrote `__amblk0…`, so the later-loaded one clobbered the
earlier's blocks (a method then fetched a wrong-arity block and DNU'd on
`value:value:`). Fixed by **salting the keys with a per-unit source hash**
(`artifact_salt`, `codegen.zig`). Nested blocks were already safe (direct C symbols
under macOS two-level namespace).

## `Module` — the store-native manager

A dependency graph *is* relational data, so `Module` forwards to the store like
`Globals`; the graph serializes into the image for free.

- `[Module named: #x requires: #[ … ]]` — a self-hosted module's **first-statement
  self-declaration** (records the `requires` edges).
- `[Module kernel: #store]` — **represents** a kernel module (registered, kind
  `#kernel`, `#loaded`) without loading it.
- `[Module loadOrder: #[ … ]]` — the names + transitive deps, **topologically
  sorted** (DFS post-order, deps first). A cycle answers `#circularDependency`
  (mirrors the [G4](../archive/work-breakdowns/41-mvp-unity-audit.md) left-recursion catch), never loops.
- `requiresOf:` / `kindOf:` / `isRegistered:` / `isLoaded:` round out the queries.

### Boot order is derived, not hardcoded

`loadStdlib` no longer hardcodes `stdlib→toml→config`. The host has the embedded
sources, so `scanRequires` reads each module's `requires:` declaration *without
loading it* (the pre-load knowledge that resolves the chicken-and-egg), registers
the edges, and `[Module loadOrder:]` decides the install order. Proven by reversing
the source table and observing the install order stay `stdlib→toml→config`.

## `[Module load:]` — runtime dylib loading

The fundamental "load a module at runtime" capability, with automatic dependency
resolution:

`[Module load: #x]` resolves `<libDir>/libbirnam_x.dylib`, loads its transitive
dependencies **first**, installs it, and marks it `#loaded`. Idempotent (the
`#loaded` fact); a cycle answers `#circularDependency`; a missing dylib answers
`#loadFailed`; neither crashes nor hangs.

**Pre-install requires without external source** is solved by the compiler
embedding a pointer-free artifact descriptor. `[Artifact inspect:]` reads that
descriptor from the dylib bytes without `dlopen`, checks identity/ABI/target and
native requirements, and returns ordinary ID values. `load:` recurses through
`requires` before `[Codegen load:]` runs `__birnam_unit_init` + `__ev` to install.

The host sets `[Module libDir:]` (its resolved lib dir) and marks the boot-time
self-hosted layer `#loaded`, so a later `load:` of `stdlib`/`toml`/`config` as some
module's dependency is a no-op. v1 is **name-based**: user modules live in the lib
dir (or a `BIRNAM_LIB_DIR` you control) as `libbirnam_<name>.dylib`. Project
builds publish into their private `.birnam/build/<target>/debug/lib`; the
host-aware bare REPL and `birnam run` point both Module and Alien at that
directory, while `birnam install` copies the validated project publication into
the runtime directory for use outside the project build tree.

## Image composition

An image captures compiled behavior as its primary executable layer. At save,
`[Codegen compileImageCode:]` compiles both the recorded whole-module programs
(`__modsrc__`) and individual method definitions (`__methods__`) into a
relocatable image unit. This matters for module top-level initialization: a
module-owned PEG grammar or named singleton may have no direct heap encoding,
but its compiled module program recreates it on mapped boot.

The source records remain in the store for reflection and explicit recovery.
`--rebuild` replays module programs before individual method sources; normal
compatible boot maps the image unit without recompiling. See
[45 — Compiled Units](45-compiled-units.md) for the authority and integrity
contract.

The sealed current generation's ordered source map is authoritative and
`__modsrc__` remains a compatibility mirror. File-backed live evolution is not
part of the current runtime surface.

## `birnam.toml` — runtime configuration

`Config` (`config.birnam`, `requires: #[#toml]`) discovers the nearest
`birnam.toml` from cwd upward and exposes it parsed as a Map — **pure birnam**
over `[System readFile:]` + `[Toml parse:]` (birnam's own PEG-based TOML parser).
A missing or malformed file is not an error: `map` answers nil and every accessor
degrades to nil, so a project without config just works.

- `[Config at: "key"]` — any value; `[Config main]` — the root-resolved entry
  point; `[Config loads]` — the normalized `load` collection.
- On **bare/`--repl`** boot, `main` names a source file run before the prompt (a
  `birnam <file>` invocation names its own entry, so it's ignored there).
- Discovery walks **cwd upward** to the nearest `birnam.toml` without changing
  process cwd. `main` is resolved relative to that project root, and `load`
  names compiled modules for dependency-first `[Module load:]` before `main`.
  Demo: `examples/config/`.

## Open items (next session)

- **`image` config key** — `image = "x.image"`
  is harder: image loading is a whole boot *mode* (`runImage` orchestrates
  loadStack→loadImage→deriveImageGlobals), an awkward chicken-and-egg with reading
  config after the stack is up.
Path-selected roots are now handled by `[Module loadArtifact:]`; the embedded
descriptor supplies their canonical identity while dependencies resolve beside
the root through the ordinary library directory.
