# birnam-mode

A basic Emacs major mode for Birnam source (`.birnam`, `.bim`).

## Install

```elisp
(add-to-list 'load-path "/path/to/birnam/tools/emacs")
(require 'birnam-mode)

;; if `birnam' is not on PATH:
(setq birnam-program "/path/to/birnam/.local/bin/birnam")

;; optional: lsp-mode uses the same binary with --lsp
(setq birnam-lsp-program birnam-program)
```

`birnam-mode` auto-activates for `.birnam` and `.bim` files.

## Features

- **Syntax highlighting** — `//` line comments (a lone `/` stays division),
  `".."` strings (with `\` escapes), `let nil true false self`, keyword
  selectors (`at:put:`), symbols (`#foo`, `#at:put:`, operator symbols like
  `#+`), datalog query vars (`?name`), block params (`:x`), Capitalized
  globals/prototypes (`System`, `Object`, …), `:=` / `->` / `|>`, numbers.
- **Indentation** — by bracket nesting depth (`birnam-indent-offset`, default 2),
  with Obj-C-style keyword alignment: a wrapped keyword message aligns each
  continuation part's colon under the first keyword's colon on TAB (the first
  keyword, or a part too long to fit, falls back to a one-step hang). Spaces
  only, so the columns hold regardless of `tab-width`.
- **REPL** — `M-x run-birnam` starts `birnam --repl` in an inferior comint
  buffer. The REPL reads Birnam directly: an expression prints its value,
  `let x := …` binds a name that persists across lines, and multi-line forms
  continue until the brackets balance. Commands: `:help`, `:globals`, `:quit`.
- **lsp-mode** — when lsp-mode is installed, Birnam buffers start the bundled
  `birnam --lsp` client automatically, via `lsp-deferred` so a `.birnam` file
  opened invisibly (a grep hit, a project search) does not spawn a server
  until it is shown. Set `birnam-lsp-automatic-start` to nil to make startup
  manual with `M-x birnam-lsp-start`. The nearest `birnam.toml` is used as the
  project root, including when Emacs was launched from another directory:
  `birnam-mode` registers Birnam projects with `project.el` and sets
  `lsp-auto-guess-root` buffer-locally, so lsp-mode resolves the root instead
  of prompting. The server provides project navigation, hover, references,
  completion, and signature help across authored dependencies.
- **Project commands** — `birnam-check-project`, `birnam-build-project`,
  `birnam-run-project`, and `birnam-test-project` use the nearest project root
  and show standard output in `compilation-mode`. Build, run, and test use
  `birnam-project-profile` (`debug` by default, or `release`). Run and test
  pass `--build` so they never fail on a stale or absent artifact manifest,
  and run also passes `--no-repl` — a compilation buffer has no terminal, so
  a project with `run.repl = "tty"` would otherwise stall there; use
  `run-birnam` for an interactive REPL. Diagnostics are printed as
  `path:line:column: compileFailed: message`, which `compilation-mode`'s
  built-in GNU rule already turns into clickable locations.

## Keys

| Key       | Command              |
|-----------|----------------------|
| `C-c C-z` | `run-birnam`         |
| `C-c C-r` | `birnam-eval-region` |
| `C-c C-b` | `birnam-eval-buffer` |
| `C-c C-l` | `birnam-eval-line`   |
| `C-c C-k` | `birnam-check-project` |
| `C-c C-u` | `birnam-build-project` |
| `C-c C-p` | `birnam-run-project` |
| `C-c C-t` | `birnam-test-project` |
| `C-c C-e` | `birnam-lsp-start` |

`eval-region`/`eval-buffer` send the text to the REPL verbatim (the REPL
accumulates multi-line forms until brackets balance); `eval-line` sends the
current line as one expression.

TAB indents first, then completes an already-indented line. For example,
`[ Syst` completes runtime global names and `[ System pri` completes methods.
If the first TAB request needs an LSP reply, completion resumes when it arrives
while the cursor and buffer remain unchanged. Later requests use cached results.
Automatic popups are controlled by the completion UI, such as Corfu's `corfu-auto`.

## Tests

```sh
mise run emacs-test            # or: bash scripts/emacs-test.sh
BIRNAM_EMACS=/path/to/emacs mise run emacs-test
```

The ERT suite uses a small lsp-mode API stub, so it does not require the
package to be installed to validate client registration, project-root
resolution, and the project command lines.
