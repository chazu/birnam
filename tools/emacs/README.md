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

## Keys

| Key       | Command              |
|-----------|----------------------|
| `C-c C-z` | `run-birnam`         |
| `C-c C-r` | `birnam-eval-region` |
| `C-c C-b` | `birnam-eval-buffer` |
| `C-c C-l` | `birnam-eval-line`   |

`eval-region`/`eval-buffer` send the text to the REPL verbatim (the REPL
accumulates multi-line forms until brackets balance); `eval-line` sends the
current line as one expression.
