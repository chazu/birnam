# Changelog

All notable changes to birnam are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions are
[semver](https://semver.org/). Birnam currently supports arm64 macOS only;
other targets are rejected at build time until their libtcc and shared-library
loader ports are complete.

## [Unreleased]

### Added

- V2 Frames support suspension and one-shot capture/restore through ordinary
  Machine operations (Design 60P). Resume commits the pending Call edge, including
  parallel copies, cleared slots and precise successor roots. Frame layout V1
  and raw Frame slots are removed; handwritten probes now use genuine V2 layouts.
  Public IdApi/Method ABIs and canonical descriptor bytes are unchanged.
- Design 74O's test-only feasibility experiments reconstruct an absent callee
  using production compiler layouts, consume an Id-authored recovery recipe,
  and test identity, native observation, GC and continuation boundaries.
  Debug/release and full verification pass. V2 continuation storage works in
  the bounded probe; the publication gap found there is resolved by 60P.
  No general reconstruction admission or performance benefit is claimed.
- Design 74N repeats isolated native-tier testing after dependency revalidation.
  Eighteen paired cases and increased native coverage pass; seven controlled
  timing groups find no independent speedup beyond variation and measure about
  0.30–0.35 ms extra warmup cost in both chain sizes.
- Design 74M's focused performance runner isolates native-tier execution,
  compilation and memory costs from dispatch changes. Seven controlled pairs
  find no independent speedup beyond variation; separate diagnostics identify
  unused cold preparation and invalidation caused by unrelated symbol interning.
- Eligible portable Blocks can use runtime-owned native PC entries and recover
  through their original active Frame after invalidation (Design 74K).
  Ordinary staging and registration remain unchanged. Native code stays alive
  through GC, nested calls, active replacement and cache eviction.
  `mise run native-tier-test` checks recovery and guarded lookup in debug/release.
- `mise run deopt-api-experiment` checks Design 74I's guard substitutes through
  the frozen IdApi against ordinary floor builds. Native mutation and custom
  lookup counterexamples identify the guard-authority boundary for promotion.
- `mise run read-reuse-experiment` runs Design 74H's development-only guarded
  Frame-read reuse experiment. Debug/release cases include GC callbacks that
  mutate cached slots without dictionary changes. Seven controlled timing
  pairs were inconclusive, so the experiment is not integrated into normal
  compilation. Runtime ABI and image contracts remain unchanged.
- `mise run generated-deopt-experiment` runs Design 74G: compiler-generated
  per-PC entries recover to their original baseline through the real driver.
  The test checks colored Frame state, precise GC, invalidation and actual
  native-library retirement in debug and release. This experiment retains its
  test-only selector; Design 74K separately admits eligible portable programs
  in ordinary execution, with the existing ABI and image contract preserved.
- `mise run deopt-experiment` runs Design 74F's test-only same-frame recovery
  experiment in debug and release. The real Runtime driver switches from a
  static optimized entry to its original baseline on invalidation, preserving
  the active Frame, values, aliases and effects. The experiment adds no runtime
  exports and changes no image ABI or Frame layout.
- Design 74 E1 (development-only): a `dev-guarded-slots` cargo feature on the
  floor, TCC rung and emitter adds guarded direct Machine slot access for
  generated StepMethods, selected per compiling process by
  `BIRNAM_DEV_GUARDED_SLOTS=1`, with a resumable loop fixture and driver.
  These development shims remain absent from ordinary builds; the runtime ABI
  hash and export surfaces are untouched. Every vtable now carries a
  dictionary-mutation epoch.
- Development-only slot-index box elision on the guarded path (Design 74D,
  `BIRNAM_DEV_ELIDE_SLOT_BOX=1` on top of the E1 selection): the floor
  answers slot accesses from an unboxed index on a guard hit and performs
  the ordinary send itself, with the store value rooted, on a miss. Measured
  at about half the baseline loop time on the Design 74 fixture.

### Changed

- Ordinary sends guard canonical Machine slot access and default lookup inside
  the runtime (Designs 74J/74L). Current methods, inheritance, custom lookup and
  native/resumable overrides retain their effects. IdApi, exported functions,
  primitive semantics and Frame/image formats remain unchanged. Seven controlled
  groups qualify a 17.3% combined improvement on the smaller warm portable
  fixture, with no above-spread regression; other rows and separate PC/lookup
  gains are inconclusive. The native compiler adds about 603 KB to the floor.
- Release acceptance for v0.15 judges micro-layer comparator flags by their
  raw per-operation medians (Design 60HC): a flag is accepted when raw time is
  within the 10% micro allowance or moved by less than the case's own
  three-process spread, because v0.15 halved the subtracted empty-loop
  baseline and two clean candidates flagged disjoint micro cases with no raw
  regression. Every other layer keeps the ordinary comparator threshold;
  v0.16 returns to the ordinary comparison.

- CodeProgram objects retain checked immutable layouts, so Block construction
  and reflection reuse validated views. Global imports and method dictionaries
  use derived identity indexes while preserving ordered enumeration, current
  binding behavior and weak-key reclamation. Host-gated timing acceptance
  closed on 2026-09-09 (`docs/measurements/as7-acceptance-comparison.json`):
  Block construction and CodeObject reflection measurably cheaper, the global
  and method indexes timing-neutral at probe scale, docgen 39.6 to 29.4 s.
- Rust ordinary/portable C emission shares byte escaping, native providers and
  consumers share private ABI carriers, and data-only receipt validators share
  mechanical checks while retaining their schemas, diagnostics and policies.
- Rust host and Codegen share the existing SHA-256 implementation. Artifact
  formats share scalar byte helpers while retaining their checked/unchecked
  offset behavior, independent validation policies, and provider-free bootstrap.
- Store snapshots use transient ordered pair indexes instead of repeated
  whole-log scans. PEG indexes rule names and copies visited pattern data once
  per parse, preserving mutable grammars, memo identity and parse diagnostics.
- Query matching borrows candidate values and validates constraints before
  copying bindings. Rejected candidates allocate nothing; successful triples
  avoid temporary value copies while preserving query ordering and equality.
- Weak Symbol lookup now uses a collision-safe name index while preserving
  reclamation, enumeration order, and stable live-name pointers. Generated
  vtable caches are indexed by generation and slot, and heap allocation metadata
  no longer stores a redundant payload pointer. Design 66 records development
  throughput evidence and the remaining allocation investigation.
- Development benchmarks can opt into the existing host-load gates with
  `BENCH_CHECK_HOST=1`; release benchmarks continue to require those gates.
- Project builds publish self-describing application, configuration, and module
  roots instead of `artifacts.toml`. Ordinary `run` and `install` consume the
  last successful publication without reading authored sources or parsing the
  manifest; `--build` explicitly refreshes it. Tests load the published module
  root without executing or requiring a valid application root.
- Project build output reports reused modules. Missing or malformed cached
  artifacts rebuild safely, and failed builds preserve the previous publication.
- Ordinary project launch preflights its selected closure once. A nine-module
  Git/native fixture measured 1.228 s before and 0.826 s after this change
  (five warm launches, release runtime; diagnostic evidence in Design 65).

### Fixed

- Native-tier admission revalidates its actual Machine/lookup dependencies after
  dictionary mutation (Design 74N). Unrelated symbols, exports and ordinary method
  replacement retain valid native entries; real guard changes still resume the
  original Frame through its baseline. IdApi and language semantics are unchanged.
- Stdlib boot regained the ~10 ms that the PEG pattern-snapshot tables and
  per-vtable method indexes had added: parse-local PEG tables now hash
  integer keys with an address hasher, and method dictionaries of at most 16
  keys scan instead of maintaining a hash index. Host-gated against the
  pre-Design-68 baseline: dylib boot 107 to 86 ms, source boot 271 to 242 ms,
  `parse.json-20kb` 362 to 310 ms, docgen 39.6 to 29.4 s. The earlier
  dylib-startup RSS growth did not reproduce (65.5 MB on every run).

## [0.14.0] — 2026-08-27

### Added

- **Stackless native execution is now the production runtime path.** Birnam
  lowers resumable methods and blocks into explicit Machines, Frames, and
  StepMethods, preserving ordinary dispatch, inheritance, DNU, late binding,
  callbacks, and tail-call behavior without growing the native stack.
- **Portable CodeObjects preserve executable meaning across processes and
  images.** Programs, lexical bindings, statements, captures, and source maps
  are encoded canonically, restored through image v8, and rematerialized into
  disposable native code without making process-local addresses durable.
- **One-shot continuations and resumable lifecycle controls.** Host suspension,
  restoration, code-binding retention, unload/retirement fencing, bounded
  history compaction, and image recovery now share explicit ownership rules and
  production acceptance coverage.

### Changed

- **Release acceptance now proves the shipped archive itself.** The release
  gate installs the immutable candidate, exercises exact-current rejection,
  image and resumable lifecycles, long-running stack-flat control, bounded
  churn, performance comparison, and longevity, then publishes the receipt and
  complete evidence bundle beside the tarball.
- The image carrier advances to v8 to preserve boxed identity and foreign
  addresses while retaining portable executable meaning.

### Fixed

- Release promotion now replaces the consumed `## [Unreleased]` heading with a
  fresh one, so the next release does not fail merely because the previous
  release successfully promoted its notes.
- Restored image entry and baked-image boot behavior on the Rust host, including
  exact-current checks and stackless registration after recovery.
- The Rust TCC dylib audit covers the stackless export seams used by installed
  and relocated release bundles.

### Performance

- This first complete stackless release explicitly accepts substantial
  throughput regressions in exchange for bounded native-stack use, resumable
  execution, portable CodeObjects, image-v8 recovery, and reclaimable live-code
  lifecycles. Against the newest retained production Rust snapshot,
  `sends.unary` is 19.6% slower, `loops.to-do` is about 237x slower,
  `query.single-clause` is about 456x slower, and full documentation generation
  is about 30x slower. The complete decision, case boundary, and release-only
  ceilings are recorded in Design 60HB; later releases return to the ordinary
  performance policy.

## [0.13.0] — 2026-08-04

### Added

- **`mise run test` runs the birnam-level module suites** (`scripts/birnam-test.sh`
  → `modules/run-tests.birnam`) against a freshly built prefix. `mise run verify`
  gates the Rust workspace and the build driver but never loads a `.birnam` test
  file, so the birnam suites had no task of their own and had to be invoked by
  hand — easy to skip when checking a change that only touches birnam source.

### Changed

- **The statement separator `;` is now optional.** Whitespace alone separates
  statements. Every expression form is self-delimiting — a bracketed group
  (`[..]`, `#[..]`, `{..}`) or a single token — and nothing juxtaposes at
  statement level, so `;` carried no information the grammar did not already
  have. `[a foo]; [b bar]` and `[a foo] [b bar]` are one program, and the two
  spellings may be mixed. Existing source is unaffected; the new suite
  `modules/lang-test.birnam` pins the equivalence.

### Fixed

- **A binary operator can be defined at runtime.** The trampoline that backs
  `Reflect defineMethod:on:as:` outside an AOT artifact derived its arity by
  counting colons in the selector. A binary operator has none, so `#|>` and
  friends were compiled as unary and handed their two-parameter block a
  `value:`, answering `#doesNotUnderstand` instead of running. The module
  already had the correct rule in `selector_arity`; the trampoline just
  open-coded its own. Only the from-source path was affected, which is why
  stdlib's own `|>` worked while an override of it could not.
- **Validation compiles through libtcc are serialized.** libtcc carries
  process-global state across `tcc_new`/`tcc_relocate`, so the two
  `birnam-alien` tests that compile shims raced when cargo ran them in parallel
  threads and killed the test process with SIGSEGV — about one run in four,
  under `cargo test --workspace` and therefore under `mise run verify`. The
  lock lives in `compile_source_for_validation` so no caller has to remember.
- **An unbound selector now errors at every arity.** Dispatch forwards an
  unbound send to `doesNotUnderstand:` as `[selector, arg0, …]`, so the handler
  receives `1 + argc` arguments; the base handler guarded on exactly 1, which is
  true only of a unary send. Every keyword and binary send to an unbound
  selector therefore answered `nil` instead of a kinded `#doesNotUnderstand`
  error, so a renamed or mistyped selector read as a silent `nil` at the call
  site. `modules/stdlib-test.birnam` now pins the behaviour at unary, one- and
  four-keyword, and binary arity.
- **`modules/cli-test.birnam` calls the current CLI entry point.** It still sent
  `run:source:rustWorkspace:guides:`; the `source:` keyword named the Zig source
  directory and went away with the Zig tree. Four cases had been failing since
  then — masked as `got=nil` by the `doesNotUnderstand:` bug above.
- **Emacs `birnam-mode` project commands** (0.4.0): `birnam-run-project` and
  `birnam-test-project` now pass `--build`, so they no longer fail on a stale
  or absent artifact manifest; `birnam-run-project` also passes `--no-repl`,
  because a compilation buffer has no terminal to host a project whose
  manifest sets `run.repl = "tty"`.
- **Emacs lsp-mode startup is deferred.** `birnam-lsp-start` now goes through
  `lsp-deferred` when lsp-mode provides it, so a `.birnam` file opened
  invisibly by a grep or project search no longer spawns a language server
  from the major-mode hook. Project rooting moved from a `let'-bound
  `default-directory` — which could not survive deferral — to a buffer-local
  `lsp-auto-guess-root` resolved through the `project.el` integration
  `birnam-mode` already installs.
- `birnam-mode.el` byte-compiles without warnings: the lazily loaded lsp-mode
  functions are declared, and `lsp-language-id-configuration` is declared
  special without a value so this file never shadows lsp-mode's default.

### Added

- `mise run emacs-test` runs the `birnam-mode` ERT suite
  (`scripts/emacs-test.sh`), which previously had no task wiring.

## [0.12.0] — 2026-07-29

### Changed

- **Rust is now Birnam's only implementation and build authority.** The Zig
  runtime, compiler integration, build graph, probes, and compatibility
  selection paths were removed.
- The installer, release bundle, CI, project tools, wrapped-library docs, and
  stage1 modules now use the Rust host exclusively.
- The native source documentation module is now named `nativedoc`.

## [0.11.0] — 2026-07-15

### Added
- **Project-aware language tools.** The LSP now indexes authored source across
  resolver-selected dependencies for navigation, references, hover, scoped
  completion, and signature help, with UTF-16 and document lifecycle coverage.
- **Emacs lsp-mode integration.** `birnam-mode` now discovers `birnam.toml`,
  starts `birnam --lsp`, roots the REPL at the project, and exposes project
  check/build/run commands with integration tests.

## [0.10.0] — 2026-07-14

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
