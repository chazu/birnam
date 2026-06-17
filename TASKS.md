# Birnam Tasks

Living task tracker — parallel to [ROADMAP.md](ROADMAP.md). ROADMAP = phase narrative + reasoning; TASKS = concrete, checkable work items (gaps, fixes, follow-ups) discovered during implementation. An item graduates to a commit; closed items keep a one-line provenance.

Convention: `[ ]` open, `[~]` in progress, `[x]` done (with commit/date). Severity tags: **BLOCKER** / **gap** / **nice-to-have** / **doc**. Each item names the file:line when known.

> History: the FFI width-complete peek/poke + integer bitwise/shift + negative-literal + REPL-resolve + wrap-error gap batch shipped 2026-06-17 (commit `c8e028b`). Networking design decisions live in the `project_birnam_networking_ffi` memory.

---

## Tooling

- [ ] **emacs mode** — `birnam-mode` major mode for `.birnam`: syntax highlighting (bracket forms, keyword selectors, symbols `#x`, query vars `?x`, literals), indentation, comint REPL (`run-birnam` over the shell stdin loop), eval-region/eval-buffer (`-e` / `:in`), `:`-command awareness.
- [ ] **LSP server** — language server for birnam. Open scope: diagnostics (parse/eval errors surfaced from the fact store), hover/jump for selectors + entities, completion over installed selectors, document symbols. Decide host: in-birnam (dogfood — server as a birnam program over the future networking/IO surface) vs a thin zig/external shim.
