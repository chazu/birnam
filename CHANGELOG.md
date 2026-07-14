# Changelog

All notable changes to birnam are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions are
[semver](https://semver.org/). Birnam currently supports arm64 macOS only;
other targets are rejected at build time until their libtcc and shared-library
loader ports are complete.

## [Unreleased]

### Added
- **Blocking process isolates and live-worker rollout.** Trusted workers now
  support copied-value request boundaries, deadlines, structured failures,
  health-checked single-worker replacement, and explicit cleanup ownership.
  Mailbox-safe live evolution carries checksum-bound activation and rollback
  controls with independent evolution receipts.
- **File-backed live module baselines.** Live module revisions can be exported,
  inspected, staged inertly, activated at the host boundary, rolled back while
  locally valid, and saved as guarded baselines that behave consistently under
  mapped boot and source rebuild.
- **Installed documentation and developer tools.** Installs and release bundles
  now carry the birnam module sources, Zig-floor documentation sources, and the
  authored guide under `share/birnam`; `doc`, `docgen`, `--lsp`, and `--listen`
  work from an installed tree without a source checkout. The complete
  verification gate includes an isolated-install smoke test.
- **User guide + self-hosted Markdown.** The generated HTML reference now opens
  with chapters covering the language, idioms, fact store, and images, rendered
  by a deliberately small Markdown module written in birnam.
- **Project-root configuration.** `birnam.toml` is discovered from cwd upward;
  `main` resolves relative to the discovered root, and `load = ["module", …]`
  loads compiled modules dependency-first before the entry point and REPL.

## [0.9.0] — 2026-07-12

The measurement release: the benchmark infrastructure landed, and what it
measured got fixed. The relational read path went from seconds-per-query to
milliseconds (the index arc), value equality became one story everywhere the
system compares values, and the first stdlib migrations gated by measurement
shipped.

### Added
- **Benchmark infrastructure (docs/46).** `[Time monotonicMicros]` and a
  calibrated, GC-disciplined harness (`modules/bench.birnam`); the micro
  suite (28 migration-gate cases plus a process-isolated parse suite); the
  relational suite over a fixed 10k-entity base that **gates on expected
  cardinalities** — it refuses to pass on wrong answers; macro whole-process
  cases (boot paths, image save/boot, stage1 compile, scripts, docgen, GC)
  via `scripts/bench.sh`; `bench-compare` with regression thresholds; mise
  tasks and `docs/misc/BENCHMARKS.md`. Tracked snapshots live in
  `bench/results/v<VERSION>.json`.

### Changed
- **Collection tier-B boundary, settled by measurement.** `includes:` and
  `asVector` moved to stdlib birnam (`includes:` is a one-send delegation to
  the retained native `indexOf:` — the fully-derived form measured 116×);
  `reversed` and `copyFrom:to:` stay native (2.58× / 1.60× — per-row loops
  fail the gate, as docs/44 §7 predicted). The whole boundary surface is
  pinned in stdlib-test either way.

### Fixed
- **Floats compare by value everywhere the system compares values.** Map
  keys, `includes:`/`indexOf:`, store facts (set semantics + retract by an
  equal-valued box), and datalog bindings — `[#[1.5 2.5] includes: 1.5]` is
  true now. Per-type like `=` (`1.0 = 1` stays false); `-0.0` and `0.0` are
  one Map key.
- **Datalog pattern constants match by value.** `#[?e #age 33]` matches an
  asserted 33 — previously number/float/string constants in triple patterns
  compared by pointer identity and silently matched nothing. Join agreement,
  rule args, and derived-tuple dedup ride the same value equality, so the
  engine now compares values one way everywhere.

### Performance
- **The index arc — the relational read path is ~quadratic no more.** The
  store gained an (e,a) postings index (an access path only: the append-only
  log stays canonical and is what serializes; the index rebuilds from it),
  the engine partitions gathered triples by attribute lazily per query,
  derived-tuple membership is hashed, and the join copies bindings only on
  match. Measured over the ~23k-fact bench base: single-pattern query
  **3.6 s → 3 ms (~1200×)**, nested pull **10.9 s → 8 ms (~1400×)**,
  recursive-rule query **4.3 s → 39 ms (~110×)**, 3-way join
  **4.7 s → 80 ms (~59×)**, asserts **102 µs → 2.7 µs/op (~44×)**. Point
  reads speed up system-wide — compiled global references ride the same
  store.

## [0.8.0] — 2026-07-11

The refounding. The legacy fact-store kernel is retired and birnam is rebuilt
from a pure Piumarta object floor (`src/id/`) up: the whole grammar compiles
to native code through the in-process TinyCC backend, the object model is
prototype-based, the relational reading is a datalog engine over the store,
and a growing share of the system — stdlib, config, CLI, REPL, module
manager — is written in birnam itself. Effectively a new system; the notes
below describe the new world rather than diff the old one.

### Added
- **The id floor and the native pipeline.** siphon → libid → store → peg →
  lang → tcc → codegen: every birnam form compiles to native code via the
  vendored TinyCC; blocks compile to native closures. FFI runs both
  directions — struct-by-value out, C-calls-birnam callbacks back in.
- **Prototype object model.** Every well-known name is a prototype owning its
  family vtable; creation is one verb `clone` (`new` aliases it) with
  copy-on-write vtables; no immediates — Integer is a boxed full i64, joined
  by Float and a raw 64-bit `Word` (with peek/poke/alloc/free for C memory).
  Native `whileTrue:`/`whileFalse:` on a real Block family, short-circuit
  `and:`/`or:`, structural equality across collections, `timesRepeat:`.
- **Result-style errors.** Errors are kinded propagating values (`kind`,
  `isError`, `message`) with `on:do:` (multi-kind) and `ensure:`; parse
  failures, FFI failures, and grammar left recursion are kinded errors
  instead of crashes or silent nil.
- **Collections + literals.** HAMT-backed Map, persistent Vector, Range, and
  String share one sequence protocol over a single native fold; `#[…]` vector
  and `{…}` map literals; `StringBuilder`; the `|>` pipeline operator.
- **The relational reading.** A datalog engine over the store:
  `[db findAll: ?v where: …]` with multi-variable joins, predicate filters,
  negation, disjunction, recursive rules (semi-naive, mutual recursion;
  durable as relational data), aggregates + group-by, time travel (`asOf:`,
  `since:`/`between:`), and pull. Writes: `[db entity]`,
  `[db assert: e attr: #a value: v]`, retraction.
- **Garbage collection.** A self-contained mark-sweep collector with
  automatic threshold collection, per-vtable trace hooks, and an exact-heap
  precise mode.
- **Images.** Save and boot the live system: relocatable AOT compiled units,
  full heap-graph serialization, incremental append + compact, and a
  self-booting executable bake. Compiled-unit-first integrity (format v3):
  every embedded dylib is checksummed and verified before load changes live
  state; full saves publish atomically; unsupported durable values reject the
  save with a structured `[System imageSaveReport]`. Doc'd stdlib methods,
  module singletons, Alien-wrapped C libraries, and runtime-loaded modules
  all survive save/boot.
- **Module system.** birnam-source modules compile to installable dylibs
  (`birnam --compile-module`, `zig build stage1`), with eval-from-source as
  the permanent floor; the module manager is itself a birnam module —
  dependencies are store facts, load order is a topological sort, cycles
  answer `#circularDependency`. Runtime loading via `[Module load: #name]`
  (deps first, idempotent). `birnam.toml` is parsed by the birnam-native
  `Toml`; `birnam init` scaffolds a project.
- **The self-hosted layer.** stdlib, toml, config, cli, and repl are birnam
  modules: the string protocol, the error protocol, the CLI subcommands, the
  console REPL session loop (`[Repl console]`), and the module manager are
  birnam code. The Zig floor keeps the per-byte scanners — the measured send
  cost (~1.5–2 µs) gates what migrates.
- **Alien — C libraries without new Zig.** `[Alien dlopen:]` +
  `[Alien wrap:signature:as:on:]` wrap any C library at runtime;
  `birnam wrap <name>` scaffolds a wrapper module; termbox2 is wrapped (the
  `logue` roguelike renders through it). Opt-in release paths for callbacks
  and dlopen handles.
- **OS + app modules, written in birnam over libc.** Time, File, Process,
  Log, Random, Socket (TCP), Http (a static HTTP/1.1 server), Json, Html,
  and Test (a live, Smalltalk-style test system).
- **Docs + tooling.** Docstrings ride definitions (`defineMethod:…doc:`) and
  land in the store; `birnam doc <module> [selector]` and `birnam docgen`
  (HTML docs covering birnam modules and the Zig floor); an LSP written in
  birnam (`birnam --lsp` — diagnostics, hover with docstrings); an
  out-of-process socket REPL (`birnam --listen`); Emacs `birnam-mode`
  refreshed for the current language.
- **One verification gate:** `zig build verify` runs the native test suite,
  the self-hosted stage-one rebuild, the birnam-level suites, and the REPL
  goldens; it is enforced by GitHub Actions and before release tagging.
- **Explicit platform boundary:** arm64 macOS is the sole supported build and
  release target. Unsupported targets now fail before compilation instead of
  producing an incomplete or non-bootable artifact. See
  `docs/misc/PLATFORM-SUPPORT.md` for the porting requirements.

### Changed
- **README rewritten for the active system:** current installation, REPL,
  script, image, tool, verification, and support guidance now replaces the
  retired kernel and command documentation.

### Removed
- **The legacy fact-store kernel** (retired to `src/attic/`) and, with it, its
  unreleased post-0.7.4 improvements (`--boot-image`, code-fact storage
  tiers, file-in pruning) — the id system supersedes them.
- The legacy configuration surface: the `$BIRNAM_PATH`/`$BIRNAM_DYLIB_PATH`
  search tiers, `[[wrapped]]` cc-built wrappers, and `[module_overrides]`.
  The live environment variables are `BIRNAM_LIB_DIR`, `BIRNAM_ID_DYLIB`,
  `BIRNAM_NO_STDLIB`, and `BIRNAM_GC_PRECISE`.
- The `--demo` flag.

## [0.7.4] — 2026-06-22

Release-process only: cutting a release now requires a CHANGELOG entry for
the version, and that section becomes the GitHub release notes.

## [0.7.3] — 2026-06-19

### Fixed
- **`:prune`/`:gc` no longer kill the linenoise REPL.** The linenoise REPL was
  birnam-owned (a `whileTrue` loop in `repl.birnam`), so its loop frame never
  unwound — there was no quiescent point to sweep, and a `:prune` mid-loop
  reclaimed the running loop's own blocks and hung the REPL.

### Changed
- **Shell owns the REPL loop; linenoise is just the line reader.** The shell
  now drives the loop and calls `[linenoise prompt:]` for input. Both the
  built-in and linenoise loops run one shared line-processor, so `:prune`/`:gc`
  **and** auto-prune/auto-gc run at the quiescent boundary in the linenoise
  REPL exactly as in the built-in one — and a long session stays memory-bounded.
  The old `repl.birnam` driver is removed. (Belt-and-suspenders: `:prune`/`:gc`
  invoked via `[language command:]` mid-loop refuse rather than crash.)

## [0.7.2] — 2026-06-18

### Changed
- **`Module load:` resolves bare names through the module search path** — the
  "known place" (`<exe>/../lib`, `$BIRNAM_DYLIB_PATH`, config `dylib_paths`, …),
  like first-party modules. A path with `/` is still used verbatim. So wrapped
  C libs load by bare name regardless of cwd; `install`/`dist` stage wrapped
  dylibs into `lib/`. Supersedes the 0.7.1 chdir workaround.
- Our own `birnam.toml` uses the `[module_overrides]` table for `repl`.

## [0.7.1] — 2026-06-18

### Fixed
- linenoise REPL failed to start from a subdirectory (wrapper's relative dylib
  path didn't resolve). (Superseded by 0.7.2's search-path resolution.)

## [0.7.0] — 2026-06-18

### Added
- **Networking — `Net`** (TCP over zig's `std.net`/`std.posix`, no FFI):
  `Net connect:port:`, `Net listen:`, socket `send:`/`recv:`/`close`, server
  `accept`. Default-loaded.
- **stdlib modules, birnam-authored** (default-loaded via one `stdlib` dylib):
  **Test** (`describe:do:`/`it:do:`/`summary` — xUnit-lite), **Stats**
  (`mean:`/`median:`/`stddev:`/…), **Csv** (`parse:`/`rows:`/`readFile:`/
  `write:to:`), **Log** (`info:`/`warn:`/`error:`/`level:`). Each carries its
  own Test-based tests (`stdlib-test.birnam`, 18/18).
- `IdApi.bind_global` so a zig module can expose a global without filing in
  birnam.

## [0.6.0] — 2026-06-18

### Added
- **stdlib surface (native):** Math `abs`/`min:`/`max:`/`pow:`; a `Random`
  prototype (`seed:`/`next`/`nextInt:`/`between:and:`); a `Time` prototype
  (`now`/`millis`/`sleep:`); OS on `System` (`getEnv:`/`setEnv:to:`/`exit:`/
  `run:`); String-utils (`repeat:`/`padLeft:`/`padRight:`/`center:`/
  `capitalize`/`titleCase`); string `asFloat`.
- **JSON + YAML parsers (PEG-backed).** `Json parse:`/`stringify:` and
  `Yaml parse:` (flat subset). JSON is **authored in birnam** (`json.birnam`
  embedded in a thin dylib) and default-loaded.
- **Birnam-authored parsers over PEG:** `peg` bound as a global with
  combinators (`seqOf:`/`choiceOf:`/`anyOf:`/`range:to:`/…); parse trees are
  navigable from birnam (`ruleName`/`children`/`start`/`end`). Plus
  `Object new` (prototype creation), a general `vtable` accessor, and type
  predicates (`isString`/`isNumber`/`isVector`/`isMap`).
- **Symbol value protocol** — symbols answer `=`/`printString`/`asString`/
  `asSymbol`.
- **Generalized module overrides** — `[module_overrides]` toml table +
  `BIRNAM_MODULE_OVERRIDES` env + `--overrides` flag (precedence argv > env >
  toml > default). `repl = "…"` is sugar for the `repl` slot.
- `birnam --help` / `--version`.

## [0.5.0] — 2026-06-18

### Added
- **Datalog write path:** `[db entity]` mints an entity; `[db assert: e attr:
  #a value: v]` asserts a fact — the birnam-level writes the query/pull surface
  was missing.
- Examples: `address-book` (datalog facts/query/pull), `fizzbuzz`, `primes`,
  `bank-ledger`.

### Changed
- linenoise REPL driver rewritten in a flat, named-block style.

## [0.4.0] — 2026-06-18

### Added
- **LSP P2:** `hover` (token/selector/arity) + `signatureHelp` (keyword-send
  parameters), lexical, no re-parse.

## [0.3.0] — 2026-06-18

### Added
- **Emacs `birnam-mode`** (`tools/emacs/`): font-lock, indentation, comint
  REPL, and lsp-mode + eglot client registration.
- **`birnam --lsp`** — a Language Server over stdio. **P0:** parse diagnostics.
  **P1:** `documentSymbol` (top-level `let`s) + prefix-filtered `completion`
  (keyword floor + interned selectors).
- **Wrapped first-party C libs** — `[[wrapped]]` in `birnam.toml` + `birnam
  wrap` (builds via `cc`, no zig); linenoise wrapped. An opt-in
  birnam-authored linenoise REPL (`repl = "linenoise"`).
- Width-complete FFI peek/poke, integer bitwise/shift, negative literals.
- Example: word-frequency counter.

## [0.2.0] — 2026-06-17

Onboarding ergonomics: get from "I have the binary" to exploring without a
manual.

### Added
- **Startup banner** — `birnam <version> — type :help for commands. Evaluate
  with brackets: [3 + 4]`. The empty prompt now teaches itself. Version is
  embedded from `VERSION` at build time.
- **`:help`** (`:h`, `?`) — REPL command reference covering both the
  shell-handled commands (`:in`/`:save`/`:load`/`:prune`/`:gc`/…) and the
  lang-handled ones (`:reset`/`:eval-mode`).
- **Script + eval run path** — any `*.birnam` argument is filed in, `-e EXPR`
  evaluates one expression; both run after the modules load and then exit.
  `-i` keeps the REPL open afterward, so a script's definitions carry into the
  live session ("load defs, then explore").
- **`birnam init`** — scaffold a project: writes `birnam.toml` and a starter
  `playground.birnam` in the current directory, never clobbering existing
  files.

### Notes
- Files separate statements with `;` (the REPL is line-based; files are not).
  The scaffolded `playground.birnam` follows this.

## [0.1.0] — 2026-06-17

First release. Made the shell pleasant to invoke and the install zero-config,
and added the tooling to cut binaries.

### Added
- **Bare-name module loading** — `birnam peg lang` instead of full dylib
  paths; names resolve along a search path (`lang` → `liblang.dylib` →
  `libbirnam_lang.dylib`). With no arguments, the default set (`peg lang`)
  loads.
- **`birnam.toml`** project config — discovered cwd-upward (git-style). Pins
  the module set (`modules`), extra search dirs (`dylib_paths`), and a snapshot
  to auto-load (`image`). A tiny TOML subset.
- **`$BIRNAM_PATH`** — Go-style `:`-separated install roots, each probed at
  `<root>/lib` then `<root>` (complements `$BIRNAM_DYLIB_PATH`, which lists
  exact dylib dirs).
- **Zero-config install** — the shell probes `<exe>/../lib`, so a
  `<prefix>/bin/birnam` + `<prefix>/lib/` layout resolves its modules with no
  environment variable set.
- **Release tooling** — `mise.toml` (pins Zig 0.15.1; `build`/`install`/`dist`/
  `release` tasks), `scripts/dist.sh` (build → stage → ad-hoc sign → tar a
  portable bundle), `scripts/release.sh patch|minor|major` (bump → tag → build
  → publish a GitHub release).
- **`--bare`** flag — load no modules (bare kernel).
- README "Get started in 60 seconds".

### Changed
- Install footprint is now exactly `bin/birnam` + `lib/libbirnam_*.dylib`;
  `libffi.a` (a build-time static archive) is no longer installed.

### Fixed
- The shell exits cleanly instead of dumping thousands of bogus
  GeneralPurposeAllocator "leaks" — its kernel/interner/module state is
  process-lifetime by design.

[0.2.0]: https://github.com/chazu/birnam/releases/tag/v0.2.0
[0.1.0]: https://github.com/chazu/birnam/releases/tag/v0.1.0
