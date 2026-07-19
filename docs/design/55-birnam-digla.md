# 55 — Birnam Digla

> **Status:** In progress, updated 2026-07-19. This is a product-facing application design
> over the shipped reflective runtime, relational Store, images, project
> workflow, HTTP/HTML/JSON modules, and live-module/isolate boundaries. The
> implementation sequence deliberately starts as a local, read-only report and
> only later becomes a server or worker control plane.

## 1. The question

Birnam's live image is inspectable, but inspection is currently a collection of
REPL techniques: print a value, find its vtable, query its facts, look up a
method, and manually connect that information. The fact store already contains
the relationships needed to answer those questions. Digla turns them
into one small application.

The first useful question is:

> Given an object or named global, what is it, what can it do, what facts are
> attached to it, and what changed recently?

The later operational question is:

> Which worker or generation is serving this behavior, what state did it have
> before a rollout, and what changed after activation?

Digla is a read-oriented workbench for answering both questions. It is
not a second object system, a debugger with privileged pointer access, or a
replacement for the LSP.

## 2. Product shape

Digla has three faces that share one snapshot model:

```text
local Birnam image
        │
        ▼
  Digla query layer ──► text report
        │                  ├─► JSON snapshot
        │                  └─► HTML report / loopback HTTP
        │
        └─ later: worker observations and rollout records
```

The first release is a local, read-only application loaded into the same image
as the thing being inspected. It can inspect one explicitly named target and
write a report. It does not need a new host API, a network protocol, an image
reader, or a background process.

The second release can serve that same report over loopback HTTP. A later
worker-facing release adds a separate control-plane Store containing serializable
worker observations; it does not attempt to merge another process's Store or
ship raw object handles across the isolate boundary.

The object-inspection path is implemented in `modules/digla/inspection.birnam`:
named and anonymous targets, current facts, effective methods, generation and
basis labels, deterministic text, and structured missing-global warnings all
share one capability-free snapshot. The terminal exposes it through `:inspect`.
The transport-neutral session layer also carries the same snapshot over framed
JSON with `--session-stdio`, so machine clients do not need to scrape terminal
text. The terminal sibling now provides resolver-backed authored source on
demand for project and dependency methods, without putting bodies into inspection
snapshots. The general `snapshot: spec` also supports honest `asOf` facts and
`since` changes, exposed in the terminal through `:changes NAME BASIS`. A JSON
file artifact, HTML, and HTTP remain the next Design 55 layers.

## 3. Governing invariants

### 3.1 Digla is a read-only lens

An inspection request must not evaluate arbitrary source, install or remove a
method, activate a compiled unit, save an image, or assert facts into the
observed image. A request is a bounded query plus rendering.

Dashboard preferences, saved query descriptions, and worker observations may
become Digla-owned facts later, but they are separate from a read-only
inspection of the live image.

### 3.2 The live object environment remains canonical

Dispatch, vtable inheritance, and native method bindings are read from the live
object environment. The Store is used as the relational data surface and as the
reflection/meta mirror. Digla must use `Reflect` for effective methods and
the Store for facts; it must not infer the live protocol from a possibly absent
or historical `[vtable selector method]` mirror fact.

This preserves the current authority contract in [45 — Compiled Units](45-compiled-units.md)
and [42 — Object Canonical](42-object-canonical-relational-derived.md).

### 3.3 Results contain values, never runtime capabilities

Snapshot records may contain strings, symbols, numbers, booleans, nil, vectors,
and maps. They must not expose raw Oops, function pointers, vtable pointers,
compiled-unit tokens, file descriptors, sockets, or isolate handles.

An object target is identified by a global name when one exists. An anonymous
object may be inspected locally, but it gets no invented durable identity in the
v1 wire format.

### 3.4 A snapshot has an explicit basis

Every fact-bearing snapshot records the Store basis used for its queries. A
current snapshot captures a basis before collecting facts and uses that basis
consistently for the relational sections. `asOf:`, `since:`, and `between:` are
explicit operations, not hidden UI state.

