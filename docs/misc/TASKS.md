# Birnam Tasks

Living task tracker — parallel to [ROADMAP.md](../archive/ROADMAP.md) (now archived/historical). ROADMAP = phase narrative + reasoning; TASKS = concrete, checkable work items (gaps, fixes, follow-ups) discovered during implementation. An item graduates to a commit; closed items keep a one-line provenance.

Convention: `[ ]` open, `[~]` in progress, `[x]` done (with commit/date). Severity tags: **BLOCKER** / **gap** / **nice-to-have** / **doc**. Each item names the file:line when known.

> History: the FFI width-complete peek/poke + integer bitwise/shift + negative-literal + REPL-resolve + wrap-error gap batch shipped 2026-06-17 (commit `c8e028b`). Networking design decisions live in the `project_birnam_networking_ffi` memory.

---

## Current Architecture

- [x] **Compiled-unit-first images** (2026-07-10) — image v3 treats embedded
  dylibs as the primary executable layer, captures module programs plus method
  definitions, verifies checksums before mutating live state, publishes full
  saves atomically, and rejects unsupported durable values with
  `[System imageSaveReport]`. See
  [`docs/design/45-compiled-units.md`](../design/45-compiled-units.md).
- [x] **First-class `CompiledUnit` lifecycle** (2026-07-13; `f21a2ae`,
  `f7f5827`) — reflected candidate generations now separate inspect, dependency
  preparation, native mapping, validation, activation, rollback, drift adoption,
  and image composition. Failed candidates leave dispatch unchanged; exact
  rollback and mapped/rebuilt image behavior are covered by the verification
  suite. See the [user guide](../guide/05-compiled-units.md) and
  [design 47](../design/47-compiled-unit-lifecycle.md).
