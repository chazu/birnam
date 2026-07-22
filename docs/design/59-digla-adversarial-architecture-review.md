# 59 — Digla Adversarial Architecture Review

> **Status:** Historical, retired 2026-07-21. This document reviews the removed
> Digla implementation at `57f8c0e`; Section 8 records its last remediation
> state through `f9f6e8d`. Referenced application paths describe the tree before
> retirement at `0496dbc` and are intentionally no longer live.

## 1. Scope and method

The review covers the current Digla application in:

- `modules/digla/`;
- `wrapped/digla-reflect/`;
- `examples/digla/`;
- [55 — Birnam Digla](55-birnam-digla.md); and
- [58 — Terminal REPL and Programming Environment](58-terminal-repl-and-programming-environment.md).

Two independent adversarial passes were made:

1. a **Standards** pass against the repository's documented object, Store,
   compiled-unit, value, module, and style principles; and
2. a **Spec** pass against the behavior and ownership promised by Designs 55
   and 58.

The authority baseline is the current contract in
[42 — Object Canonical](42-object-canonical-relational-derived.md) and
[45 — Compiled Units](45-compiled-units.md). The older tenets in
`docs/misc/VISION.md` are used only where they remain compatible with those
decisions. In particular:

- compatible compiled units are the primary executable layer;
- the active object environment is canonical for identity and dispatch;
- Store is the historied, relational MOP mirror rather than dispatch authority;
- supported post-bootstrap meta-mutation is Store-arbitrated;
- raw floor mutation is a bootstrap or private escape hatch; and
- source is an authoring, inspection, bootstrap, and recovery surface rather
  than normal runtime authority.

## 2. Executive verdict

Digla has a coherent copied-value inspection model and a useful
terminal-independent editing core. Its browser, snapshot, history, and guarded
source-save work generally reinforce Birnam's live-image direction.

The largest architectural problem is the project-local native accelerator. It
gets performance and session behavior by replacing public methods on `Reflect`,
`System`, `Block`, and `Integer` through raw vtable mutation. That makes an
optional application module silently alter core image semantics outside the
supported historied MOP path. The same wrapper owns evaluation state in
process-global Zig variables and duplicates language/layout logic that the
terminal design says should have one shared owner.

The second problem is boundary honesty. Design 58 selects a separate terminal
client with distinct editor, language-service, and session responsibilities,
but the shipped project is one `Digla` singleton and one eager `diglaApp` load
root containing Termbox, sockets, browser, local image access, and server code.
The implementation is useful, but the design currently describes a cleaner
capability and deployment boundary than the code provides.

## 3. Standards findings

### S1. Raw public-protocol replacement bypasses the supported MOP

**Severity:** Critical. **Classification:** Hard violation.