The current method protocol and current generation digest are labelled as
current runtime observations. Digla does not claim that `asOf:` can
reconstruct an old native dispatch generation. Historical code inspection is a
later feature built on generation and receipt records.

### 3.5 The first transport is local

The first HTTP server, when added, binds to loopback by default. It exposes
allow-listed read endpoints and never forwards arbitrary REPL expressions. A
non-loopback bind is a later explicit deployment choice with an authentication
and authorization design of its own.

### 3.6 Every slice is independently useful

Each implementation slice should produce a runnable report, fixture, or
endpoint. A slice may add no new Zig code; the existing `Reflect`, `db`, `Doc`,
`Json`, `Html`, `File`, and `Http` surfaces are preferred until a missing seam
is demonstrated by a test.

## 4. Users and first questions

### 4.1 The live-language user

- What does this value represent?
- What prototype supplies its behavior?
- Which current facts are attached to it?
- Which effective selectors does it answer?
- Where is the authored source for one selector?

### 4.2 The image maintainer

- What globals, modules, and generation digest are present?
- What facts changed since a captured basis?
- Is a value durable enough to appear in an image-facing report?
- Which state is current, and which is only a historical fact view?

### 4.3 The later worker operator

- Which worker is selected and healthy?
- Which image and generation digest does it run?
- What was the worker's last known state before an evolution request?
- Which rollout receipt, failure, or pending cleanup condition explains the
  current selection?

The third group is deliberately later. The first two groups can be served by
the current image without inventing a fleet protocol.

## 5. Digla snapshot v1

The snapshot is the seam between querying and presentation. Renderers must not
walk the Store or reflection APIs independently.

The initial shape is a plain value record:

```text
Snapshot {
  schema:             1
  basis:              integer | nil
  asOf:               integer | nil
  since:              integer | nil
  target:             Target
  generationDigest:   string | nil
  sections:           vector of strings
  object:             ObjectCard | nil
  facts:              vector of FactCard
  methods:            vector of MethodCard
  changes:            vector of ChangeCard
  warnings:           vector of string
}

Target {
  globalName:         string | nil
  printString:        string
  vtableName:         string | nil
}

FactCard {
  attribute:          string
  value:              serializable value
}

MethodCard {
  selector:           string
  target:             string | nil
  owner:              string | nil
  kind:               "primitive" | "compiled" | "method"
  sourceAvailable:    boolean | nil
  source:             string | nil
}

ChangeCard {
  entity:             string | nil
  attribute:          string
  value:              serializable value
  rendered:           boolean
  window:             "since" | "between" | "asOf"
  basis:              integer
}
```

The exact Map key representation may use strings for transport stability even
though the in-image API uses symbols. The snapshot must make unavailable
information explicit with nil or a warning; it must not turn an unnameable
object, missing source, or unretained history into a plausible-looking value.
`sourceAvailable` is `nil` when source was not requested, so the default method
inventory does not perform a hidden per-method source probe.

V1 intentionally omits raw entity IDs, complete-store dumps, transaction-log
operation records, and historical method bodies. Those are separate design
questions, not fields to smuggle into the first report.

## 6. Query layer

The query layer has a small, explicit surface. It should be implemented as
ordinary Birnam methods over existing runtime APIs before any host-side bridge
is considered.

```text
Digla inspect: value
Digla inspectGlobal: name
Digla snapshot: spec
Digla renderText: snapshot
Digla renderJson: snapshot
Digla renderHtml: snapshot
```

The `spec` is data, not source. The initial fields are:

```text
{
  target: "global-name",
  sections: #["object" "facts" "methods" "source" "changes"],
  selector: nil | "selectedMethod:",
  basis: nil | integer,
  since: nil | integer,
  includeSource: false | true
}
```

The implementation uses:

- `Reflect vtableOf:`, `Reflect allMethodsOf:`,
  `Reflect methodSource:`, and `Reflect factsOf:` for the object card and
  current protocol;
