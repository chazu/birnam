# 53 — Project-Aware Language Tools

> **Status:** P0 workspace/coordinate slice, P1 safe static diagnostics, the P2
> project source index/navigation/semantic assistance slice, and the P3 Emacs
> integration slice implemented and extended with project test/profile commands,
> 2026-07-22. This document
> extends the shipped modules/lsp.birnam server and tools/emacs/birnam-mode.el
> around the shared project resolver in [51 — Project Manifests and Dependency
> Resolution](51-project-manifests-and-dependency-resolution.md). It is not a
> second compiler or a second project index.

## 1. The question

Birnam already has a useful editor foothold:

- birnam-mode provides syntax highlighting, indentation, comint, and
  evaluation commands;
- birnam --lsp speaks stdio LSP;
- the server stores open documents, publishes diagnostics, and answers
  hover requests from Birnam documentation;
- the current verification gate exercises installed LSP startup.

These tools understand a project rather than a single buffer. A user can open a
file from a nested directory and get the same modules, dependency errors, source
locations, and project root used by `birnam check/build/run/test`.

## 2. Governing invariants

### 2.1 The project resolver is the semantic source

LSP and Emacs ask the project resolver for root, module identity, dependency
closure, source/artifact origin, and build status. They do not parse
birnam.toml independently or infer dependencies from filenames.

### 2.2 Diagnostics are safe by default

Diagnostics on open/save parse and statically validate source. didChange only
retains the latest full text/version so latency-sensitive completion and
navigation requests cannot queue behind a diagnostic pass for every keystroke.
Static diagnostics do not evaluate top-level forms, install methods, mutate the
user's Store, activate a generation, or write a project artifact. Any future
eval/snapshot diagnostic is an explicit opt-in with a separate process or image
boundary.

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

Workspace metadata is selected at initialization, but project identity is
resolved per document. Successful snapshots are cached by canonical project
root, so multiple documents in one project share one project context while
documents in different workspace folders remain independent. A document change
refreshes only the affected source analysis. Saving a manifest or dependency
file refreshes the disk-backed project snapshot and republishes affected open
documents; unsaved project-topology edits do not alter the resolver graph.

The server may report a project-resolution diagnostic while still providing
buffer-local syntax features for an otherwise editable file.

## 4. Protocol surface

### P0 — harden the existing MVP — implemented first slice

Keep the existing stdio and JSON-RPC contract while adding:

- UTF-16 position conversion;
- full didOpen/didChange/didClose version handling;
- bounded document storage and cleanup on close;
- workspace/root initialization using design 51;
- a narrow host `Project snapshotForUri:` bridge that returns the shared
  resolver's immutable native Map snapshot without duplicating manifest parsing
  or serializing the result through JSON.

Diagnostics remain parse/static only. A malformed or incomplete buffer should
produce useful diagnostics without crashing or mutating the live image.

### P2 — project symbols and navigation — implemented

Add or complete:

~~~text
documentSymbol
workspace/symbol
definition
references
~~~

The server builds a dependency-aware authored-source index from the resolver's
module closure. `documentSymbol` reports top-level `let`, module, and literal
Reflect method declarations; `definition` and `references` use lexical bindings
and indexed method/module occurrences across open and unopened dependency
modules; `workspace/symbol` searches the same index. It does not claim to prove
late-bound dispatch or infer runtime receiver identity.

### P2 — semantic assistance — implemented

The implemented semantic assistance provides:

- method responders and method source;
- canonical module/dependency information;
- in-scope local completion rather than a global-name dump;
- selector arity and receiver information;
- completion entries that distinguish globals, locals, modules, methods, and
  symbols;
- live floor methods when an incomplete send names an exact literal global
  receiver, without dumping every runtime or interned name.

The server uses the existing documentation generator plus authored-source
indexing, while preserving source spans and avoiding generated Zig
implementation details as Birnam source. Completion is intentionally scoped to
the current project and lexical bindings rather than every interned runtime
name. References remain conservative around late-bound dispatch.

### P3 — project commands and live workflow

