# Changelog

All notable changes to birnam are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions are
[semver](https://semver.org/). Birnam currently supports arm64 macOS only;
other targets are rejected at build time until their libtcc and shared-library
loader ports are complete.

## [0.7.4] — 2026-06-22

_Notes for the next release land here; `scripts/release.sh` promotes this
heading to the new version (dated) and uses the section as the release notes._

### Added
- **Compiled-unit-first image integrity:** image format v3 checksums every
  embedded dylib and verifies all compiled units before load changes live
  state. Full saves publish atomically, and unsupported durable values reject
  the save with a structured `[System imageSaveReport]` rather than being
  silently omitted.
- **Explicit platform boundary:** arm64 macOS is the sole supported build and
  release target. Unsupported targets now fail before compilation instead of
  producing an incomplete or non-bootable artifact. See
  `docs/misc/PLATFORM-SUPPORT.md` for the porting requirements.
- **One verification gate:** `zig build verify` now runs the native test suite,
  self-hosted stage-one rebuild, birnam-level suites, and REPL goldens; it is
  enforced by GitHub Actions and before release tagging.
- **`--boot-image <path>` (experimental): boot from a saved image instead of
  recompiling the birnam modules from source.** Loads only the native prefix
  (`peg`, `lang` — the parser/compiler) and replays the image, reusing the
  proven `loadFromFile` + `rematerializeClosures` + `resolveAllPending` path;
  produce the image with a normal boot + `:save`. Boot-only peak drops ~656 MB →
  ~241 MB (2.7×), and pure-birnam modules round-trip from the image alone (e.g.
  `Stats sum:`). Opt-in — normal boot is unaffected. Known gap: json's `parse:`
  stays broken under `--boot-image` (json builds a stateful PEG grammar at its
  dylib init, which boot-image skips and which does not yet round-trip); `net`
  (a Zig FFI module) is also not re-inited yet.
- New docs: **the six fact/object indexes from first principles**
  (`docs/11a-the-six-indexes.md`), the **code-fact tiers** build plan
  (`docs/19a-code-fact-tiers-plan.md`), and the **transient-load RFC**
  (`docs/19b-transient-load-rfc.md`).

### Changed
- **README rewritten for the active system:** current installation, REPL,
  script, image, tool, verification, and support guidance now replaces the
  retired kernel and command documentation.
- **File-in now prunes, so loading a large program is memory-bounded.** Loading
  + compiling a non-trivial program (the `logue` roguelike: ~4.4 GB) accumulated
  every top-level form's run-and-discard CST/AST/bytecode tree to peak, because
  `fileIn:` never pruned and a `birnam script.birnam` run exits before the REPL
  loop's auto-prune is ever reached. Two levers, both reusing the trusted
  `prune.pruneReachable`/`compact` path: (1) a threshold-gated `maybe_prune`
  between top-level forms bounds **peak** during the load — in a faithful
  reproduction, peak goes from growing linearly with program size (3.2 / 6.5 /
  13.8 GB at 120k / 240k / 480k entities) to **flat ~800 MB**; (2) one forced
  compaction at the load boundary (`prune_now`) sweeps the residual tail and
  collapses the loaded program's per-fact history, dropping a worst-case
  steady-state from ~715 MB to ~3 MB. Both run only under current-only retention
  with no live snapshot, and keep all reachable state — only unreachable
  `M_USER` garbage is swept. New doc: `docs/19-load-memory.md`. (Two append-only
  `IdApi` hooks: `maybe_prune`, `prune_now`.)
- **Compaction tightens the index epoch (~12× smaller steady-state index).**
  `compact` rebuilds the five indexes via per-fact `compactInsert`, which
  path-copies all five HAMTs per fact; the abandoned spine nodes accumulate in
  the fresh epoch and only release at the *next* compaction (which makes its own
  equal pile), so the live epoch never shrinks below one rebuild's worth of
  garbage (measured ~90%: `logue` held a 168 MB index epoch for ~14 MB of live
  structure). `compact` now deep-clones the live roots into a fresh arena and
  frees the garbagey one (`Hamt.cloneInto` / `Rrb.cloneInto` + `tightenLiveEpoch`)
  — `logue`'s index epoch drops 168 MB → 14 MB (bare boot 74 → 6 MB). Best-effort:
  a failed tighten leaves a valid larger epoch, never failing the compaction.
- **Code-fact storage tiers — AST/bytecode facts are cheaper.** AST-node and
  bytecode metadata are write-once and never reverse-ref-walked, so they now
  drop their per-fact RRB history (a `:no-history` tier) and the never-read VAET
  reverse-ref index — dropping code facts from five indexes to four and
  collapsing their retained history. Reads are unchanged (reverse-refs run on the
  retained AVET). A per-attribute policy (`kernel_eids.isCodeAttr`, injected like
  the cardinality schema) drives it; deliberately excludes the shared
  `:vtable`(19) attribute. Plan: `docs/19a-code-fact-tiers-plan.md`.

### Fixed
- **Image `:save`/`:load` of user-defined objects no longer breaks on a fresh
  load.** `[Object new]` tagged the instance's and its per-object vtable's
  structural facts (`:vtable`, `:delegate`) with the lang module's eid — CODE
  provenance — but the image persists only `M_USER` (CODE is reconstructed by
  module init). So a fresh process that loaded an image got the object's method
  facts (which were already `M_USER`) but not its vtable/delegate chain, and
  every send to it returned nil. The object is runtime data, so its structural
  facts now ride `M_USER` and round-trip — verified by defining a user object,
  saving, and calling it in a fresh `:load` process.
- **Persistent maps now serialize into images (`VK_MAP`).** The image writer
  skipped any fact whose value (or RRB element) was an `ASSOC_VT` map, so a map
  literal embedded in a method's `:constant-pool` or an AST `:ast-value` was
  silently dropped — breaking methods that used it on a fresh load. Map entries
  now serialize recursively (key + value). On-disk format → v6, with v5 still
  accepted (the layouts are identical apart from the added value kind).
- **Doc: `rematerializeClosures` no longer claims captures aren't serialized.**
  The note was stale — capturing methods' `:method-captures-ptr` specs already
  round-trip via `VK_CAPTURES` (covered by an existing test).

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
