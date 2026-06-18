# Birnam Tasks

Living task tracker — parallel to [ROADMAP.md](ROADMAP.md). ROADMAP = phase narrative + reasoning; TASKS = concrete, checkable work items (gaps, fixes, follow-ups) discovered during implementation. An item graduates to a commit; closed items keep a one-line provenance.

Convention: `[ ]` open, `[~]` in progress, `[x]` done (with commit/date). Severity tags: **BLOCKER** / **gap** / **nice-to-have** / **doc**. Each item names the file:line when known.

> History: the FFI width-complete peek/poke + integer bitwise/shift + negative-literal + REPL-resolve + wrap-error gap batch shipped 2026-06-17 (commit `c8e028b`). Networking design decisions live in the `project_birnam_networking_ffi` memory.

---

## Tooling

- [x] **emacs mode** (2026-06-18) — `birnam-mode` in `tools/emacs/birnam-mode.el`: font-lock (keyword selectors, symbols `#x`, query vars `?x`, block args `:x`, `:=`/`|>`, `//` comments that don't swallow division, literals), bracket-depth indentation, comint REPL (`run-birnam`), eval-region/eval-buffer (file in a temp unit via `:in`) + eval-line. Byte-compiles clean; verified via the emacs MCP. Deeper `:`-command awareness in the REPL buffer still open.
- [ ] **LSP server** — `birnam --lsp`. **Host decided: thin zig JSON-RPC envelope in the same binary, semantics delegated to the in-process PEG/evaluator/fact store** (not a second parser, not an external file index). Enabler already in-image: PEG CST has byte-spans, AST nodes are span-tagged entities (`newAstNode(VT, start, end)`), `parse:from:` reflective on `GRAMMAR_VT`, errors are `ERROR_VT` values, selectors/methods are queryable entities. Migrate the envelope into `lsp.birnam` (full dogfood) once a JSON codec + `Content-Length` stdin reader land (after `peekInt32:`).
  - **Diagnostics decided: parse + static only** (syntax errors via PEG farthest-fail offset; AST scope-walk for unbound vars + selector arity). No execution, no store mutation — safe per-keystroke. Eval-based diagnostics (`ERROR_VT` over a scratch/snapshot fork) deferred to opt-in / on-save.
  - [x] **P0 SHIPPED (2026-06-18)** — `src/lsp.zig` + `--lsp` arg routing in `shell.zig`. stdio JSON-RPC 2.0 (`Content-Length` framing), `initialize`/`shutdown`/`exit`, `did{Open,Change,Close}` (full sync), per-URI doc store, byte→Position, `publishDiagnostics`. Parse via `parseExpr:` (LANG_VT) on `;`-split top-forms; the PEG farthest-fail offset turned out **already plumbed** (`ATTR_PEG_POSITION` on the parse-failure ERROR_VT) → zero kernel work. eglot auto-registered in `birnam-mode.el`. Verified end-to-end (multi-form, real example file clean, blank/comment-only forms skipped, multiple errors). **Open within P0:** UTF-16 positions (currently bytes), prune CST/AST facts per didChange (store grows), static unbound-var + arity checks, expose the `:peg-expected` symbol in the message.
  - **P1** `documentSymbol` (top-level `let` + method installs from AST), `completion` (installed selectors + keyword-arity from `:` count + in-scope `let` + `let/nil/true/false/self/super` floor).
  - **P2** `hover` (span→node→`BYTECODE_METHOD_VT` source/sig; entity attrs), `signatureHelp` (arity = colon count).
  - **P3** `definition`/`references` — within-doc trivial (AST `let`→span); cross-unit needs source-location provenance facts on install — defer, ties to 15.A-AST.
  - Editor wiring: `(add-to-list 'eglot-server-programs '(birnam-mode "birnam" "--lsp"))`.