Emacs exposes check, build, run, and isolated project tests through
project-root-aware compilation commands. Build, run, and test share an explicit
debug/release profile selection. Live staging, activation, rollback, and baseline save remain explicit
commands; an editor action must never activate code merely because a buffer
changed. LSP command/code-action plumbing is deliberately deferred until the
CLI command result contract is exposed as a stable machine-readable protocol;
the editor integration does not hide that boundary.

## 5. Emacs integration

birnam-mode remains a normal major mode and delegates semantic behavior to
lsp-mode. The implemented integration is:

- detect the nearest birnam.toml with project.el;
- start birnam --lsp with the project root as the working directory;
- register the server without requiring users to hand-edit an lsp-mode entry;
- retain run-birnam, region/buffer/line evaluation, and the comint REPL;
- make REPL and evaluation commands use the project root and selected binary;
- add compilation-mode commands for birnam check, build, run, and test, with an
  explicit debug/release profile for artifact-consuming commands;
- expose explicit live-module commands only when the user invokes them.

The mode does not become a package manager or duplicate LSP responses. Its
remaining syntax-specific responsibility is presentation: font-lock,
indentation, balanced editing, and convenient command bindings. Deeper command
awareness is a small mode enhancement, not a replacement for LSP.

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

### P0 — workspace and coordinates — implemented first slice

Project discovery now goes through the shared host resolver, with per-document
project identity, canonical-root snapshot caching, UTF-16 conversion, document
versions, stale-change rejection, close cleanup, save-triggered refresh, and
protocol coverage for multibyte source, nested project paths, and multi-root
workspaces. The first slice publishes resolver failures as LSP diagnostics with
the same kind and message as `birnam check`; open buffers still override only
buffer analysis, not the on-disk project graph.

### P1 — safe static diagnostics — implemented

`Lsp staticDiagnosticsOf:` parses and inspects source without evaluation, Store
mutation, method installation, or artifact writes. The parser/CST is still a
Zig floor capability; the LSP module owns the policy that interprets it. The
LSP converts byte spans to UTF-16 ranges and combines those diagnostics with
the shared project resolver's immutable failure snapshot. The implemented
static boundary is deliberately narrow:

- syntax errors with a source span;
- selector-arity mismatches for literal `Reflect defineMethod:on:as:` and
  `defineMethod:on:as:doc:` declarations when the block parameter count is
  provable;
- no invented unbound-name errors, because ordinary identifiers and dispatch
  remain late-bound globals/runtime behavior.

Repeated didChange requests replace the document text/version without compiling
an artifact or retaining stale documents. didOpen and didSave publish fresh
diagnostics; this keeps the synchronous stdio server responsive to interactive
requests while preserving a deterministic diagnostic boundary.

### P2 — navigation and completion — implemented

Dependency-wide authored-source indexing, method/module provenance hover,
signature help, scoped completion, and conservative references across unopened
project modules are implemented. Source symbols, lexical occurrences, and
lexical bindings are CST walks owned by `modules/lsp.birnam`; Zig only supplies
the parser/CST construction seam.

### P3 — Emacs project commands — implemented

Project-root-aware lsp-mode startup and compilation commands are wired to the
stable CLI. Build, run, and isolated-test commands honor the customizable
`birnam-project-profile`; check retains source-only semantics. An active project
snapshot exposes its selected `buildRoot` and `profile`. The existing
REPL/evaluation workflow remains independent.

## 8. Acceptance scenarios

- Opening src/app.birnam from a nested directory selects the same project root
  and dependency graph as birnam check.
- A syntax error after a non-ASCII character reports the correct UTF-16 range.
- Repeated changes to one document replace its retained text/version; save
  replaces its diagnostics, without retaining an unbounded old document/store
  record.
- A missing dependency is reported by both birnam check and LSP with the same
  diagnostic kind and chain.
- Document symbols include top-level bindings and literal method declarations;
  same-document definition and reference requests return authored source ranges.
- Hover and go-to-definition for a method in a dependency open its authored
  source, not a temporary artifact or unrelated generated code.
- Completion in a local scope does not dump every interned name by default.
- Completion after `[System pri` includes the native `println:` method through
  read-only reflection of that exact receiver.
- M-x lsp/automatic lsp-mode startup works from a clean installed bundle and a
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
