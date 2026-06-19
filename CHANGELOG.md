# Changelog

All notable changes to birnam are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions are
[semver](https://semver.org/). Binaries are arm64 macOS only for now — other
platforms build from source (`zig build install`, Zig 0.15.1).

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
