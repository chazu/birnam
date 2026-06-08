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
`MAGIC = "BRNIMG\x00\x01"` (8 bytes), `FORMAT_VERSION = 5`. The file holds, in order:

- **Manifest** — `bytecode_format_version`, `thread_mode` (`single`/`datomic`),
  and the module dependency set (`{name, abi_hash}` each). Embedded in the file
  (no sidecar) since v4.
- **Interner section** (v5) — the eid↔name bijection, so binding and method-name
  identity survive reload (the [interner](24-interner.md) rationale).
- **Counters** — `next_tx`, `next_eid`.
- **Facts** — iterated via `meatIter`, but **only `m == M_USER` facts are
  written.** CODE facts (bootstrap, module init) are *reconstructed* every boot by
  re-running boot + module init, so they ride unstable runtime module eids and
  must not be persisted; DATA facts ride the stable `M_USER` eid. See
  `kernel_eids.zig` `M_USER`.

Values are tagged by kind: `VK_IMMEDIATE` (self-describing Ref verbatim),
`VK_STRING` (len + bytes), `VK_FLOAT` (f64 bits, re-boxed), `VK_RRB` (count +
recursive elements), `VK_CAPTURES` (a `:method-captures-ptr` array).

### Save and load

- `write` / `saveToFile` serialize; `load` / `loadFromFile` **replay**: facts are
  re-`assert`ed into a fresh store, the interner is rebuilt with `restoreEntry`,
  and the counters are restored to their monotonic floor.
- `loadFrozen` uses the lean `frozenInsert` path (no tx churn / history) for the
  Phase-14 `--strip=history` mode.
- `rematerializeClosures` rebuilds a fn-ptr `Closure` for each `:bytecode-stream`
  entity after load (layout `[CLOSURE_VT][bytecodeDispatcherImpl][mkEid(method)]`),
  so stored methods are callable again.
- `peekManifest` parses just the manifest (no facts), which the dynamic-AOT loader
  uses to learn module deps before dlopen.

Load is replay-based today; the mmap-segment fast path is a deferred Phase-12.5
optimization the single-file layout already allows.

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
