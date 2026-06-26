# Images, Freeze, and AOT

How a running Birnam persists and ships. `src/kernel/image.zig` is the `.bim`
snapshot format; `src/kernel/freeze.zig` compiles an FFI wrapper dylib;
`src/kernel/loader.zig` + `macho.zig` + `elf.zig` are the binary-format readers;
`src/aot_main.zig` is the ahead-of-time driver that turns an image into a
standalone executable.

> Implementation: `image.zig`, `freeze.zig`, `loader.zig`, `macho.zig`,
> `elf.zig`, `aot_main.zig`. Concept: [The Vision](10-vision.md) §Execution
> Modes, §Image.

## The `.bim` image — `image.zig`

An image is the serialized store plus enough metadata to reconstruct identity.
`MAGIC = "BRNIMG\x00\x01"` (8 bytes). Two writers share one format:
`write` stamps `FORMAT_VERSION = 6` (an **M_USER overlay** — user data only), and
`writeFull` stamps `FULL_FORMAT_VERSION = 8` (a **full image** — every fact, the
whole tower). `MIN_LOAD_VERSION = 5`; v5–v8 share every section layout (later
versions only ADD value kinds / the v8 `boot_validity` field), so a newer loader
reads an older image verbatim. The file holds, in order:

- **Manifest** — `bytecode_format_version`, `thread_mode` (`single`/`datomic`),
  the module dependency set (`{name, abi_hash}` each), and (v8) `boot_validity` —
  a hash of the kernel eid-layout signature folded with each module dylib's
  content, gating whether a full image can be trusted as a boot cache (W1 Stage
  3a). Embedded in the file (no sidecar) since v4.
- **Interner section** (v5) — the eid↔name bijection, so binding and method-name
  identity survive reload (the [interner](24-interner.md) rationale). **This is
  the durable anchor:** under W1, native modules relink their methods onto
  image-restored eids *by interned name* (D5.1), so they load in any order.
- **Counters** — `next_tx`, `next_eid`.
- **Facts** — iterated via `meatIter`. **`writeFull` persists ALL facts**
  (bootstrap, module init, user data) — the W1 model: a full image IS the booted
  system, loaded not recompiled. `write` keeps the legacy **M_USER-only** filter
  for lightweight overlay saves (`m` is then a persistence selector); but `m` is
  fundamentally a *provenance tag*, not a gate — full images carry every `m`. The
  irreducible residue `writeFull` skips is ~9 native fn-ptr facts (`function-impl`
  / `reader-impl`), relinked by name on load like FFI wraps. See `kernel_eids.zig`
  `M_USER`.

Values are tagged by kind: `VK_IMMEDIATE` (self-describing Ref verbatim),
`VK_STRING` (len + bytes), `VK_FLOAT` (f64 bits, re-boxed), `VK_RRB` (count +
recursive elements), `VK_CAPTURES` (a `:method-captures-ptr` array), and
`VK_MAP` (v6 — an `ASSOC_VT` persistent map: entry count + each `(key, value)`
pair, recursive, so a map literal embedded in a method's constant-pool or an
`:ast-value` round-trips instead of being silently dropped).

### Save and load

- `write` / `saveToFile` serialize; `load` / `loadFromFile` **replay**: facts are
  re-`assert`ed into a fresh store, the interner is rebuilt with `restoreEntry`,
  and the counters are restored to their monotonic floor.
- `loadFrozen` uses the lean `frozenInsert` path (no tx churn / history) for the
  Phase-14 `--strip=history` mode.
- `rematerializeClosures` rebuilds the runtime closures the image can't serialize:
  a fn-ptr `Closure` for each `:bytecode-stream` method, plus **durable block
  globals** — captureless and capturing block-closure wrappers (W1 Stage 2/2b, via
  `:closure-block-method` / `:closure-block-captures` recipe facts) and lang's
  REPL **tree-walker** block globals (via `:treewalk-block-ast`/`-scope` + a
  module-registered shim). So `let f := [block]` survives save → load.
- `peekManifest` parses just the manifest (no facts), which the dynamic-AOT loader
  uses to learn module deps before dlopen.

Load is replay-based today; the mmap-segment fast path is a deferred Phase-12.5
optimization the single-file layout already allows.

### Booting from an image — the auto compile-cache (W1)

A full image **is** the booted system: load it instead of recompiling the tower.
The default `birnam` boot is an automatic compile-cache (W1 Stage 3b.3):

1. Look for the canonical cache — `$BIRNAM_BOOT_CACHE`, else
   `<exe-dir>/../lib/birnam-boot.bim`.