[`bringUp`](../../wrapped/digla-reflect/digla_reflect.zig#L851) installs native
implementations directly with `api.method` for:

- `Reflect allMethodsOf:`;
- `System print:` and `println:`;
- `Block whileTrue:` and `whileFalse:`;
- `Integer to:do:`; and
- the Digla accelerator selectors.

The wrapper is loaded post-bootstrap by
[`reflect-accelerator.birnam`](../../modules/digla/reflect-accelerator.birnam#L9).
Design 42 requires supported post-bootstrap public meta-mutation to enter through
Store's semantic operation. Raw mutation is retained for bootstrap and private
mechanism, and its mutation hook may only degrade the mirror to honest absence.

The current implementation therefore leaves the affected bindings unhistoried
and potentially absent from the public MOP. Loading an optional application also
changes the semantics of canonical language protocols for every caller in that
image.

**Corrective direction:** expose Digla-owned native helpers instead of replacing
public protocols. A genuinely universal replacement should be installed through
a tracked compiled-unit successor or another explicitly accepted Store-arbitrated
operation.

### S2. Evaluation and reflection state is process-global native state

**Severity:** High. **Classification:** Hard violation.

The accelerator keeps its global-name cache, output-capture flag and buffer,
stream sink, deadline, and interrupt probe in file-global Zig variables at
[`digla_reflect.zig:23`](../../wrapped/digla-reflect/digla_reflect.zig#L23).

This state is neither attached to a `ReplSession` nor scoped to one evaluation.
A nested evaluation, a second session, or a future concurrent client can capture
another request's output, overwrite its sink, or interrupt the wrong evaluation.
It repeats the process-global scratch pattern that Design 42 rejects for the
relational engine and weakens Birnam's keep-concurrency-viable posture.

**Corrective direction:** make accelerator state an explicit session/evaluation
context. Pass that capability to Digla-native helpers, or place evaluation in an
owned isolate when stronger interruption or concurrency is required.

### S3. `nil` is used as both a value and an absence sentinel

**Severity:** High. **Classification:** Repository-principle conflict.

Digla stores `nil` for absent editor modes, missing snapshot fields, unavailable
method source, inactive operations, callbacks, connection state, and optional
transport responses. Examples include
[`editor.birnam`](../../modules/digla/editor.birnam#L13),
[`inspection.birnam`](../../modules/digla/inspection.birnam#L51), and
[`socket-protocol.birnam`](../../modules/digla/socket-protocol.birnam#L212).

Birnam's no-NULL API discipline says `nil` is a real object and absence must ride
a distinct channel such as `at:ifAbsent:`, an empty result, or a tagged outcome.
Design 55 itself specifies nullable snapshot fields, so the contradiction is not
only an implementation mistake: Digla's v1 schema conflicts with the broader
Birnam value discipline.

JSON `null` may remain an appropriate wire representation when the schema says a
field is present with an explicit null value. Internal maps should still preserve
the difference between absent, unavailable, and explicitly `nil`.

**Corrective direction:** omit optional internal keys or use tagged
option/outcome records. Make any wire conversion to JSON `null` explicit at the
transport boundary.

### S4. Session extensibility is stringly and centrally dispatched

**Severity:** Medium. **Classification:** Judgment call.

[`localSessionMethod:params:session:`](../../modules/digla/local-session.birnam#L324)
compares `"birnam/*"` method strings in one dispatcher, while
[`performSessionRequest:in:`](../../modules/digla/local-session.birnam#L407)
switches centrally on adapter tags. The same `Digla` singleton also owns editor,
language-service, inspection, rendering, transport, and browser methods.

This works at the JSON boundary but spreads wire vocabulary into the in-image
object model. Adding a session adapter or request requires editing central
conditionals instead of teaching a prototype a shared message protocol.

**Corrective direction:** introduce adapter prototypes implementing a common
request operation. Keep string method names and map-shaped envelopes at the
framing/copy boundary.

### S5. `diglaApp` does not register its documentation ownership

**Severity:** Medium. **Classification:** Draft-standard violation.

[`app.birnam`](../../modules/digla/app.birnam#L3) declares `#diglaApp`, and the
manifest composes the browser source files into that module, but it never calls
`Reflect docModule:is:` before those method definitions. The draft style guide
requires registration before the first definition so method documentation is
attributed to the intended module.

**Corrective direction:** register `#diglaApp` before its composed definitions,
or move the methods to a documented module with the correct ownership.

## 4. Spec findings

### P1. The accelerator contradicts Digla's reflection contract

**Severity:** Critical.

Design 55 says the live object environment remains canonical, Digla uses
`Reflect` for effective methods, and Store supplies facts and the MOP mirror.
Instead, Digla first replaces `Reflect allMethodsOf:` through its own private
native module and then queries that altered surface.

The optimized implementation does enumerate the live vtable chain, but the
inspector now depends on an application-specific mutation of the reflective
protocol it is meant to observe. This is the spec expression of S1 rather than a
separate mechanism defect.

**Corrective direction:** call a Digla-specific accelerator behind the snapshot
implementation, retaining the standard `Reflect` contract as the public and
historied behavior.

### P2. The selected separate-client architecture is not separated in deployment

**Severity:** High.

Design 58 selects this ownership shape:

```text
termbox client
  +-- EditorModel
  +-- LanguageService
  +-- ReplSession
        +-- LocalImageSession
        +-- TcpImageSession
```

It also says UI state and terminal failure stay out of the application image.
The shipped manifest instead always loads `diglaApp`; that root requires sockets
and Termbox and composes browser methods into the same application. Headless
stdio/server launches therefore acquire UI dependencies, and local UI operation
runs beside the image it edits.

The source files provide useful conceptual seams, but every method lands on the
same `Digla` prototype and shares one application load closure. The capability
and deployment boundary exists mostly by convention.

**Corrective direction:** split client and server load roots. Give `EditorModel`,
`LanguageService`, `LocalImageSession`, and `TcpImageSession` distinct prototypes
with explicit capabilities.

### P3. Application-triggered GC masks a compiled-generation correctness defect

**Severity:** High.

[`app.birnam`](../../modules/digla/app.birnam#L4) runs `[System gc]` before the
browser methods are installed. Design 58 records that this prevents
`invalidSourceMap` when the browser extends an already-large compiled unit.

The collection is narrow and preserves normal runtime GC afterward, but it makes
correct compiled-unit loading depend on application-controlled allocation timing.
That is incompatible with the robust generation and image lifecycle promised by
Design 45. A new source part should not need to schedule global collection to
keep the active generation valid.

**Corrective direction:** reproduce and repair the generation/GC rooting defect at
the compiled-unit seam, then remove application-scheduled GC.

### P4. Designs 55 and 58 leave the product/capability boundary unresolved

**Severity:** High. **Classification:** Spec conflict.

Design 55 defines Digla as a read-only lens and explicitly excludes arbitrary
remote REPL or evaluation. Design 58 adds persistent evaluation over framed TCP.
Those capabilities can coexist if the inspector and programming session are
separate products or authority-bearing objects. Today both live on the same
`Digla` singleton and session protocol.

This does not make explicit evaluation intrinsically wrong; it makes the name,
authority, and security boundary ambiguous. A client acquiring “Digla” may be
acquiring a read-only snapshot lens or arbitrary code execution depending on the
entrypoint and configuration.

**Corrective direction:** explicitly supersede Design 55's remote-evaluation
non-goal, or split `DiglaInspector` from a separately authorized programming
session.

### P5. Snapshot method ownership is incorrect and basis consistency is implicit

**Severity:** Medium.

[`inspectionMethodCard:`](../../modules/digla/inspection.birnam#L51) reads the
descriptor's `owner` but writes the target name into the card. An inherited
method can therefore claim the inspected target as its defining owner. This
breaks the `MethodCard.owner` provenance promised by Design 55.

[`snapshot:`](../../modules/digla/inspection.birnam#L216) captures the current
Store basis before collecting facts, but current facts are then read through the
ordinary current reflection surface rather than explicitly at that basis. The
single-threaded image makes the result stable today; the contract is not encoded
and becomes false if readers can overlap a transactor later.

**Corrective direction:** preserve the descriptor's actual owner name and thread
the captured basis through all fact-bearing snapshot reads.

### P6. LSP and coordinate reuse is overclaimed

**Severity:** Medium.

Digla does reuse `Lsp` parsing for diagnostics, source bindings, selector
reconstruction, hover groundwork, and definitions. Production highlighting,
bracket matching, display width, wrapping, and source-to-cell layout are also
implemented independently in Birnam and again in
[`digla_reflect.zig`](../../wrapped/digla-reflect/digla_reflect.zig#L306).

`PositionMap` has focused tests but is not used by the production editor or
language-service path, and the editor carries no document version to bind an LSP
result to the source that produced it. The native accelerator can therefore
drift from both the Birnam fallback and the language grammar while Design 58
claims a single shared mapping/document seam.

**Corrective direction:** establish one versioned document-analysis interface.
Make the native path an implementation of that interface and differentially test
it against the Birnam reference for every supported syntax and Unicode case.

## 5. Commitments Digla currently honors

The findings above should not erase the architecture that is already sound:

- snapshot and browser results are copied, capability-free values;
- method source is requested only for an explicit selection;
- historical facts and current runtime methods are labelled rather than blended;
- the browser and editor store source positions and value state, not terminal
  coordinates;
- renderers consume browser snapshots instead of independently querying Store or
  reflection;
- local and framed sessions share structured request/reply values;
- authored-source saves detect external changes, refuse scratch buffers, and do
  not imply evaluation or activation;
- TCP binds to loopback unless an explicit non-loopback escape hatch is supplied;
- static diagnostics reuse the shared LSP parser; and
- ordinary UI behavior remains outside Birnam's Zig runtime floor.

These are worth preserving through remediation.

## 6. Recommended correction order

### P0 — restore authority and ownership

1. Stop replacing `Reflect`, `System`, `Block`, and `Integer` methods through raw
   application load.
2. Introduce Digla-owned native helpers with explicit session/evaluation context.
3. Move capture, deadline, interrupt, and cache state out of process-global Zig
   variables.

No richer runtime assistance should deepen the current accelerator dependency
before this boundary is corrected.

### P1 — make the claimed seams real

1. Reproduce and fix the `invalidSourceMap` GC/rooting defect; remove `[System gc]`
   from `diglaApp`.
2. Split client, server, and inspection load roots and capability-bearing
   prototypes.
3. Resolve the Design 55/58 product boundary explicitly.
4. Correct method-owner provenance and make basis threading mechanical.

### P2 — converge language and value models

1. Establish the versioned document/analysis/coordinate seam described by
   Design 58.
2. Differentially test native layout/highlighting against the Birnam reference,
   then remove duplicate owners.
3. Replace nil-as-absence internal state with omitted fields or tagged outcomes.
4. Replace string/tag adapter dispatch with prototype messages.
5. Register documentation ownership for every composed module.

## 7. Acceptance criteria

This review can be considered addressed when:

1. loading Digla performs no raw replacement of public core protocols;
2. two session/evaluation contexts cannot share output, deadline, interrupt, or
   reflection-cache state;
3. the Store MOP remains historied and truthful for every supported Digla-related
   method installation;
4. browser source composition remains valid without application-triggered GC;
5. headless/server operation does not load Termbox or editor/browser state;
6. read-only inspection and arbitrary evaluation have distinct, explicit
   capabilities;
7. inherited `MethodCard.owner` values identify their real defining owner;
8. every fact-bearing snapshot read is tied to its recorded basis;
9. one versioned production seam owns parsing-derived spans and coordinate
   conversion, with native/reference conformance tests;
10. internal optional state distinguishes absent from the real `nil` object; and
11. all focused Digla suites, process-level session smokes, real PTY checks, and
    `zig build verify` remain green with startup and interaction benchmarks
    recorded against the preceding checkpoint.

## 8. Remediation outcome

The correction order above was executed as a series of independently testable
commits. The resulting architecture keeps Digla's performance helpers private,
makes authority and deployment boundaries executable rather than conventional,
and preserves explicit copied-value seams at the protocol boundary.

| Criterion | Resolution | Evidence |
|---|---|---|
| 1. No raw public replacement | The wrapper installs only private `DiglaAccelerator` protocol. Cooperative core hooks enter through `Store addMethod:on:with:`. | `b9da9c8`; the session smoke checks the tracked Store facts and canonical vtable identities. |
| 2. Isolated evaluation state | Output buffers/sinks, deadlines, interrupt probes, and the global-name index belong to explicit accelerator instances held by session evaluation contexts. | `b9da9c8`; nested-context smoke returns `a1a2|b`. |
| 3. Truthful Store MOP | Every supported post-bootstrap core hook is Store-arbitrated and checked through `hasValue:attr:`. | `b9da9c8`; `coreHooksTracked` process assertion. |
| 4. No application GC | Prepared dispatch generations and source projections are rooted across collection; `diglaApp` no longer schedules GC. | `0a3b712`, `96f549f`; compiled-unit GC regression coverage. |
| 5. Headless closure | `examples/digla` contains session/inspection/server code; `examples/digla-client` alone composes editor, browser, Termbox, and native display. | `ce751e0`; process smoke proves no `Termbox`, `editorStateFor:`, or browser protocol in the headless image. |
| 6. Explicit capabilities | Inspection and evaluation are distinct capability prototypes, and closed/headless session states carry neither implicitly. | `8ad77b3`; focused session capability tests. |
| 7. Real method owner | `MethodCard.owner` is derived from the descriptor's defining owner rather than the inspected target. | `fc5e4db`; inherited-owner regression test. |
| 8. Basis-bound snapshots | Fact-bearing reads use the captured snapshot basis; changes use an explicit closed basis window. | `fc5e4db`; snapshot mutation-after-basis test. |
| 9. One versioned analysis seam | The editor consumes one document-versioned analysis/frame contract. The native display adapter implements that contract and is differentially checked against the Birnam reference. | `51fa8f4`; `NATIVE-CONFORMANCE:true` in the composed client smoke. |
| 10. Honest optional state | `DiglaOption` distinguishes `none` from `some: nil`. Persistent editor, browser, session, connection, and transport-operation fields use it; snapshot nulls are confined to the explicit v1 copied wire schema. Requests resolve through handler prototypes and adapters share `performSessionRequest:in:`. | `640ea96`; option, nil-result, session, editor, browser, protocol, and socket tests. Designs 55 and 58 now state the boundary. |
| 11. Verification | Focused suites, composed local/TCP session smoke, native/reference conformance, real Termbox PTY launch/input/Escape, and the repository verification graph pass. | Commands and measurements below. |

Every composed module now calls `Reflect docModule:is:` before its first method
definition, including `#diglaReflect` and `#termbox`. This closes S5 and keeps
generated documentation ownership aligned with the actual load roots.

### 8.1 Performance correction discovered during verification

Removing Digla's public `Reflect` override exposed a pre-existing performance
defect in canonical `Reflect allMethodsOf:`: resolving one unexported owner used
repeated `attrAt:index:` and `currentValue:attr:` point reads over all globals.
`f9f6e8d` replaces that path with one call-local `liveTriplesDo:` snapshot and a
reverse index. The index is request-local, so the optimization adds no
process-global scratch state and every descriptor in one result observes the
same Store snapshot.

Five framed evaluations of the exact reported expression, with a persistent
session and a 6,099-byte result each time, measured:

| Measurement | Before `f9f6e8d` | After `f9f6e8d` |
|---|---:|---:|
| `[ Reflect allMethodsOf: "YourMom" ]` | 1,807.1–1,811.7 ms | 30.5–32.2 ms |
| Headless session ready | 1,792.8–1,820.6 ms | 1,801.0–1,814.3 ms |

The reflection interaction is about 57× faster without changing its public
owner or bypassing the MOP. Five real-PTY client launches reached Termbox's
alternate screen in 3,455–3,514 ms. That is 0.17–0.24 s above the historical
3.27–3.29 s app-boundary checkpoint in Design 58; the result is recorded rather
than hidden, and the headless/service startup path remains materially smaller.

### 8.2 Verification record

The final tree passed:

- `zig build test`;
- `zig build test-birnam`;
- `zig build test-project-check`;
- `zig build test-lsp-project`;
- `zig build test-digla-session`; and
- `zig build verify`.

The real PTY check launched `birnam run examples/digla-client`, entered a full
evaluation form through Termbox, observed the client remain responsive, and
shut the alternate screen down cleanly with Escape. The process-level Digla
smoke additionally proves framed output ordering, bounded output, deadlines,
interrupt correlation, post-interrupt health, headless dependency exclusion,
context isolation, tracked hooks, and native/reference analysis conformance.
