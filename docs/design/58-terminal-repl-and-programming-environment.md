# 58 — Terminal REPL and Programming Environment

> **Status:** In progress, updated 2026-07-19. This design explores a terminal-native
> programming environment for Birnam, backed by termbox2, the project-aware
> language service, and an explicit session connection to a running image. It
> leaves the current REPL, LSP, and socket REPL intact; its sibling
> runtime-session protocol is now available over framed stdio and loopback TCP.

## 1. The question

Birnam already has three useful but separate editor surfaces:

- the pure-Birnam console and socket REPL in `modules/repl.birnam`;
- the project-aware JSON-RPC language server in `modules/lsp.birnam`; and
- the reusable `Termbox` module backed by `wrapped/termbox2/`.

The next environment should feel like a real programming tool while retaining
Birnam's defining property: evaluation happens against a live image. A user
should be able to open a terminal editor, connect it to an already-running
image, edit a multiline form, see source diagnostics and completions, evaluate
explicitly, inspect the resulting value, and keep working with history.

The terminal surface is a client. It is not a second compiler, evaluator, project
resolver, or authority model.

The first shared application foundation is now shipped as the self-hosted
`AppConfig` module. It resolves an application's startup snapshot from defaults,
TOML, environment, and CLI layers, with the conventional per-user path. Digla
can consume that snapshot while keeping history and terminal-session state in
their own modules; the terminal client and session protocol described here
remain the next product slice.

The first Digla project slice uses the project builder's multi-file module
composition:

```text
modules/digla/digla.birnam       module entry
modules/digla/config.birnam      AppConfig adapter
modules/digla/session.birnam     transport-neutral request/reply and editor state
modules/digla/inspection.birnam  capability-free live-image snapshots
modules/digla/local-session.birnam synchronous live-image session adapter
modules/digla/protocol.birnam    injected JSON loop and framed stdio adapter
modules/digla/socket-protocol.birnam optional framed TCP adapter
modules/digla/socket-protocol-test.birnam framing and bind-policy tests
modules/digla/render.birnam      headless startup summary
modules/digla/terminal.birnam    local Termbox event loop
modules/digla/position-map.birnam source-byte/LSP UTF-16 conversion
modules/digla/position-map-test.birnam coordinate contract tests
modules/digla/editor.birnam      value-only cursor/edit/history/evaluation model
modules/digla/editor-test.birnam headless editor contract
modules/digla/display-layout.birnam source-to-terminal-cell projection
modules/digla/display-layout-test.birnam terminal coordinate contract tests
modules/digla/analysis.birnam       lexical highlights and LSP diagnostics
modules/digla/analysis-test.birnam analysis contract tests
modules/digla/language-service.birnam source/runtime completion merge
modules/digla/language-service-test.birnam completion contract tests
modules/digla/render-test.birnam    headless renderer decision tests
modules/digla/termbox.birnam     Termbox module entry
wrapped/termbox2/                vendored C library and Alien wrapper
examples/digla/birnam.toml       composed application project
```

`Termbox` is a separate optional module. Its C artifact is built through the
application manifest's `[[wrapped]]` declaration, so the standard image and
headless Digla tests do not acquire a terminal dependency.

The first local editor slice is now implemented. `birnam run examples/digla -- --ui`
launches the Termbox client; Escape exits, character input edits the
buffer, Ctrl-J inserts a newline, Enter evaluates the current Birnam form
against the local image, and history is recalled with Ctrl-P/Ctrl-N or the
arrow keys. The model is headless and stores source cursors as UTF-8 byte
boundaries. The initial PositionMap source/LSP conversion and terminal-cell
DisplayLayout projection slices are implemented and tested. Lexical syntax
highlights and parser-backed diagnostics are now projected through that same
source-coordinate seam. Evaluation, inspection, global enumeration, and image
identity now cross one JSON-RPC-shaped `ReplSession` contract instead of being
called directly by the editor. `--session-stdio` exposes that same contract as
Content-Length-framed JSON for child processes, while `--session-listen` and
`--session-listen-once` expose it over serial loopback TCP sessions for scripts
and AI agents. Tab now opens a completion
menu that merges in-scope parser/CST candidates with bounded live-image globals;
Tab or Down cycles, Up cycles backward, Enter accepts, and Escape dismisses the
menu before it quits the editor. Selection, viewport refinements, hover,
signature help, source navigation, streamed output, and bounded interruption
remain later slices.

