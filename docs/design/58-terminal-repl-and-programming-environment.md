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

The shared application foundation is now shipped as the self-hosted
`AppConfig` module. It resolves an application's startup snapshot from defaults,
TOML, environment, and CLI layers, with the conventional per-user path. Digla
consumes that snapshot while keeping history and terminal-session state in its
own modules. The terminal-independent editor now selects either its synchronous
local-image adapter or a lazy framed TCP client from the same configuration.

The first Digla project slice uses the project builder's multi-file module
composition:

```text
modules/digla/digla.birnam       module entry
modules/digla/config.birnam      AppConfig adapter
modules/digla/session.birnam     transport-neutral request/reply and editor state
modules/digla/inspection.birnam  capability-free live-image snapshots
modules/digla/local-session.birnam synchronous live-image session adapter
modules/digla/protocol.birnam    injected JSON loop and framed stdio adapter
modules/digla/socket-protocol.birnam optional framed TCP client/server adapter
modules/digla/socket-protocol-test.birnam framing, client, and bind-policy tests
modules/digla/reflect-accelerator.birnam optional project-local reflection fast path
modules/digla/app.birnam        one dependency root for optional runtime adapters
modules/digla/render.birnam      headless summary and terminal renderer
modules/digla/terminal.birnam    local Termbox event loop
modules/digla/position-map.birnam source-byte/LSP UTF-16 conversion
modules/digla/position-map-test.birnam coordinate contract tests
modules/digla/editor.birnam      value-only cursor/edit/history/evaluation model
modules/digla/editor-test.birnam headless editor contract
modules/digla/display-layout.birnam source-to-terminal-cell projection
modules/digla/display-layout-test.birnam terminal coordinate contract tests
modules/digla/analysis.birnam       lexical highlights and LSP diagnostics
modules/digla/analysis-test.birnam analysis contract tests
modules/digla/language-service.birnam completion, hover, signatures, definitions
modules/digla/language-service-test.birnam semantic assistance contract tests
modules/digla/project-source.birnam lazy resolver-backed authored source lookup
modules/digla/project-source-test.birnam project/source-part navigation tests
modules/digla/render-test.birnam    headless renderer decision tests
modules/digla/termbox.birnam     Termbox module entry
wrapped/termbox2/                vendored C library and native wrapper module
wrapped/digla-reflect/           application-local reflection and display accelerator
examples/digla/birnam.toml       composed application project
```

`Termbox` is a separate optional module. Its application-owned Zig wrapper and C
artifact are built together through the manifest's `[[wrapped]]` declaration.
The wrapper binds public selectors directly through `IdApi`; it does not add a
core runtime primitive, and headless Digla tests do not acquire a terminal
dependency.

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
and AI agents. `--ui --connect=remote` selects the matching lazy client adapter;
the same editor model then sends evaluation, inspection, globals, runtime
completion, identity, and close requests to the connected image while retaining
source analysis and terminal state locally. Correlated `birnam/output`
notifications are merged into the normal transcript without duplicating the
response's buffered event fallback. `:connect local|remote` now switches that
adapter through the terminal-independent command model, performing a handshake
before releasing the old active session and preserving the immutable startup
configuration plus all editor-owned state. Tab now opens a completion
menu that merges in-scope parser/CST candidates with bounded live-image globals;
Tab or Down cycles, Up cycles backward, Enter accepts, and Escape dismisses the
menu before it quits the editor. Shift-arrow/Home/End selection now uses source
byte anchors and replaces or deletes through the ordinary edit operations. A
visual-row viewport follows the cursor through buffers taller than the
terminal without storing terminal coordinates in editor state. F1 hover merges
same-buffer provenance with copied live-image inspection, F2 reconstructs full
keyword selectors for signature help, and F3 first jumps within the buffer, then
lazily opens authored definitions from every composed source part in the active
project or a dependency. F5 restores the previous buffer, cursor, diagnostics,
and viewport from a terminal-independent navigation stack. F4 refreshes cached
parser diagnostics explicitly; source edits only mark the cache stale, so parsing
is not on the per-key render path.
Page Up/Page Down now navigate the wrapped transcript with a clamped tail-relative
offset, and new evaluation output resumes tail following. Evaluated `System print:`
and `System println:` calls are captured by the application wrapper as ordered,
copied output events, inserted before the value in the terminal transcript, and
emitted as `birnam/output` protocol notifications before the final response. Long logical source lines
soft-wrap into visual rows through the same byte/cell projection; cursor motion,
selection, and viewport following remain byte-authoritative across those rows.
Positive `timeoutMs` evaluation requests now bound the canonical stack-flat Block
and Integer loops with application-local cooperative safepoints. Real-time output
delivery and protocol-driven interruption during a still-running evaluation remain
later slices; the synchronous adapter advertises `interrupt=false` truthfully.

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
  diagnosticsDirty: Boolean
  completion: optional CompletionMenu
  assistance: optional AssistancePanel
  viewport: Viewport

