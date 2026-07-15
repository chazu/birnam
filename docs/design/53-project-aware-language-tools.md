# 53 — Project-Aware Language Tools

> **Status:** Proposed, 2026-07-14. This document extends the shipped
> modules/lsp.birnam server and tools/emacs/birnam-mode.el around the shared
> project resolver in [51 — Project Manifests and Dependency
> Resolution](51-project-manifests-and-dependency-resolution.md). It is not a
> second compiler or a second project index.

## 1. The question

Birnam already has a useful editor foothold:

- birnam-mode provides syntax highlighting, indentation, comint, and
  evaluation commands;
- birnam --lsp speaks stdio LSP;
- the server stores open documents, publishes compile diagnostics, and answers
  hover requests from Birnam documentation;
- the current verification gate exercises installed LSP startup.

The next step is to make those tools understand a project rather than a single
buffer. A user should be able to open a file from a nested directory and get
the same modules, dependency errors, source locations, and commands that
birnam check/build/run use.

## 2. Governing invariants

### 2.1 The project resolver is the semantic source

LSP and Emacs ask the project resolver for root, module identity, dependency
closure, source/artifact origin, and build status. They do not parse
birnam.toml independently or infer dependencies from filenames.

### 2.2 Diagnostics are safe by default

Per-keystroke and didChange diagnostics parse and statically validate source.
They do not evaluate top-level forms, install methods, mutate the user's Store,
activate a generation, or write a project artifact. Any future eval/snapshot
diagnostic is an explicit opt-in or on-save operation with a separate process or
image boundary.

### 2.3 Editor coordinates are LSP coordinates

All protocol positions use UTF-16 code units as required by LSP. Internal source
spans may remain byte offsets, but conversion happens at the protocol boundary
and is tested with non-ASCII text before any feature claims a range.

### 2.4 Source provenance is first-class

Definitions, references, hover, diagnostics, and completions identify the
canonical module and authored source span that produced them. A generated
artifact path or a runtime Store entity is not sufficient as an editor location.

### 2.5 Open buffers override disk for analysis only

An open document's text is the source for diagnostics and local features. It is
not written to disk, installed into the running image, or treated as the
project's active compiled generation until the user explicitly saves/builds or
uses the live-module workflow.

### 2.6 Editor integration stays thin

The server owns protocol framing, workspace state, and calls into Birnam's
parser/compiler/reflection APIs. Emacs owns presentation and commands. Neither
should grow a shadow grammar, dependency resolver, or evaluator.

## 3. Workspace model

On initialize, the server determines a workspace in this order:

1. explicit LSP workspace folder/root URI;
2. nearest birnam.toml for the opened document;
3. the process working directory as a no-manifest fallback.

The server creates a ProjectWorkspace containing:

~~~text
project root and manifest path
ProjectResolution, when one exists
open URI → document text/version
source path ↔ canonical module index
diagnostic version per document
selected build/profile status
~~~

Workspace refresh is explicit at initialization, configuration change, and
dependency/build notification. A document change refreshes only the affected
source analysis unless the module declaration or dependency metadata changed.

The server may report a project-resolution diagnostic while still providing
buffer-local syntax features for an otherwise editable file.

## 4. Protocol surface

### P0 — harden the existing MVP

Keep the existing stdio and JSON-RPC contract while adding:

- UTF-16 position conversion;
- full didOpen/didChange/didClose version handling;
- parse diagnostics with accurate spans and expected-token information;
- static unbound-variable and selector-arity diagnostics where the existing AST
  and reflection surface can prove them;
- bounded document storage and cleanup on close;
- workspace/root initialization using design 51.

Diagnostics remain parse/static only. A malformed or incomplete buffer should
produce useful diagnostics without crashing or mutating the live image.

### P1 — project symbols and navigation

Add or complete:

~~~text
documentSymbol
workspace/symbol
definition
references
~~~