## 2. Goals and boundaries

The first environment should provide:

- a terminal-native editor with cursor movement, insertion, deletion, selection,
  multiline input, scrolling, resize handling, and a visible prompt;
- command history stored by the client, with recall and search later;
- parser-backed syntax highlighting, bracket matching, and diagnostics;
- project-aware completion, hover, signature help, and source navigation from
  the existing language-service APIs;
- explicit evaluation against either the local image or a connected image;
- structured values, errors, output, and source locations rather than only a
  stream of preformatted prompt text; and
- one UI that can switch between a local session and a remote session.

The first slice does not promise a full-screen IDE, multiple concurrent users,
remote authentication, arbitrary background execution, or implicit activation of
unsaved source. Changing a buffer is analysis; pressing the evaluation key is an
explicit runtime operation.

## 3. Three interface shapes

### A. Separate terminal client with pluggable sessions — selected

```text
termbox client
  ├── EditorModel
  ├── LanguageService
  └── ReplSession
        ├── LocalImageSession
        └── TcpImageSession
```

The terminal process owns presentation, editing state, history, and local source
analysis. A session owns evaluation and runtime requests. The local session calls
the current image APIs; the TCP session speaks a framed protocol to a running
image.

This keeps terminal failures, terminal modes, and UI state out of the application
image. It also lets the same running image serve a terminal client, an ordinary
socket client, or a future graphical client.

### B. Terminal UI inside the running image

The image would load termbox2, own the terminal, and run the editor beside the
application's live objects.

This gives direct access to runtime state and avoids a session protocol, but it
couples application lifetime to terminal lifetime, makes the image's blocking
loop responsible for terminal redraw, and prevents a clean separation between
the user's editor and the program being edited. It is a useful demonstration mode
for a single-process application, not the foundation for connecting to arbitrary
running images.

### C. LSP-first terminal shell

The terminal would be primarily an LSP client. Evaluation would be a custom LSP
command, and the running image would be treated as another language-server
backend.

This maximizes reuse of editor concepts and standard protocol vocabulary, but
LSP's document model is not a runtime-session model. Evaluation, streamed output,
runtime values, image identity, interrupts, and session capabilities would become
awkward extensions of an editor protocol. LSP remains the language-service
interface; the runtime session is a sibling interface carried by the same framing
where convenient.

The selected design takes the clean ownership boundary from A, the protocol reuse
from C, and the direct runtime access of B only in the local session adapter.

## 4. Ownership model

| Layer | Owns | Does not own |
|---|---|---|
| `EditorModel` | buffer text, cursor, selection, history, viewport, transcript | parsing, evaluation, runtime state |
| `LanguageService` | CST-backed spans, diagnostics, completion, hover, signatures | terminal cells, image mutation |
| `ReplSession` | evaluate, interrupt, inspect, globals, runtime capabilities | key handling, syntax colors |
| `TerminalRenderer` | termbox cells, colors, cursor, resize, redraw | source offsets, protocol framing |
| Running image | evaluation, Store/dispatch, runtime reflection, output events | terminal layout, client history |

The editor model stores the cursor as a source position. The renderer projects
that position into terminal cells; the language service projects it into LSP
positions only at the protocol boundary. No layer should pass terminal `x/y`
coordinates into the parser or use UTF-16 columns as internal source indexes.

## 5. Session protocol

The current `Repl serveOn:` path is a useful compatibility target: it accepts
multiline forms, evaluates them persistently, and serves one blocking client.
Its prompt-oriented string protocol is too weak for a programming environment,
though. It cannot distinguish a value from an error, stream output structurally,
carry source ranges, report capabilities, or support an interrupt cleanly.

The target protocol should reuse the existing JSON-RPC envelope and
`Content-Length` framing already used by LSP. Stdio remains the LSP transport;
TCP becomes an additional transport for a running image. Runtime methods live in
a `birnam/` namespace rather than pretending to be standard LSP methods.

The initial target surface is deliberately small:

```text
birnam/session/open       → session id, image identity, capabilities
birnam/evaluate           → value/error result plus streamed output
birnam/interrupt          → cancel the current evaluation at a safe boundary
birnam/inspect            → structured print value or selected runtime object
birnam/globals            → user-visible persisted bindings
birnam/close              → release the session
```

