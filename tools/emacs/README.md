# birnam-mode

A basic Emacs major mode for Birnam source (`.birnam`, `.bim`).

## Install

```elisp
(add-to-list 'load-path "/path/to/birnam/tools/emacs")
(require 'birnam-mode)

;; if `birnam' is not on PATH:
(setq birnam-program "/path/to/birnam/zig-out/bin/birnam")
```

`birnam-mode` auto-activates for `.birnam` and `.bim` files.

## Features

- **Syntax highlighting** — `//` line comments (a lone `/` stays division),
  `".."` strings, `let nil true false self super`, keyword selectors
  (`at:put:`), symbols (`#foo`, `#at:put:`), datalog query vars (`?name`),
  block params (`:x`), `:=` / `|>`, numbers.
- **Indentation** — by bracket nesting depth (`birnam-indent-offset`, default 2).
  Simple by design; adjust wrapped sends by hand.
- **REPL** — `M-x run-birnam` starts an inferior shell via comint.
- **LSP** — connects to `birnam --lsp` for live parse diagnostics (P0).
  Registered for both **lsp-mode** and **eglot**; both honour `birnam-program`.
  - lsp-mode: `M-x lsp` (or `lsp-deferred`). Auto-start:
    `(add-hook 'birnam-mode-hook #'lsp-deferred)`
  - eglot: `M-x eglot`.

## Keys

| Key       | Command              |
|-----------|----------------------|
| `C-c C-z` | `run-birnam`         |
| `C-c C-r` | `birnam-eval-region` |
| `C-c C-b` | `birnam-eval-buffer` |
| `C-c C-l` | `birnam-eval-line`   |

`eval-region`/`eval-buffer` file the text into the REPL as a temp unit
(`:in /tmp/...`); `eval-line` sends the current line as one expression.