Transcript
  prompt / input
  value / error / stdout / stderr entries
```

History is client-side state, not an image fact. A first implementation may use a
bounded line list and a history file; prefix search and reverse search can follow
once the basic loop is reliable. The transcript is separate from the active
buffer, so output can scroll without changing the source being edited.

The editor should reuse the native bracket scanner for fast continuation decisions,
a lexical scanner for highlighting, and the parser/CST for diagnostics and semantic
assistance. Parser work must not run synchronously for every typed character: the
current client marks diagnostics stale on edits and refreshes them on F4 or when Tab
requests completion. A balanced form is eligible for evaluation, but balance alone
never substitutes for compilation.

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
   logical-line records. DisplayLayout now also produces terminal-width soft rows,
   row-relative cursor positions, and visual-row viewport coordinates.
2. **Reusable termbox module — implemented and precompiled.** Promote the Logue
   wrapper into an optional, reusable terminal module. Its public selectors are
   now bound by an application-owned native wrapper over the existing int-clean
   C adapter, avoiding runtime Alien compilation and Birnam forwarding methods.
   Keep terminal setup out of the boot-critical REPL.
3. **Local terminal editor — initial slice implemented.** Add a single-buffer
   `--ui` entrypoint with editing, UTF-8-aware cursor movement, multiline input,
   history, local evaluation, a scrollable transcript against the local image,
   terminal-cell-aware wrapping for evaluation results and lexical syntax spans.
   Parser-backed diagnostics are cached and refreshed by F4 or completion rather
   than reparsed after each edit. Autocomplete merges scoped source
   candidates and bounded runtime globals behind a terminal-independent language
   service, while the editor owns menu state and the renderer owns its cells.
   Shift-movement selection, a cursor-following visual-row viewport, soft-wrapped
   source rows, and manual Page Up/Page Down transcript navigation are implemented.
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
   copied values. The Digla-local wrapped module captures evaluation `System`
   output without touching the runtime floor. The shared protocol emits each event
   as a correlated `birnam/output` notification before the result, retains the
   ordered `events` array as a request/reply fallback, and rejects stray unframed
   stdout in a process-level smoke. Because dispatch is still synchronous, the
   capability remains `streamedOutput=false`; real-time delivery remains open.
   Positive `timeoutMs` requests now activate application-local cooperative checks
   on `Block whileTrue:/whileFalse:` and `Integer to:do:`. Expiry propagates an
   `#evaluationDeadline` error into a structured `phase="timeout"` response and
   leaves the session usable. The adapter advertises this narrower contract as
   `cooperativeDeadline=true` while retaining `interrupt=false`: the serial JSON
   loop cannot consume an interrupt request during evaluation, and arbitrary
   recursion or a blocking native/foreign call has no safe application-level
   preemption point.
6. **Runtime-aware assistance — inspection, completion, and explicit semantic slices implemented.** `:inspect`,
   `:globals`, and `:session` use the shared session layer without evaluating
   report expressions. `birnam/runtimeCompletion` supplies bounded live-image
   globals, merged with parser/CST source candidates in the shared language
   service and presented by the terminal menu. F1 merges source information,
   optional docs, and copied runtime inspection; F2 reports full-selector signatures;
   F3 resolves same-buffer definitions and lazily opens resolver-selected project
   or dependency source parts; F5 restores prior in-memory document state.

The first usable milestone is now complete: a user can launch the terminal editor,
edit and evaluate multiline Birnam forms locally, recall history, see lexical
highlighting and parser-backed diagnostics, and attach a client to a running image
over loopback.

The reflection hot path is now resolved without changing Birnam's core/runtime
floor. Digla loads an application-local native wrapper through `IdApi` and
rebinds `Reflect allMethodsOf:` only in its project image. Exported targets use
the existing constant-time export lookup; live globals use a basis-keyed reverse
index rebuilt through one `Store liveTriplesDo:` pass. The editor and session
protocol remain unaware of the optimization.

In a controlled full-project TCP comparison, the exact expression
`[ Reflect allMethodsOf: "YourMom" ]` produced an identical 5,913-character
result before and after the override (including an identical SHA-256) while
falling from 1.587 s to 17.7 ms, about 90x. After adding a new global and method
to the live image, reflection correctly named both and completed in 12.9 ms;
this also exercises Store-basis cache invalidation. Simple evaluation remains
about 8 ms.

