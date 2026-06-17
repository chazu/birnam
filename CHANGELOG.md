# Changelog

All notable changes to birnam are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions are
[semver](https://semver.org/). Binaries are arm64 macOS only for now — other
platforms build from source (`zig build install`, Zig 0.15.1).

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