Language features continue to use ordinary LSP requests such as
`textDocument/completion`, `textDocument/hover`, and diagnostics. Runtime-aware
completion now uses the explicit `birnam/runtimeCompletion` capability. The
terminal-independent language-service seam merges those copied-value results
with source candidates; a client can still offer source candidates when a
session does not implement the runtime method. The local adapter enumerates
global names with one indexed relational query, caches the sorted snapshot per
session, and invalidates it after every evaluation; prefix filtering remains
bounded to 64 results. This keeps repeated explicit completion requests out of
the Store's indexed-attribute loop without adding a new runtime primitive.

The server defaults to loopback binding. A socket REPL is arbitrary code
execution, so binding beyond loopback requires both a non-loopback `--bind` and
the explicit `--allow-remote` flag. The current adapter serves clients serially,
rejects headers above 8 KiB and bodies above 1 MiB, and closes malformed framed
connections. Authentication or an equivalent capability remains necessary
before a future design makes remote binding routine.

## 6. Editor model

The first editor can use a simple one-document model rather than a general text
editor framework:

```text
EditorBuffer
  text
  cursor: SourceOffset
  selection: optional SourceRange
  history: HistoryStore
  diagnostics: [Diagnostic]
  completion: optional CompletionMenu
  viewport: Viewport

Transcript
  prompt / input
  value / error / stdout / stderr entries
```

History is client-side state, not an image fact. A first implementation may use a
bounded line list and a history file; prefix search and reverse search can follow
once the basic loop is reliable. The transcript is separate from the active
buffer, so output can scroll without changing the source being edited.

The editor should reuse the native bracket scanner for fast continuation decisions
and the parser/CST for highlighting and diagnostics. A balanced form is eligible
for evaluation, but balance alone never substitutes for compilation.

## 7. Position and coordinate model

Birnam needs three coordinate systems. They describe the same source but answer
different questions.

### 7.1 Source bytes

The parser and compiler operate on the exact UTF-8 byte string. Internal spans
should therefore use half-open byte ranges:

```text
ByteSpan(start: 12, end: 24)
```

Byte offsets are stable for slicing the original source and are the natural
currency for CST nodes, compiler diagnostics, source-backed method spans, and
editor mutations. A cursor is a boundary between bytes, not a byte that points
at a character. Normal cursor positions must lie on UTF-8 codepoint boundaries.

The document maintains a line index containing raw byte offsets for line starts.
CRLF is treated as one logical newline for line calculations while the original
bytes remain available for evaluation and edits.

### 7.2 LSP UTF-16 positions

LSP positions are `(line, character)`, where `character` counts UTF-16 code units.
This is not a byte offset and not a terminal column: a non-BMP codepoint counts as
two UTF-16 units, while a combining mark usually counts as one even though it may
occupy zero terminal cells.

The LSP adapter owns conversion:

```text
byteOffset → (line, utf16Character)
(line, utf16Character) → byteOffset
```

Conversions should be performed against the exact document version associated
with the request. A position from an old `didChange` version must not be applied
to the new buffer. Positions that land inside a multibyte codepoint or surrogate
pair are normalized to a codepoint boundary consistently and covered by tests.

### 7.3 Terminal cells

Termbox renders a rectangular grid of cells `(x, y)`. Cell coordinates depend on
presentation state:

- tabs expand according to the configured tab width;
- soft-wrapped source lines create additional visual rows;
- combining marks have zero display width;
- wide characters consume two cells;
- prompts and continuation indentation shift the visible origin; and
- the viewport may show only a slice of the transcript or buffer.

The terminal cursor therefore cannot be calculated as `byteOffset - lineStart`.
The renderer builds a `DisplayLayout` for the current buffer and viewport. Each
visual row records the source byte boundary before each displayed cluster, and
the layout supplies both directions:

```text
source byte boundary → terminal cell
terminal cell         → nearest source byte boundary
```

The editor cursor remains a source boundary. The renderer projects it into a cell,
including the special cases for a cursor at the end of a wide or combining
cluster. Mouse selection, if added, projects the selected cell back through the
same layout rather than inventing a second text index.

### 7.4 The mapping layer

The shared document package should own these structures:

```text
TextDocument
  text: String
  version: Integer
  lineIndex: LineIndex
  utf16Index: lazy per-line map

PositionMap
  byteAt(line, utf16Column)
  lspPositionAt(byteOffset)
  byteAtCell(layout, x, y)
  cellAtByte(layout, byteOffset)

DisplayLayout
  visual rows
  source-boundary ↔ cell mappings
  diagnostic/style spans projected onto rows
```