Definitions and references use source-location provenance recorded when a module
or method is installed or indexed. A definition in an unopened dependency may
be served from the resolver's source index; it must not require executing that
dependency in the user's image.

### P2 — semantic assistance

Extend the current lexical hover/signature support with:

- method responders and method source;
- canonical module/dependency information;
- in-scope local completion rather than a global-name dump;
- selector arity and receiver information;
- completion entries that distinguish globals, locals, modules, methods, and
  symbols.

The server can use the existing documentation generator and reflection facts,
but must preserve authored source spans and avoid presenting generated Zig
implementation details as Birnam source.

### P3 — project commands and live workflow

Expose check, build, and run through LSP command/code-action hooks only after
design 52 has stable machine diagnostics. Live staging, activation, rollback,
and baseline save remain explicit commands; an editor action must never activate
code merely because a buffer changed.

## 5. Emacs integration

birnam-mode remains a normal major mode and delegates semantic behavior to
Eglot. The proposed integration is:

- detect the nearest birnam.toml with project.el;
- start birnam --lsp with the project root as the working directory;
- register the server without requiring users to hand-edit an Eglot entry;
- retain run-birnam, region/buffer/line evaluation, and the comint REPL;
- make REPL and evaluation commands use the project root and selected binary;
- add compilation-mode commands for birnam check, build, and run once those
  commands exist;
- expose explicit live-module commands only when the user invokes them.

The mode should not become a package manager or duplicate LSP responses. Its
remaining syntax-specific responsibility is presentation: font-lock,
indentation, balanced editing, and convenient command bindings. Deeper :
command awareness is a small mode enhancement, not a replacement for LSP.

## 6. Static analysis boundary

The first static checker may use the existing PEG CST, AST spans, interned names,
selector metadata, and compile error values. It should report:

~~~text
syntax error with farthest-fail span
unbound local/query variable when statically provable
invalid selector arity when statically provable
unknown project/module dependency
stale or incompatible selected artifact
~~~

It must not pretend to solve late-bound dispatch globally. When a conclusion
depends on runtime receiver identity, the tool reports uncertainty or omits the
diagnostic rather than inventing a type system.

## 7. Implementation slices

### P0 — workspace and coordinates

Factor project discovery into the shared resolver, add UTF-16 conversion and
document versions, and lock the behavior with protocol fixtures containing
multibyte source and nested project paths.

### P1 — safe static diagnostics

Add non-mutating static checks and project-resolution diagnostics. Confirm that
repeated didChange requests do not grow the Store or leave stale documents.

### P2 — navigation and completion

Record/consume source provenance, implement definitions/references, then improve
hover, signature help, and scoped completion.

### P3 — Emacs project commands

Wire project-root-aware Eglot and compilation commands to the stable CLI. Keep
the existing REPL/evaluation workflow available throughout.

## 8. Acceptance scenarios

- Opening src/app.birnam from a nested directory selects the same project root
  and dependency graph as birnam check.
- A syntax error after a non-ASCII character reports the correct UTF-16 range.
- Repeated changes to one document replace its diagnostics and do not retain an
  unbounded old document/store record.
- A missing dependency is reported by both birnam check and LSP with the same
  diagnostic kind and chain.
- Hover and go-to-definition for a method in a dependency open its authored
  source, not a temporary artifact or unrelated generated code.
- Completion in a local scope does not dump every interned name by default.
- M-x eglot/automatic Eglot startup works from a clean installed bundle and a
  checkout, regardless of the editor's current cwd.
- Emacs check and build commands show standard compiler diagnostics in a
  compilation buffer and leave the REPL/live image untouched.
- Region evaluation still works independently of LSP and project checking.

## 9. Explicit non-goals

- no type system or whole-program late-bound dispatch proof;
- no evaluation on every keystroke;
- no implicit live activation or image mutation from editor save;
- no editor-specific dependency parser;
- no requirement that users abandon the REPL or direct file workflow.