- `db basis`, `findAll:where:`, `findAll:where:asOf:`,
  `findAll:where:since:`, and `pull:spec:` for relational sections;
- `Codegen currentGenerationDigest` for the current executable-generation
  label;
- `Doc globalNameOf:`, `Doc`, and recorded module sources only when the caller
  explicitly requests source-oriented details or the documentation module is
  loaded for named-object presentation.

There is no `eval:` endpoint in this API. An object can be selected by an
already-bound local value or by a named global lookup, never by an expression
supplied in a report request.

## 7. Presentation

### 7.1 Text is the acceptance surface

The text renderer is deterministic and human-readable. It is the first golden
transcript and the debugging tool for every later renderer. A minimal report
looks like:

```text
target:      #worker
printString: aWorker
vtable:      #LiveExampleWorker
basis:       42
generation:  9e7c...

facts:
  #status       #healthy
  #version      2

methods:
  handle:       compiled   LiveExampleWorker
  healthy       compiled   LiveExampleWorker

changes since 39:
  #status       #healthy
```

### 7.2 JSON is the machine-readable snapshot

JSON is written to a file before HTTP exists. This gives scripts and tests a
stable contract without coupling the application to a socket protocol.

### 7.3 HTML is a report, not a second query engine

The HTML renderer presents the same snapshot with links between a target, its
facts, methods, and source. It may include a small client-side filter, but all
data selection happens before rendering.

### 7.4 HTTP is a thin delivery layer

The first server exposes only read routes over the current image:

```text
GET /                         overview
GET /object/<global-name>     one target snapshot
GET /object/<global-name>.json
GET /object/<global-name>.html
GET /changes/<basis>          bounded current-fact change report
```

The route layer parses the path, constructs an allow-listed spec, calls the
query layer, and renders the result. It does not expose a general Birnam
evaluator or the socket REPL.

## 8. Later worker model

Worker observation is a separate application layer above the local image
inspector.

The Digla control-plane image owns durable records such as:

```text
worker   #identity       "payments-a"
worker   #status         #healthy
worker   #imageDigest    "..."
worker   #generation     "..."
worker   #selected       #true
worker   #lastHealth     1720000000
rollout  #worker         worker
rollout  #acceptedBase   "..."
rollout  #receiptDigest  "..."
rollout  #outcome        #accepted
```

The process handle remains transient in an ordinary object or host-side table.
Only copied-value worker observations and the existing value-like
`EvolutionReceipt` cross the isolate seam. The control plane can therefore use
the Store for history and queries without pretending that another process's
objects are local entities.

The first worker slice is read-only: start one worker, ask its existing health
surface, and record the result. Activation, rollback, replacement, pooling,
authentication, and concurrent routing remain explicit later policies.

## 9. Incremental implementation plan

The slices are ordered so that each one can land and be demonstrated without
waiting for the final Digla architecture.

### Slice 0 — fixture and transcript

**Goal:** establish one stable world to inspect.

Add `examples/digla/` as a small project with a named prototype, one
object with facts, one inherited method, one local method, and a deliberate
fact update after a captured basis. Keep the fixture free of workers,
networking, image saves, and arbitrary evaluation.

**Likely files:**

```text
examples/digla/birnam.toml
examples/digla/main.birnam
examples/digla/digla-fixture.birnam
examples/digla/README.md
```

**Acceptance:** `birnam run examples/digla --build` prints the fixture's
current value and the basis used for its update. A fresh process produces the
same semantic output.

### Slice 1 — one local object card — implemented

**Goal:** answer “what is this object?” in text.

Implement the smallest `Digla inspect: value` method. It returns a Map
with `printString`, `globalName`, `vtableName`, and current `facts`. Do not
enumerate the whole image, add JSON, or add HTTP.

**Likely files:** `modules/digla/digla.birnam`, its composed source parts and
`modules/digla/digla-test.birnam`, plus the fixture project.

**Acceptance:** the fixture prints a deterministic object card; the test covers
a named object, an anonymous object, an object with no facts, and a fact with a
string value. The method performs no Store writes.