The language service returns byte spans internally. The LSP adapter calls
`lspPositionAt`; the terminal renderer calls `cellAtByte`; edit commands use
`byteAtCell` or the source cursor directly. This keeps Unicode rules in one place
and prevents syntax highlighting, diagnostics, completion, and cursor movement
from drifting apart.

The first mapping tests should cover:

- ASCII and multiline text;
- accented characters and non-BMP emoji;
- combining marks;
- CJK/wide characters;
- tabs and mixed indentation;
- CRLF input;
- soft-wrapped long lines; and
- diagnostics and completion requests after edits to multibyte text.

The core invariants are:

1. every internal source span can be sliced from the document without changing
   its meaning;
2. byte → UTF-16 → byte round-trips at valid codepoint boundaries;
3. byte → cell → byte returns the same or the nearest visible source boundary;
4. all conversions use one document version; and
5. a rendering change never changes the source range sent to the compiler.

## 8. Implementation sequence

1. **PositionMap and DisplayLayout — implemented.** Extract the shared
   UTF-8/UTF-16 and absolute byte-position seam from the existing LSP behavior,
   then project source boundaries into terminal cells with Unicode width and
   logical-line records. Viewport and soft-wrap refinements remain open.
2. **Reusable termbox module — implemented.** Promote the Logue wrapper into an
   optional, reusable terminal module. Its public selectors share one native
   Alien dispatcher so startup does not compile a trampoline per terminal
   operation. Keep terminal setup out of the boot-critical REPL.
3. **Local terminal editor — initial slice implemented.** Add a single-buffer
   `--ui` entrypoint with editing, UTF-8-aware cursor movement, multiline input,
   history, local evaluation, a scrollable transcript against the local image,
   terminal-cell-aware wrapping for evaluation results, lexical syntax spans,
   and parser-backed diagnostics. Autocomplete now merges scoped source
   candidates and bounded runtime globals behind a terminal-independent language
   service, while the editor owns menu state and the renderer owns its cells.
   Selection and viewport refinements are still open.
4. **Compatibility remote session — superseded by the framed seam.** The
   prompt-oriented socket REPL cannot preserve structured values, errors, or
   capabilities without ambiguous text parsing. Keep it compatible for comint
   clients, but put Digla's next remote adapter over the framed contract.
5. **Framed session protocol — stdio and loopback TCP implemented.** JSON-RPC-shaped
   `open`, `evaluate`, `inspect`, `globals`, `interrupt`, and `close` requests now
   run through a transport-neutral value layer. `--session-stdio` reuses the
   shipped Content-Length framing. The optional `diglaSocket` module adds a
   loopback-only-by-default serial TCP listener over the same iterative request
   loop, with a one-client mode for scripts. Evaluation results, compile/runtime
   errors, inspection snapshots, image identity, and truthful capabilities are
   copied values; output notifications and bounded interrupt behavior remain open.
6. **Runtime-aware assistance — inspection and completion slices implemented.** `:inspect`,
   `:globals`, and `:session` use the shared session layer without evaluating
   report expressions. `birnam/runtimeCompletion` supplies bounded live-image
   globals, merged with parser/CST source candidates in the shared language
   service and presented by the terminal menu. Docs, hover, signatures, and
   source-on-demand remain.

The first usable milestone is now complete: a user can launch the terminal editor,
edit and evaluate multiline Birnam forms locally, recall history, see parser-backed
highlighting and diagnostics, and attach a client to a running image over loopback.

The next performance seam is below the Digla transport. In a real project-image
TCP smoke, `[8 * 8]` evaluates in about 8 ms while
`[ Reflect allMethodsOf: "YourMom" ]` takes about 1.76 s. Framing, JSON, and
compilation were separately ruled out; the remaining time is target-name
resolution inside `Reflect allMethodsOf:` against the larger project image. That
floor path needs improvement without coupling the editor or session protocol to
a special-case reflection cache.

## 9. Open decisions

- final CLI spelling: `birnam term`, `birnam --repl=term`, or a separate binary;
- whether the serial listener should evolve into the planned poll-based reactor
  before multiple simultaneous clients become a product requirement;
- history-file location, retention, and opt-out behavior;
- runtime values use a small typed copied-value record, while deeper inspection
  uses the capability-free snapshot schema from Design 55;
- authentication and session ownership once connections can leave loopback.