The terminal wrapper is no longer part of that startup bottleneck. Replacing its
runtime-compiled Alien layer with the precompiled wrapped module reduced full
Digla cold launch from about 3.5 s to 1.89-2.00 s in three consecutive runs; a
real Termbox alternate-screen render and Escape shutdown smoke also passed.
After the subsequent navigation, structured-output, deadline, and TCP-client
slices, the current larger application image launches in 2.60-2.64 s across
three consecutive runs. The TCP client implementation is kept to two registered
adapter methods plus transport-neutral session selection so the remote slice does
not grow into another layer of startup helpers.

Project-aware source navigation does not add another manifest parser or eager
workspace index. A project run now installs the same existing read-only `Project`
bridge used by LSP and lends it the already-resolved active `Resolution` for an
empty-workspace request. In three process-level comparisons, materializing that
snapshot measured 2.88-2.90 s total versus a 2.84-2.87 s baseline; resolving and
loading `definitionFor:` from a composed Digla source part measured 3.11-3.17 s,
or about 0.24-0.29 s beyond startup. The lookup reads all ordered source paths but
parses only files containing the requested token. A real PTY opened the authored
file with F3 and restored the scratch buffer with F5. The native bridge also rejects
non-String URI values as `projectUriInvalid` rather than dereferencing them.

Evaluation output now follows the same application-local boundary. The wrapped
module temporarily rebinds `System print:` and `System println:` to collect copied
stdout records only while `Codegen evalPersisting:` runs; outside that interval it
preserves ordinary stdout behavior. This prevents evaluated output from corrupting
stdio JSON framing or painting behind Termbox. The local terminal inserts the
ordered records between the submitted form and its value, while stdio and TCP
clients receive correlated `birnam/output` notifications followed by the ordinary
evaluation response. Consecutive stdout writes share one allocation-conscious
buffer capped at 256 KiB; overflow is visibly marked and further writes are
dropped for that evaluation. A strict process-level decoder verifies both an
ordinary multi-write evaluation and a 512 KiB truncation probe, with no bytes
outside the expected frames. The same process smoke runs a nonterminating
`whileTrue:` and an effectively unbounded `to:do:` with 10 ms deadlines; both
return structured timeout responses before the watchdog and the session then
closes normally.

The TCP client path is also covered process-to-process: a terminal-independent
editor model starts local, switches with `:connect remote`, evaluates
`[System println: 7]; [6 * 7]`, receives the transcript entries in order, and
closes both ends. Five steady remote
round trips of `[ Reflect allMethodsOf: "YourMom" ]` completed in 23.2-24.3 ms
each including framing, JSON, copied-value rendering, and a 6,099-character
response.

The typing path is also bounded away from whole-buffer parser work. Diagnostics
are cached and explicitly refreshed, already-buffered Termbox events are applied
before the next redraw, and the same application-local native wrapper accelerates
the terminal-independent source-frame projection, display columns, lexical spans,
and transcript wrapping. In a real PTY smoke, a 60-byte method definition was
consumed and rendered with syntax colors inside a 500 ms observation window. The
exact `allMethodsOf:` expression above evaluated and rendered its full result as
terminal-width rows inside a one-second observation window; F1/F2/F3/F4 behavior
was verified in that same live project image. Page Up/Page Down then moved through
the wrapped result in persisted 10-row pages, and a subsequent evaluation reset the
viewport to its new tail.

Source wrapping and paste behavior were verified separately in the real PTY. A
2,201-byte logical line occupied 29 visual rows; moving left across a wrap boundary
projected the cursor from continuation column 0 to the preceding row's final cell,
and Shift-End selection rendered across the viewport before a replacement collapsed
it to one row. The Termbox adapter now coalesces consecutive character events into
one editor insertion while retaining a following special key; a 2.2 KiB paste and
redraw completed inside a 500 ms observation window, and a queued Home key was then
delivered correctly.

## 9. Open decisions

- final CLI spelling: `birnam term`, `birnam --repl=term`, or a separate binary;
- whether the serial listener should evolve into the planned poll-based reactor
  before multiple simultaneous clients become a product requirement;
- history-file location, retention, and opt-out behavior;
- runtime values use a small typed copied-value record, while deeper inspection
  uses the capability-free snapshot schema from Design 55;
- authentication and session ownership once connections can leave loopback.
- whether a future protocol interrupt justifies an explicit evaluator safepoint
  below the application layer. The current cooperative deadline intentionally
  covers only Birnam's canonical stack-flat loops; it cannot safely preempt
  recursion, arbitrary native work, or a foreign call.