### Slice 2 — effective method card — implemented

**Goal:** connect the object to its behavior.

Add `methods` to the object card using `Reflect allMethodsOf:`. Include selector,
target, owner, and method kind. Keep source bodies out of the result; expose
only `sourceAvailable`.

**Acceptance:** the fixture demonstrates an inherited selector, a direct
selector, a primitive, and a Birnam-defined method. A shadowed selector appears
once, with the nearest definition winning.

### Slice 3 — inspect a named global — implemented

**Goal:** make the application usable from a project main without passing a
local object around.

Add `Digla inspectGlobal: name`, where `name` is a symbol or string naming
an existing global. Resolve the global through the existing `Globals` surface;
do not parse or evaluate a user expression.

**Acceptance:** `main.birnam` can inspect `#Fixture` and `#db` by name. Missing
globals produce a structured warning/result rather than a VM failure.

### Slice 4 — snapshot record and JSON file — data contract implemented; file artifact pending

**Goal:** freeze the first machine-readable contract.

Implement `Digla snapshot: spec` for the `object`, `facts`, and `methods`
sections, then serialize it with the existing `Json` and `File` modules. Add a
golden JSON file to the fixture's ignored output directory, or compare its
parsed structure rather than unstable formatting.

**Acceptance:** JSON contains only serializable values; no Oop, closure, vtable
pointer, or runtime handle can reach the renderer. Repeated snapshots of the
same fixture have equal semantic content.

The data-driven `snapshot: spec` selector is implemented for named targets and
section filtering. `birnam/snapshot` exposes the same capability-free value over
the framed session, and round-trip tests cover its JSON-safe result. The remaining
work in this slice is an explicit file artifact/golden acceptance path.

### Slice 5 — source lookup on demand — implemented

**Goal:** make the report explain behavior without making every snapshot large.

Add `includeSource` to the spec. For a selected method, use
`Reflect methodSource:` and include the source only when requested. Do not add a
source search endpoint yet.

**Acceptance:** a method with recorded source returns it; a primitive or
undocumented method returns `nil` plus a truthful warning. The default snapshot
does not contain method bodies.

Callers must provide one `selector` when `includeSource` is true. Digla performs
exactly that selected `Reflect methodSource:` lookup; ordinary inspection leaves
`source` and `sourceAvailable` nil. The terminal exposes the same operation as
`:inspect GLOBAL SELECTOR`, and both local and framed clients receive the shared
snapshot shape.

### Slice 6 — deterministic HTML report

**Goal:** make the fact store legible to a human who does not know the REPL.

Render the same snapshot using `Html`. Add links or anchors for facts, methods,
and source sections, but keep the renderer query-free. Write an HTML report to
disk from the fixture project.

**Acceptance:** the HTML contains the target card, facts, effective methods,
the generation label when available, and escaped source/value text. A golden
smoke checks the important headings and escaping.

### Slice 7 — historical facts and changes — implemented for snapshots, sessions, and terminal text

**Goal:** demonstrate the Store's most distinctive application value.

Add `basis`, `asOf`, and `since` to the snapshot spec. Capture the basis before
the fixture update, query facts through the relational API, and label the
result as a historical fact view rather than historical code.

**Acceptance:** the report shows the old value, the current value, and the
change window. A history-off attribute produces an explicit unavailable-history
warning rather than a fabricated old value. Tests cover `asOf:`, `since:`, and
the documented history-retention boundary.

`snapshot:` accepts `asOf` (with `basis` as an alias) and `since`, validates
both against the live Store watermark, and performs relational reads only for
requested sections. Historical facts and current change cards cross the local
and framed session seam as copied JSON-safe values. Text labels the windows as
`facts as of N` and `changes since N`; `:changes NAME BASIS` requests those
sections without evaluating source or traversing the method inventory. Methods
and generation remain current observations and carry an explicit warning when
requested beside historical facts. The remaining HTML/report work belongs to
Slice 6 rather than this query contract.