2. If present and its v8 `boot_validity` matches (kernel eid-layout signature +
   the current module dylibs' content — Stage 3a), **replay it** and skip
   recompiling: load only the native prefix (`peg`, `lang`), replay the full
   image, `rematerializeClosures` + `resolveAllPending`. lang **skips its
   `installSourceMethod` birnam compiles** (the `boot_from_image` ShellContext
   flag — Stage 3b.2b); the prelude/stdlib bytecode comes from the image. Native
   modules not in the prefix (`net`) load *after* the replay and **relink by name**
   (`bind_native_method` + interned VTs — Stage 3b.1) onto the image-restored eids.
3. If the cache is absent or stale, recompile from source as before **and write a
   fresh cache** for next time. Editing/rebuilding any module dylib or the kernel
   flips `boot_validity` → the next boot recompiles + refreshes.

`--boot-image <path>` is the same machinery with an explicit path (and the same
validation/fallback). The earlier gaps are **closed**: a full image carries the
whole tower, so `json`'s `parse:` grammar (built at `json` init) round-trips by
construction, and `net`'s FFI relinks post-replay. The legacy M_USER `:save`
remains for lightweight user-data overlays on top of a booted system.

## Freeze — `freeze.zig`

`:freeze` compiles an image's FFI wraps into a standalone wrapper dylib:

1. `reconstructSource` walks every `:ffi-path` module and its wraps and emits
   canonical Birnam source — `let mN := [Module load: "path"];` followed by
   `[mN wrap: #sel as: "symbol" signature: "sig"];` per wrap.
2. `writeArtifacts` writes `<base>.birnam` (the reconstructed source), `<base>.zig`
   (a tiny wrapper module that `@embedFile`s the source and, on
   `birnam_module_init`, sends `fileInSource:` to the `language` prototype), and a
   copy of `module_abi.zig`.
3. `compile` invokes `zig build-lib … -dynamic -fPIC` to produce `lib<base>.dylib`.

Because the wrap is re-filed-in at load, the symbols rebind by name — the freeze
output is pure data + a thin re-file-in shim, requiring no `cc` at save time.

## Binary-format readers — `loader.zig`, `macho.zig`, `elf.zig`

Symbol enumeration is fact-dispatched, not hardcoded. `loader.registerReader`
self-registers each reader at boot with `[reader :reader-magic <u32>]` and
`[reader :reader-impl <fn-ptr>]`. `loader.openSymbols` sniffs the first four bytes
and queries `entitiesMatching(ATTR_READER_MAGIC, magic)` to dispatch — so adding a
format (PE, COFF) is a fact assertion, no kernel switch.

- **`macho.zig`** — Mach-O 64-bit (darwin, little-endian arm64/x86_64). Validates
  the magic, scans load commands for `LC_SYMTAB`, walks `Nlist64` entries
  resolving names via the string table, classifies `section`/`absolute`/
  `undefined`/`other`, and adapts to the shared `loader.SymbolIter`.
- **`elf.zig`** — ELF64 (linux, little-endian). Validates class/endianness, finds
  `SHT_DYNSYM` (the dlsym-callable view), walks `Sym` entries via the linked
  string table, classifies by `st_shndx` and binding.

Both expose `Reader.init`/`Reader.symbols` and an `openSymbolsAdapter` registered
with the loader. See [FFI](2d-ffi.md), which consumes these.

## AOT — `aot_main.zig`

The AOT driver turns an embedded `.bim` (`@embedFile`) into a standalone binary,
selected by build options:

- `--aot=static` — kernel + modules + image all linked into one executable. The
  peg/lang modules are `@import`ed and init is called namespace-qualified (no
  dlopen).
- `--aot=dynamic` — kernel + image linked; modules `dlopen`'d at startup from the
  manifest (`peekManifest` → `dylib_search.resolve` → dlopen → `birnam_module_init`
  via the ABI-mirror types).
- `--strip=malleable` — drop the frontend (no peg/lang static, AST stripped); a
  pure-VM binary that runs bytecode and only executes via `--entry`.
- `--strip=history` — `loadFrozen` + `store.freeze()`; read-only indexes, no
  time-travel.

`main` allocates the three kernel structs, enables the index + strings arenas,
boots, populates the IdApi, then branches (comptime) to the static-full,
static-malleable, or dynamic path. Each path loads the baked image, resolves
pending FFI, rematerializes closures, optionally freezes, and then runs
`--entry=<selector>[:<recv>]` (exit code = the result's low 8 bits) or drops into
the REPL. `lookupBinding` walks the scope chain to resolve a named receiver.

All modes share the same fact store, object model, bytecode format, and `.bim`
layout — AOT is "wrap a Mode-1 image as an executable," same bytes underneath
([vision](10-vision.md) §Execution Modes).

## Where to go next

- What the readers feed: [FFI](2d-ffi.md).
- Why the interner must persist: [Interner](24-interner.md).
- The retention horizon AOT sets to "now": [Time-Travel](15-time-travel.md).