- [x] **L1a generation authority and image coherence** (2026-07-14) — one
  runtime-wide `DispatchGeneration` now owns canonical source and dispatch
  authority; activation and rollback publish only at the host quiescence
  boundary; image v4 saves and restores the same generation on mapped and rebuilt
  boot; legacy `__modsrc__`, `__methods__`, `__methodops__`, and per-module active
  units no longer drive production recovery or inspection. `zig build verify`
  includes source/AOT G0 parity and generation-aware image coverage. See the
  [historical implementation plan](../design/50-five-phase-live-modules-and-isolates-implementation-plan.md#6-l1a--generation-authority-and-image-coherence).

---

## Next Architecture Sequence

The retired phase plan remains available as historical rationale in
[design 50](../design/50-five-phase-live-modules-and-isolates-implementation-plan.md).

- [x] **I1a — blocking single-worker isolate substrate** (2026-07-14) — adds a clean worker boot
  mode that suppresses ordinary `main`/REPL; implement bounded wire-v1 framing and
  the complete tagged copied-value codec; let the substrate own the receive loop;
  support exactly one deadline-bearing blocking request; distinguish application,
  protocol, timeout, transport, and exit failures; and own channel closure,
  bounded termination, orphan cleanup, and single-owner eventual reaping. Prepared
  candidates, Oops, native pointers, pools, promises, and replacement policy
  remain absent. `zig build verify` covers wire/value codec bounds, structured
  failures, non-durable handle rejection, real image boot, application errors,
  terminal timeout cleanup, clean stop, and two independent handles. See the
  [user guide](../guide/06-isolates.md) and
  [design 48](../design/48-concurrency-reactors-and-isolates.md#9-interleaved-implementation-sequence).
The file-backed live-module, worker-replacement, and worker-rollout policy slices
were intentionally withdrawn on 2026-07-17. Their design notes remain historical;
the current verification surface stops at compiled-unit lifecycle and the I1a
blocking isolate substrate.

The minimal reactor remains orthogonal and may be added when concurrent I/O is
the immediate product target. Standard initialization, image-native source
editing, migration, structured operations, richer supervision, append
persistence, pools, and multi-module or cross-isolate activation remain deferred
until product evidence selects the next failure model.

---

## Proposed Applications

- [ ] **Birnam Digla / Design 55** — Slices 1–3 now provide a local, read-only,
  capability-free object snapshot over `Reflect` and the Store, including
  effective methods, named-global lookup, deterministic text, and structured
  warnings. The snapshot round-trips through the framed JSON session protocol.
  Next: finish Slice 4 with `snapshot: spec` plus a file artifact, then add
  source-on-demand and deterministic HTML. The full sequence remains in
  [design 55](../design/55-birnam-digla.md).

- [x] **Digla terminal editor foundation / Design 58** (updated 2026-07-19) — the
  multi-file `modules/digla/` project now includes a headless editor model,
  UTF-8 byte-boundary cursor movement, history, local evaluation, terminal-cell
  wrapped transcript rendering, an optional Termbox `--ui` entrypoint, and the initial source/LSP
  PositionMap conversion plus terminal-cell DisplayLayout mapping, lexical
  syntax spans, parser-backed diagnostics, and a bounded completion menu merging
  scoped source candidates with live-image globals. Evaluation, inspection,
  globals, runtime completion, and image identity use one JSON-RPC-shaped session
  boundary; terminal commands, `--session-stdio`, and the loopback-only-by-default
  `--session-listen` adapter consume the same copied-value responses. A lazy
  `--ui --connect=remote` client now drives that TCP session through the unchanged
  terminal-independent editor model, correlates output notifications, and closes
  the active connection on editor exit. `:connect local|remote` performs an
  explicit handshake and switches adapters without replacing editor state or the
  immutable startup snapshot. A real loopback server/client smoke test
  covers evaluation, transcript insertion, copied values, and close. The
  Termbox surface now loads through a precompiled application wrapper rather
  than runtime Alien compilation, reducing measured full-project cold launch
  from about 3.5 s to 1.9-2.0 s. A second project-local wrapper accelerates
  `Reflect allMethodsOf:` with exported-name lookup and a basis-keyed, single-pass
  reverse-global index: a controlled full-image request fell from 1.587 s to
  17.7 ms with byte-identical rendered output, while reflection after live-image
  mutation completes in 12.9 ms. Shift-movement selection now replaces/deletes
  through source-byte ranges, and a visual-row viewport keeps the cursor
  visible in buffers taller than the terminal. F1 merges source and copied runtime
  hover, F2 reconstructs full keyword signatures, F3 lazily opens same-buffer,
  project, or dependency source definitions, F5 restores prior document state,
  and F4 explicitly refreshes cached diagnostics. Typing no longer
  invokes the parser per key; buffered terminal events are coalesced and the
  application wrapper accelerates source-frame projection and transcript wrapping.
  Evaluation-local `System` output is now captured by that application wrapper,
  inserted before the value in the terminal transcript, and delivered as ordered
  `birnam/output` notifications plus a buffered response fallback without leaking
  unframed bytes onto session stdout. Framed clients now receive each captured
  write immediately through an evaluation-local sink while the response retains
  its bounded aggregate fallback; a process timing regression proves output leads
  a long-running result by more than 100 ms. TCP sessions now also consume a
  correlated `birnam/interrupt` nonblockingly at the same canonical loop
  boundaries, reject stale targets, return a structured interrupted phase, and
  remain usable. The remote terminal now drives TCP evaluation incrementally,
  appends each output event before the result, stays responsive to transcript
  scrolling, and pipelines Ctrl-C/Escape interrupts through a terminal-independent
  operation seam. A timed Termbox wait polls at 16 ms only during active work and
  avoids idle redraws; stdio and the synchronous local adapter stay non-interruptible
  except for deadlines.
  Positive `timeoutMs` requests and the
  `evaluation-timeout-ms` application setting now provide the narrower safe slice:
  application-local cooperative deadlines on canonical Block and Integer loops,
  returning structured timeout errors for every transport.
  A real PTY consumed and rendered a 60-byte form in under 500 ms, and evaluated
  and visibly wrapped the full `allMethodsOf:` result within one second. Page Up
  and Page Down now navigate those wrapped transcript rows with clamped offsets;
  new evaluation output resumes tail following. Source now soft-wraps into visual
  rows without changing byte authority, including wide-glyph cursor boundaries,
  selection, and cursor-following viewports. Consecutive character events are
  coalesced into one persistent-buffer edit; a 2.2 KiB PTY paste plus redraw completed
  inside a 500 ms observation window. Resolver snapshots reuse the active project
  resolution, while source lookup parses only files containing the requested token;
  the measured lookup adds roughly 0.24-0.29 s beyond startup. The current enriched
  image starts in 2.79-2.84 s across five current launches, and three steady remote
  `allMethodsOf:` round trips complete in 25.3-25.9 ms including framing and a
  6,099-character result. Next: carry Design 55's structured inspection model into
  richer terminal object cards and navigation without coupling those snapshots to
  Termbox or weakening the machine-client contract.

---

## Tooling

- [x] **Birnam-owned project policy** (2026-07-16) — resolution, dependency-aware
  artifact invalidation, artifact-record shape, and publication authorization
  run through `src/id/project-policy.birnam`; Zig retains filesystem/source
  observation, hashing, process launch, serialization, and atomic rename. See
  [design 57](../design/57-project-policy-in-birnam.md). Next: apply the same
  boundary to install/run artifact selection and native-record validation.

- [x] **emacs mode** (2026-06-18) — `birnam-mode` in `tools/emacs/birnam-mode.el`: font-lock (keyword selectors, symbols `#x`, query vars `?x`, block args `:x`, `:=`/`|>`, `//` comments that don't swallow division, literals), bracket-depth indentation, comint REPL (`run-birnam`), eval-region/eval-buffer (file in a temp unit via `:in`) + eval-line. Byte-compiles clean; verified via the emacs MCP. Deeper `:`-command awareness in the REPL buffer still open.
- [x] **LSP server / Design 53** (2026-07-15) — `birnam --lsp` remains a thin stdio JSON-RPC envelope with semantics delegated to the in-process parser/compiler/reflection APIs. The current contract and staged roadmap live in [design 53](../design/53-project-aware-language-tools.md).
  - [x] **P0 workspace/coordinates** — shared project-resolver snapshots, UTF-16 positions, document versions and cleanup, stale-change rejection, save refresh, and multi-root/nested-project coverage.
  - [x] **P1 safe static diagnostics** — parse-only syntax and provable literal selector-arity diagnostics through `Lsp staticDiagnosticsOf:`; no evaluation, Store mutation, artifact writes, or invented unbound-name errors. Zig retains only parser/CST construction.
  - [x] **P2 first navigation slice** — parser/CST-backed document symbols, same-document lexical definitions/references, and open-buffer workspace symbols through `Lsp sourceSymbolsOf:`, `sourceOccurrencesOf:`, and `sourceBindingsOf:`. Landed in `c9f4cad` and `979a54f`.
  - [ ] **Next P2 slice** — dependency-wide authored-source indexing, richer method/receiver hover, signature help, scoped completion, and references across unopened project modules. Do not use generated artifacts or image execution as source locations.
  - Editor wiring: `(add-to-list 'eglot-server-programs '(birnam-mode "birnam" "--lsp"))`.

## Language surface

- [x] **Birnam-authored parsers over PEG** (2026-06-18) — `peg` singleton bound as a global with birnam-friendly combinators: `newGrammar`, `literal:`/`ruleRef:`/`anyByte`/`eof`, `star:`/`plus:`/`opt:`/`not:`/`and:`, and `seqOf:`/`choiceOf:` (vector forms — keyword sends are fixed-arity, so the variadic `seq:`/`choice:` aren't birnam-callable) + `anyOf:` (charset from a string) / `range:to:`. Parse trees are tagged `CST_NODE_VT` (eid 254) and answer `ruleName` (string), `children` (vector), `firstChild`/`nextSibling`, `start`/`end` (byte offsets → slice source with `copyFrom:to:`). Example: `examples/peg-csv.birnam`. **GOTCHA fixed:** `parseExprImpl` treated "CST has any vtable" as an error — now `== ERROR_VT` (CST nodes carry a vtable). **Follow-up:** port the zig `Json`/`Yaml` to birnam units now that this exists.
- [x] **Symbol value protocol** (2026-06-18) — symbols now answer `=` (identity), `printString` (`#foo`), `displayString`/`asString` (`foo`), `asSymbol` (self). `SYMBOL_VT` (eid 255) delegating OBJECT_VT; `send.vtableEidOf` falls back to it for an interned NAMED eid with no explicit `:vtable` (no per-symbol facts). Verified; datalog symbol-keys + map keys unaffected (identity unchanged).

## Stdlib modules

- [x] **Test / Stats / CSV / Logging — birnam-authored** (2026-06-18) — the M6/M5/M3/M7 milestones, all written in birnam under `src/modules/stdlib/`, filed in by one `libbirnam_stdlib.dylib` shim, added to the default load (`peg lang json stdlib`). **Test** (M6): `describe:do:`/`it:do:`/`summary` (catches errors via `on: #Error do:` returning `[e message]` — a bare `let := error` halts). **Stats** (M5): `sum:`/`mean:`/`median:`/`min:`/`max:`/`mode:`/`variance:`/`stddev:`. **Csv** (M3): `parse:`/`rows:` (header→symbol-key maps via `zip:`)/`readFile:`/`write:to:` (limits: no quoted-comma fields, write uses map-key order). **Log** (M7): `info:`/`warn:`/`error:`/`debug:`/`level:` → stderr, `[Time millis]`-stamped. Tests: `src/modules/stdlib/stdlib-test.birnam` (18/18 green). Each module carries Test-based tests — the dogfood goal.
- [ ] **nice-to-have: dedup birnam-module shim** — `src/modules/json/json.zig` is generic boilerplate (`@embedFile` a `.birnam` + `[language fileInSource:]` at init). When a 2nd birnam-authored module dylib appears, replace per-module shims with one templated `unit_module.zig` selected by a build flag (e.g. `-Dunit=json`). Not urgent — only json today. (Longer arc: birnam emits its own module dylibs via `macho.zig`, no shim — see bootstrap notes.)

- [x] **OS / Process** (2026-06-18) — `System getEnv:` (→ string|nil), `setEnv:to:`, `exit:` (status code), `run:` (shell via libc `system()` → exit code). Zig methods on `SYSTEM_PROTO_VT`; libc `setenv`/`system` declared extern (absent from zig 0.15 `std.c`). Verified. **Open:** `spawn:args:` + stdout capture (use `std.process.Child`).
- [x] **Time** (2026-06-18) — `Time` prototype: `now` (unix seconds), `millis` (epoch ms), `sleep:` (ms). New eids TIME_PROTO/_VT (246/247). Verified.
- [x] **Math + Random** (2026-06-18) — numbers gained `abs`/`min:`/`max:`/`pow:`. `Random` prototype: `seed:`, `next` (float 0..1), `nextInt:` (0..n-1), `between:and:` (global Xoshiro, eids 248/249). Verified.
- [x] **String-utils (M4)** (2026-06-18) — `repeat:`, `padLeft:`/`padRight:`, `center:`, `capitalize`, `titleCase` on STRING_VT. Verified. (`slugify` deferred — needs non-alnum-run collapse.)
- [x] **JSON module — birnam-authored** (2026-06-18) — **ported from zig to birnam.** `src/modules/json/json.birnam` defines `Json` (`parse:` + `stringify:`) entirely in birnam over the PEG engine + CST surface; `src/modules/json/json.zig` is a thin dylib shell that `@embedFile`s it and `[language fileInSource:]`s it at init. Added to the **default load order** (`peg lang json`) as `libbirnam_json.dylib`. Enabling primitives added to lang: `Object new` (fresh prototype + private vtable) + general `vtable` accessor; type predicates `isString`/`isNumber`/`isVector`/`isMap`; string `asFloat`. The original zig JSON (buildJsonGrammar/jsonParseImpl/stringify) is retained but unwired (`buildJsonGrammar()` no longer called) — delete in a follow-up. Verified: default-load parse (object/array/float/bool/null) + stringify round-trip.
- [x] **YAML module** (2026-06-18) — `Yaml parse:` (flat subset). PEG grammar (lang.zig, `y-`-prefixed rules) → birnam data: block mapping (`key: value` lines) + block sequence (`- item` lines) + scalars (quoted/number/bareword, with `true`/`false`→bool, `null`/`~`/empty→nil) + inline flow `[a, b]`/`{k: v}`. One level of block structure only (indentation-sensitivity is not PEG-expressible) — documented limit. New eids YAML_PROTO/_VT (252/253). Verified: mapping, flow seq+map, nested, block sequence. **Open:** nested block structure, `Yaml stringify:`.
  - Both follow the same shape as `lang.zig`'s grammar (build with `peg_seq`/`peg_choice`/`peg_star`/`peg_charset`/`peg_literal`/`peg_rule_ref`) and return native birnam data, so a parsed result drops straight into `at:`/`do:`/`collect:`. Decide host: a wrapped/first-party birnam unit vs a zig `peg`-module like `query`.

## Networking

- [x] **Net — TCP over std.net** (2026-06-18) — `src/modules/net/net.zig`, `libbirnam_net.dylib`, default-loaded. `Net connect: host port: n` → socket; `Net listen: port` → server; socket `send:`/`recv:`/`close`; server `accept` (blocking). Built on `std.net`/`std.posix` (no FFI — sidesteps the `peekInt32:` gap). Sockets/servers are runtime-only entities holding the OS fd. Added `IdApi.bind_global` (append-only) so a zig module can expose a global (`Net`) without filing in birnam. Verified both ways (birnam client ↔ python server, birnam server ↔ python client) + `examples/net-echo.birnam`. **Open:** non-blocking/async I/O, TLS, UDP, `recv:` returns at most one read (no read-until).

## Config / shell

- [x] **linenoise REPL: shell-owned loop** (2026-06-19) — flipped the design so the SHELL owns the read-eval-print loop and linenoise is just the line reader (`[linenoise prompt:]` called by the shell loop). Both the built-in stdin loop and the linenoise loop now run one shared `processReplLine` (dispatch + per-line maintenance) at the quiescent boundary — so `:prune`/`:gc` and auto-prune/gc work in the linenoise REPL too (previously: birnam-owned `whileTrue` had no safe sweep point, so `:prune` hung the loop). `runBirnamRepl` files in only the wrapper (`linenoise.birnam`) + resolves the `linenoise` global via `resolveGlobal`; `repl.birnam` (the old birnam driver) deleted. The `g_repl_bridge_active` guard remains as belt-and-suspenders for `[language command:]`. Built-in loop verified (prune mid-session, stdlib survives); linenoise startup verified (real-TTY-only for interactive input).
- [x] **Modules in a known place / `Module load:` search** (2026-06-18) — Go-like: all module dylibs resolve through one search path. First-party modules already did (`<exe>/../lib` etc.). Now `Module load:` of a BARE name (no `/`) resolves the same way via a shell-registered `ffi_module.dylib_resolver` (→ `dylib_search.resolveWith`); a `/`-path stays literal. linenoise loads by bare name (`liblinenoise.dylib`); repo finds it via `dylib_paths = ["wrapped/linenoise"]` (cfg-relative, cwd-independent); install/dist stage wrapped dylibs into `lib/`. Reverts the v0.7.1 chdir hack. Verified REPL from repo root + subdir, installed + dev. **Open:** the linenoise REPL *driver* source (`repl.birnam`/`linenoise.birnam`) still isn't shipped in the tarball — embed it (like json/stdlib units) so the linenoise REPL works in a bare bundle, not just in the repo.

- [x] **Generalize module overrides** (2026-06-18) — `[module_overrides]` table in `birnam.toml` (slot→impl), top-level `repl = "…"` kept as back-compat sugar into the "repl" slot. Also `BIRNAM_MODULE_OVERRIDES=repl=linenoise,json=jsmn` env var + `--overrides <same-list>` flag, parsed by `config.parseOverrideList`. Shell merges toml→env→argv; `config.overrideFor` is last-wins → precedence argv > env > toml > default. `config.Override{slot,impl}`. Verified all four precedence cases via a pty (toml→linenoise handoff; flag/env override toml; flag beats env). **Open:** only the `repl` slot is *consumed* (the `runBirnamRepl` handoff); other slots (e.g. `json`) are parsed + stored but no module declares an override point yet — wire that when the JSON/YAML modules land, so e.g. `json=jsmn` selects an impl. See [[project_birnam_ergonomics]].