### Slice 8 — loopback HTTP delivery

**Goal:** turn the local report into a tiny Digla application.

Add a read-only server using the existing `Http`/`Socket` surface. Serve the
overview and one named object route on `127.0.0.1`. The route only accepts
global names and section flags from an allow-list.

**Acceptance:** a clean project run serves text, JSON, and HTML for the fixture;
an arbitrary expression is not evaluated; a non-loopback bind is rejected or
requires an explicit later option. Add a bounded integration smoke and leave
the server's shutdown behavior documented.

### Slice 9 — image-facing read-only mode

**Goal:** inspect a saved application image without changing its state.

Package the Digla module and fixture as an image application whose
`main` serves the report. Validate both mapped boot and `--rebuild` behavior.
Do not add a new image format or a CLI command yet; use the existing project
run/image workflow.

**Acceptance:** the same image produces equivalent object/fact reports under
normal boot and explicit source recovery. The report identifies its current
generation digest and does not claim that source recovery is the normal
executable path.

### Slice 10 — one observed worker

**Goal:** prove that the local snapshot model can cross the existing isolate
boundary safely.

Add a control-plane `Worker` record and one read-only adapter that starts or
connects to a single worker, performs the existing bounded health request, and
asserts only copied-value status, image digest, generation digest, and receipt
metadata into the Digla Store.

**Acceptance:** worker success, timeout, application failure, and exit are
represented as distinguishable facts. No process handle is serialized. The
control-plane report can answer “what was the last known state of this worker?”
using ordinary Store queries.

### Slice 11 — rollout history view

**Goal:** connect historical application state to live evolution.

Record accepted and rejected `EvolutionReceipt` outcomes and render a timeline
of worker generation changes. This slice remains read-only with respect to
operator actions; it observes an explicitly invoked existing rollout.

**Acceptance:** the report distinguishes base-digest mismatch, preparation
failure, activation success, rollback, and pending cleanup. A worker's current
selection remains explainable from its facts and receipts.

## 10. Ownership and verification

The first slices should be Birnam-authored. Zig changes are justified only when
an existing public surface cannot produce a safe, serializable result.

```text
modules/digla/digla.birnam      query and snapshot facade entry
modules/digla/*.birnam          composed Digla implementation parts
modules/digla/digla-test.birnam Store/reflection/report tests
examples/digla/                  runnable fixture and project smoke
scripts/digla-smoke.sh           later text/JSON/HTML/HTTP acceptance
src/id/codegen.zig                only if a missing reflection seam is proven
src/id/main.zig                   only for a later explicit CLI/image bridge
```

Validation should proceed in increasing scope:

1. module tests and fixture transcript after Slices 1–3;
2. JSON/HTML golden checks after Slices 4–6;
3. relational history tests after Slice 7;
4. HTTP smoke after Slice 8;
5. `zig build verify` after any cross-layer change or before declaring an image,
   project, or isolate slice complete.

No slice should require a clean worktree beyond the files it owns. In
particular, Digla work must not rewrite the current live-module or
project-tooling design surfaces unless an implementation actually changes their
contract.

## 11. Explicit non-goals

- no arbitrary remote REPL or eval endpoint;
- no debugger protocol, breakpoints, stepping, or native memory inspection;
- no fact-driven replacement of canonical object dispatch;
- no complete Store dump as the default UI;
- no historical native method reconstruction in v1;
- no cross-process Store merge or shared object identity;
- no implicit activation, rollback, image save, or worker replacement from a
  report request;
- no authentication claim for a non-loopback deployment;
- no LSP replacement or second project dependency resolver.

## 12. First implementation recommendation

Implement Slices 0–3 as one narrow vertical slice: a fixture project that can
print a named object's facts, vtable, and effective methods. Stop there and
evaluate the report as a user-facing artifact before adding JSON or HTTP.

That first slice is small enough to expose mistakes in identity naming,
reflection vocabulary, and deterministic rendering while already proving the
central promise: the same live image can explain an object relationally and
structurally through one coherent application.
