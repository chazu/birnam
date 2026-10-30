# 50 — Five-Phase Implementation Plan: Live Modules and Isolates

> **Status:** Retired, 2026-07-17. L1b/I1b/L2 policy work was removed after
> implementation review. This plan is retained as historical rationale; only
> the lower-level compiled-unit mechanism and I1a blocking isolate substrate
> remain shipped.
>
> Executes [48 — Concurrency](48-concurrency-reactors-and-isolates.md) and
> [49 — Live Module Evolution](49-live-module-evolution.md) after the
> [combined adversarial review](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/48a-live-evolution-isolate-combined-adversarial-review.md).
> I1a's executable contracts were accepted by the
> [plan-hardening follow-up review](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/50a-i1a-plan-hardening-follow-up-review.md).
> The implemented starting point remains
> [47 — CompiledUnit Lifecycle](../2026-10-runtime-refactors/47-compiled-unit-lifecycle.md).

## 1. Outcome and order

The work ships as five independently green vertical slices:

```text
L1a — generation authority and image coherence
→ I1a — blocking single-worker isolate substrate
→ L1b — file-backed revision and image-baseline workflow
→ I1b — single-worker rolling replacement
→ L2  — blocking mailbox-safe live rollout
```

Each phase must leave `zig build verify` green and expose a useful boundary of its
own. A later phase may extend an earlier wire or policy surface, but it may not
retroactively require promises, a reactor, pool coordination, shared Oops, or
suspended execution.

## 2. Governing implementation rules

1. One runtime has one current `DispatchGeneration`; a `CompiledUnit` remains a
   candidate/artifact delta and provenance record.
2. Dispatch changes only while a host-owned process-local quiescence guard is
   active. Ordinary Birnam sends cannot enter that phase.
3. A generation publishes dispatch and its complete canonical source map at one
   non-failing commit seam. That map is also the portable provenance.
4. `__modsrc__`, `__methods__`, `__methodops__`, and per-name
   `activeCompiledUnit` leave the authority model in L1a. Image compilation,
   rebuild, inspection, and current-source queries read only the generation.
5. Isolate messages always pass through wire-v1 encode/decode. Multiple handles
   may exist, but each handle is single-flight and blocking through L2.
6. Worker PIDs, descriptors, pending requests, native handles, and rollback
   capabilities are process-local and never become durable Store values.
7. Every externally blocking process operation has a caller-visible deadline.
   Timeout makes a worker terminal. Cleanup either reaps it once within the
   escalation budget or leaves one process-local `#reapPending` owner for later
   nonblocking host-boundary drains; no caller waits without a bound.
8. Full-image save by temporary-file replacement is the only supported generation
   baseline. Append is outside the generation-image contract.
9. The reactor remains orthogonal. Nothing in these phases introduces an API whose
   implementation secretly needs a reactor or foreign thread.

## 3. Representation choices fixed by this plan

These choices close the representation questions left open in design 49.

### 3.1 Runtime generation and current cell

Add a distinct `DispatchGeneration` prototype in the existing self-hosted
`compiledUnit` module. Store the current object in the single-valued cell:

```text
entity #dispatch, attribute #activeGeneration, value DispatchGeneration
```

`CompiledUnit activeFor:` may remain as a convenience query, but it derives its
answer from the current generation. There are no per-name authority mirrors.

A generation records immutable values for:

- sealed source map;
- canonical base digest;
- predecessor, when process-local rollback remains possible;
- delta `CompiledUnit`, if this generation came from one candidate; and
- state/provenance needed for inspection and image recovery.

### 3.2 Universal source revisions and sealed source maps

Every module uses the same revision model. There is no permanent foundation/live,
system/application, or file/image distinction. Represent a `SourceRevision` as an
immutable Vector record rather than a mutable entity:

```text
#[#sourceRevision module dependencies authoredSource nonMethodDigest methodDigest revisionDigest]
```

Represent the complete source map as a sorted Vector of immutable entries:

```text
#[ #[moduleA revisionA] #[moduleB revisionB] ... ]
```

`authoredSource` is always the complete document. Codegen derives, without
rewriting it, a non-method projection and a declarative method manifest. Through
L2 every existing module may replace any declarative method, but its canonical
identity, dependency declaration, and non-method projection must remain unchanged.
This is a temporary evolution-policy restriction, not a second class of module.
Later initialization/migration work removes it for top-level-effect changes and
module introduction.

No ordinary API returns a mutable cell for either revision or map. Codegen
rehashes the records before prepare, commit, save, and boot publication; a raw
mutation or inconsistent durable record produces an error rather than new
authority.

The complete current source map is sufficient portable dispatch provenance.
Image compilation and rebuild execute each current authored document exactly once,
whole and in dependency order. L1a therefore retires `__methods__` and
`__methodops__` as recovery inputs rather than manufacturing a second ordered
operation history from the same source.

### 3.3 File metadata

L1b exports an ordinary parseable `.birnam` file whose first line is an evolution
envelope comment:

```text
// birnam-live-base sha256:<64 lowercase hexadecimal digits>
```

The remaining bytes are the exact authored source. Import recognizes and removes
only that first metadata line before hashing/compilation. `fileOut` preserves every
authored byte after it. Missing, malformed, stale, or sibling bases are distinct
results; forced replacement is an explicit later policy, not an implicit fallback.

### 3.4 No compatibility authority

Backward compatibility is not a goal. `activeCompiledUnit`, `__modsrc__`,
`__methods__`, `__methodops__`, and pre-generation image formats do not participate
in the shipped contract. They may exist transiently while the implementation is
being replaced, but Phase L1a cannot exit while a production reader or writer
depends on them. Documentation and tooling move directly to the generation.

Ordinary `System load:` remains script evaluation, not a route into module
authority. Once the initial generation is published, supported dispatch mutation
requires an evolution intent and the host quiescence guard. Raw floor mutation is
an explicitly unsupported corruption escape and makes reconciliation/save fail.

### 3.5 Initial isolate defaults

Use overridable conservative defaults:

- boot ready: 10 seconds;
- ordinary request: 30 seconds;
- graceful stop acknowledgement: 2 seconds;
- TERM grace: 2 seconds;
- KILL-to-reap: 2 seconds.

The first worker inherits stdout and stderr; protocol traffic uses only the
dedicated binary descriptor. I1a supports the three fixed substrate error records
from design 48. Applications use scalar/vector/map payloads initially; a public
record-schema registry is deferred until a real application requires it.

A supplied request timeout is the service budget and begins before local encoding.
If it expires before any frame byte is written, the request answers `#timeout` and
the worker remains ready. After the first frame byte, expiry is terminal and may
add at most the configured graceful/TERM/KILL-to-reap cleanup budgets to the
caller-visible duration. The API documents that total upper bound rather than
pretending cleanup consumes time remaining on an already-expired service budget.
The boot-ready timeout uses the same rule: boot work receives its service budget,
then terminal cleanup receives the separate bounded escalation budget.

### 3.6 Recovery-only worker boot

Extract image recovery from entry dispatch before adding worker mode. A
`recoverImageRuntime`-shaped helper loads the native stack, restores or rebuilds
the image, re-derives globals, refreshes module paths, validates the current
generation, and returns without invoking global `main` or the REPL. Ordinary
`runImage` calls that helper and then applies the existing entry convention.

The outer ordinary-image or worker runner, not the recovery helper, owns the live
GC stack anchor for the whole runtime turn. Worker boot performs this exact order:

1. parse worker arguments before baked-image and ordinary image routing;
2. establish the caller-owned GC stack anchor and load/recover the explicit image;
3. require and validate its current `DispatchGeneration`;
4. resolve one named global receiver and a selector of arity one;
5. resolve the isolate driver and Codegen host-phase exports;
6. enter `#running` exactly once; and
7. send `ready` with request id zero, then enter the substrate-owned loop.

Any failure before step 7 attempts one `bootError` with request id zero, closes the
channel, and exits nonzero. No worker path calls `main`, constructs a REPL, or
leaves the Codegen phase at `#boot` after reporting ready.

### 3.7 Public isolate outcomes

Successful `start` answers the ready opaque handle, `request:timeout:` answers the
decoded reply value, and `stop` (including repeated stop after a reaped exit)
answers its handle. A stop that exhausts its reap budget answers an `#exit` failure
with kind `#reapPending`; subsequent stop is idempotent, performs one nonblocking
drain attempt, and answers the same failure until the reap succeeds. Every failure
answers an `IsolateFailure`, a value-like error object with read-only queries:

```text
class       #boot | #application | #protocol | #encode | #timeout |
            #transport | #exit | #busy | #stopped
kind        the remote or local reason Symbol
message     diagnostic String
exitStatus  Integer or nil
requestId   Integer or nil
```

`IsolateFailure` answers `isError` true and carries no worker handle, PID,
descriptor, prepared capability, or mutable registry reference. The wire
`bootError`, `replyError`, and `protocolError` records supply `kind` and `message`;
the coordinator assigns their outer classes. Local failures use the same object,
so callers can distinguish every failure class without parsing text or consulting
ambient `lastError` state.

Birnam errors are values: if a handler result answers `isError` true, the child
encodes it as `replyError`. An application that intends to return error-shaped
data must wrap its fields in an ordinary transferable map or vector. Encode
failure occurs before a request frame is written and leaves a ready worker usable;
a timeout or any failure after a request may have been sent is terminal.

### 3.8 Process-local handle and reap ownership

Each collectable handle stores only a monotonically increasing nonzero `u64`
registry token. Tokens are never reused during a process lifetime; exhaustion
refuses another start. Registry entries retain native bytes and process state, not
Oops. An exited entry remains as a tombstone while its handle is reachable, so
repeated `stop` is idempotent and a stale handle can never address a newer worker.

Worker handles are explicitly non-durable. Full-image preflight rejects a reachable
handle in durable Store state with `#nonDurableIsolateHandle`; it never omits the
binding silently and never serializes the token, PID, or descriptor.

Termination closes the channel, waits through the graceful and TERM budgets,
sends KILL if necessary, and polls `waitpid(..., WNOHANG)` through the final reap
budget. A successful `waitpid` is recorded once as `#exited`. If the child is not
yet waitable, the entry becomes terminal `#reapPending`; the caller returns on
time and later host-boundary drains retry nonblocking `waitpid` until the sole
owner records the reap. No API can reuse a `#reapPending` worker.

The isolate native module exports nonallocating `drain` and bounded `shutdown`
hooks. Runtime host frames call `drain` before and after isolate operations and at
top-level REPL boundaries, and install `shutdown` as a defer after the isolate
module loads. GC finalization only enqueues the token. Shutdown performs the same
bounded escalation and reports any still-pending PID rather than blocking without
a deadline. Ordinary `drain` never sleeps or polls with a positive timeout: it
closes newly orphaned channels, sends any signal whose monotonic escalation time
has arrived, performs `waitpid(..., WNOHANG)`, and returns. Repeated boundaries
advance pending cleanup without adding latency to unrelated top-level turns.

### 3.9 Spawn, descriptor, environment, and request-id contract

The parent resolves absolute executable and image paths before spawn. A platform
adapter creates one full-duplex local stream channel with close-on-exec set on both
original descriptors; spawn file actions duplicate only the child endpoint to the
fixed protocol descriptor and close both originals. The implementation must use a
close-from/CLOEXEC-default facility where available, with an enumerated-close
fallback covered by descriptor-leak tests. Both endpoints use a no-SIGPIPE write
operation (`SO_NOSIGPIPE` on Darwin or `MSG_NOSIGNAL` where available).

I1a is a trusted-worker API and inherits the parent environment and working
directory unchanged. Only descriptors are capability-filtered. Environment
filtering, per-worker overrides, captured output, changed credentials, and hostile
sandboxing remain later policy. Stdout and stderr are inherited; protocol bytes
use only the dedicated descriptor.

Wire request ids start at one and increase monotonically through `maxInt(i64)` so
they remain representable by Birnam `Integer` diagnostics. `request`, `stop`, and
later request/reply controls use a nonzero id; their response must echo it.
`ready` and `bootError` use zero. `protocolError` echoes an offending id only when
the header was trustworthy, otherwise zero. Attempting to advance past
`maxInt(i64)` makes the handle terminal with `#requestIdExhausted`; ids never wrap.
The encoder enforces the same frame, nesting, container, node, and string limits
as the decoder while building into a bounded buffer, so an oversized local value
fails before any frame byte is written.

## 4. Implemented L1a baseline

- Every shipped Birnam module declares one canonical identity; source and AOT boot
  stage the same exact document and `test-source-fallback` compares their G0 digest.
- `DispatchGeneration` owns the one current cell, sealed source map, predecessor,
  and canonical digest. `CompiledUnit` is candidate delta/provenance only.
- The host owns the REPL turn loop and native quiescence guard. Activation and
  rollback return `EvolutionIntent` values fulfilled only at that safe point.
- Store rejects ordinary method mutation while running. Post-G0 AOT module
  introduction is explicitly unavailable through L2; all existing modules use the
  same method-only successor path.
- Image format v4 carries one authority record and one code unit bound to it. Full
  replacement is the sole write mode; mapped and rebuilt paths consume the same
  dependency-ordered complete documents.
- Documentation and image/source inspection derive from the generation, not
  `activeCompiledUnit`, `__modsrc__`, `__methods__`, or `__methodops__`.
- `modules/process.birnam` still uses shell/text `popen`; I1a adds a separate
  binary isolate substrate rather than changing that API.

## 5. Delivery discipline

For every phase:

1. begin from a green `zig build verify`;
2. add characterization tests before changing an implemented authority boundary;
3. keep new native surfaces narrow and put lifecycle/coordinator policy in Birnam;
4. land commit-sized increments in the order listed below;
5. run the narrow test step after every increment and the complete gate at phase
   exit;
6. update the user guide only for APIs that actually shipped in that phase; and
7. do not start the next phase while an exit criterion is red or qualified away.

## 6. L1a — generation authority and image coherence

### 6.1 Goal

Boot directly into one runtime-wide generation, enforce quiescent commit/rollback,
make every existing module method-live-editable, and make mapped and rebuilt full
images reproduce the same complete source map and base digest.

L1a has no file import UI, worker, mailbox, reactor, promise, migration hook, or
initializer transaction.

### 6.2 Implemented increments

#### L1a.0 — Define the new boot and source contract — complete

- Inventory every shipped Birnam module used by normal boot, tooling, tests, and
  image recovery. Give each durable module one canonical
  `Module named:requires:` declaration. Test programs remain scripts.
- Make source fallback and AOT compilation validate that declaration and use it as
  identity. Paths are diagnostic origins only. Remove basename authority rather
  than migrating it.
- Add a full-document Codegen projection that returns canonical identity,
  dependencies, exact source digest, non-method digest, and the declarative method
  manifest without evaluating the document or changing its bytes.
- Define a method-only successor as equal identity, dependencies, and non-method
  digest with any valid declarative-method difference. Reject duplicate bindings,
  indirect/unstable targets, malformed declarations, and ambiguous documents.
- Characterize current candidate activation, rollback, image source recovery, and
  source/AOT fallback before changing their authority.

Narrow gates: `zig build test-codegen`, `zig build test-birnam`, and a new
`zig build test-source-fallback`.

#### L1a.1 — Add immutable revisions, generations, and canonical hashing — complete

- Add `SourceRevision` constructors/queries and `DispatchGeneration` policy to
  `src/id/compiled-unit.birnam`.
- Add Codegen helpers that validate the Vector record shapes and compute the exact
  domain-separated SHA-256 encodings from design 49, simplified so the complete
  canonical source map is the portable generation input.
- Build the initial complete source map from the boot manifest and publish `G0`
  once, after all boot modules are installed and before user entry. There is no
  legacy-authority state and no callable migration operation.
- Reject duplicate modules, mismatched revision digests, malformed dependency
  vectors, missing dependencies, cycles, and source/manifest disagreement before
  publishing `G0`.
- Add golden digest vectors shared by mapped/rebuilt tests.

Narrow gates: `zig build test-codegen` and `zig build test-birnam`.

#### L1a.2 — Introduce an enforceable host quiescence boundary — complete

- Add a process-local phase enum and host-only begin/end exports in
  `src/id/codegen.zig`.
- Retain the Codegen dylib handle in a small `HostRuntime` record in
  `src/id/main.zig` and resolve those exports once when the stack loads. Do not add
  an IdApi field or a Birnam-callable method that can enter the phase.
- Make every public prepared commit and rollback reject unless the phase is
  `#quiescentEvolution`; preparation and inspection remain callable while running.
- Make begin/end reject nesting, restore `#running` on all error paths, and expose
  read-only diagnostic phase state only in test builds.
- Change `src/id/main.zig` to drive existing `Repl consoleInit`, repeated
  `consoleStep`, and `consoleDone` sends itself. This creates a real between-form
  host boundary while retaining the Birnam reader/evaluator.
- Retain `[Repl console]` only as a compatibility helper, not the host's normal
  path. Add golden coverage for long sessions, EOF, faults, config bring-up, and
  teardown so the refactor changes no console behavior.
- Make ordinary supported method installation/removal reject while `#running`;
  candidate-private construction remains inert and boot/recovery have explicit
  native phases. The existing post-mutation observer is not sufficient authority.
- Add tests proving an ordinary method, top-level evaluation, raw public MOP, and
  nested guard cannot publish dispatch, while a host-boundary helper can.

Narrow gates: `zig build test-main`, `zig build test-codegen`, and
`zig build test-repl`.

#### L1a.3 — Publish whole-runtime successors and exact predecessors — complete

- Compile a candidate from a complete successor document, but materialize only its
  declarative method delta. Revalidate that its non-method projection and
  dependencies still match the active revision.
- Extend the candidate publication plan to carry expected current generation,
  replacement `SourceRevision`, complete successor source map, and successor base
  digest.
- Revalidate the expected generation object and digest immediately before native
  mutation.
- Apply the already-prevalidated binding delta, publish the successor generation,
  and refresh compatibility mirrors in the existing non-failing commit tail.
- Make rollback require the successor to remain current, validate post-bindings,
  apply the inverse, and republish the exact predecessor generation.
- Preserve `CompiledUnit` objects as delta/provenance records; do not make their
  per-name state the current-runtime authority.
- Make `activate`/`rollback` produce value-like `EvolutionIntent`s. The host
  recognizes them only after top-level evaluation returns, enters the guard,
  commits synchronously, exits the guard, and reports the result.

Narrow gates: `zig build test-codegen` and `zig build test-birnam`.

#### L1a.4 — Move image compilation and rebuild to generation authority — complete

- Replace `compileImageCode:` and rebuild enumeration with one dependency-ordered
  walk of the current generation's complete authored revisions. Execute each
  document whole and exactly once; preserve its internal statement order.
- Introduce a new image format with an authority record before executable code.
  Bind the code artifact to the generation digest and validate both before install.
- Do not read old image formats and do not fall back to legacy facts.
- Before full save, reconcile dispatch, rehash the generation, and reject drift or
  partial authority without replacing the prior image.
- Remove append from the supported generation-image path. L1a provides full
  temporary-file replacement only; power-loss durability remains a separate claim.

Narrow gates: `zig build test-image` and `zig build test-codegen`.

#### L1a.5 — Reconstruct one generation on mapped and rebuilt boot — complete

- Split image recovery from user entry in `src/id/main.zig` so recovery can finish
  and publish authority before `main`, REPL, or later worker entry.
- On mapped boot, validate the saved map/digest and restore that exact generation;
  do not mint a second composed owner.
- On rebuilt boot, compile/replay the same complete ordered revisions, then assert
  the identical base digest.
- Reject an image whose map, digest, or artifact binding cannot be validated.
- Cover a second activation after boot, then another full save/boot, to prove the
  successor contains untouched modules and does not depend on an obsolete shared
  base.

Narrow gate: `zig build test-image`.

#### L1a.6 — Phase exit and documentation — complete

- Update `docs/guide/05-compiled-units.md` from the per-name authority API to the
  shipped guarded generation semantics.
- Update `docs/guide/04-images.md` with generation-aware full-save semantics and
  the removal of append from the supported format.
- Mark the L1a task complete only after the design 49 §13.1 matrix is represented
  by named tests.
- Run `zig build test-source-fallback` and `zig build verify`.

### 6.3 L1a exit criteria

- Exactly one current generation cell exists after cutover or boot.
- Ordinary code cannot commit or rollback in a running turn.
- Every existing module can activate a method-only successor; a changed
  non-method projection is rejected without mutation.
- Stale expected base and malformed revisions leave dispatch/source authority
  unchanged.
- `compileImageCode:` and rebuild consume complete generation revisions, not
  `__modsrc__`, `__methods__`, or `__methodops__`.
- Mapped and rebuilt boots produce the same canonical digest.
- Full save uses temporary-file replacement and preserves the prior image on
  reported failure; append is outside the supported generation path.
- `zig build verify` is green.

## 7. I1a — blocking single-worker isolate substrate

### 7.1 Goal

Boot a clean child from an explicit image, exchange bounded copied values through
one blocking request per handle, distinguish all failure classes, and close the
channel plus retain the sole reap owner on every terminal path. Several independent
handles may coexist, but there is no pool or replacement policy yet.

### 7.2 Implemented increments

#### I1a.0 — Extract and characterize the worker-safe host seams

- Add `src/id/isolate.zig`, `src/id/isolate_test.zig`, and the isolated
  `test-isolate` build target with a replaceable fake process adapter; do not add
  it to the production load stack yet.
- Add `src/id/isolate.birnam` plus focused self-hosted tests for the facade and
  `IsolateFailure`; include that source in the narrow test boot set before its test
  G0 is sealed until I1a.1 makes it part of the production G0.
- Extract recovery-only image boot as fixed in §3.6; keep the GC stack anchor in
  each outer host runner and make ordinary image behavior a characterized caller
  of the new seam.
- Add worker-argument tests proving parsing precedes baked/ordinary image routing,
  and recovery tests proving neither `main` nor the REPL runs.
- Define `IsolateFailure` and lock the class/kind/message/status/request-id matrix
  with self-hosted tests before transport code returns it.
- Add the native token/reap state machine with fake-process tests for ready,
  stopping, exited, reap-pending, repeated stop, orphan enqueue, token exhaustion,
  and exactly-once successful `waitpid` recording.
- Add host `drain`/`shutdown` resolution and defer wiring against an empty registry
  before a real child can exist.

Narrow gates: `zig build test-main`, `zig build test-birnam`, and
`zig build test-isolate` with a fake process adapter.

#### I1a.1 — Publish the isolate mechanism and facade into G0

- Register the I1a.0 native mechanism as `libbirnam_id_isolate` in the normal
  runtime load stack.
- Embed the I1a.0 Birnam facade and include it in the dependency-ordered stage-1
  self-hosted set before G0 is sealed. Update the source/AOT fallback manifest and
  assert identical G0 digests with the isolate facade present.
- Keep `modules/process.birnam` unchanged and explicitly separate.
- Add a test-only worker fixture module with a named `handle:` receiver to the
  fixture boot manifest before its G0 is sealed, then save that image. Never file
  the handler in after G0 or add a fixture-only module-introduction escape hatch.

#### I1a.2 — Implement wire-v1 frame and value codecs

- Implement the exact 20-byte header, kind values, tag table, network byte order,
  request-id/direction rules, scalar map-key restriction, duplicate-key rejection,
  and fixed error schemas from design 48.
- Enforce frame, nesting, container, decoded-node, and string/resource limits while
  encoding and before decoder allocation; use a bounded encoder buffer.
- Loop on partial reads/writes and EINTR; distinguish clean EOF, truncated header,
  truncated payload, unknown tag/version/kind, and trailing bytes.
- Add table-driven round trips, every malformed case, f64 bit patterns, embedded
  NUL strings, symbol spelling, request-id/state-direction mismatches, and
  cyclic/non-transferable/oversized encode rejection.
- Fuzz the decoder with bounded random bytes and assert no unbounded allocation or
  crash.

Narrow gate: `zig build test-isolate`.

#### I1a.3 — Add worker boot mode without normal entry

- Add `--worker IMAGE RECEIVER SELECTOR --channel-fd N` parsing in
  `src/id/main.zig` before baked-image and ordinary image routing.
- Use §3.6's recovery-only function under a worker-owned live stack anchor.
- Require a current generation, resolve a named receiver and arity-one selector,
  resolve the driver/phase hooks, enter `#running`, and only then send zero-id
  `ready`. Every earlier error is a zero-id `bootError` when the channel is usable.
- Make the substrate own the frame loop and invoke the selector once per ordinary
  request. Applications receive decoded values, never envelopes or stop/control
  frames.
- Export one narrow worker-driver entry from the isolate module. `main.zig` passes
  it the IdApi, channel, resolved entry, and the already-resolved Codegen phase
  hooks; this keeps codec/process mechanics in `isolate.zig` while preserving
  host-only authority for later L2 controls.
- Encode handler-returned Birnam errors as `replyError`; boot/protocol/transport
  errors map to the distinct `IsolateFailure` classes fixed in §3.7.

Narrow gates: `zig build test-main` and `zig build test-isolate`.

#### I1a.4 — Implement spawn, handle ownership, and ready handshake

- Use `posix_spawn`, `socketpair` (or two pipes if required), file actions, and a
  dedicated inherited descriptor. Never run Birnam in a post-fork parent copy.
- Implement §3.9's absolute-path, descriptor-closing, inherited-environment, and
  no-SIGPIPE contract behind a replaceable process adapter.
- Add an opaque token handle backed by a native registry entry containing PID,
  channel, lifecycle/reap state, request id, and owned byte diagnostics. Keep Oops
  out of the registry and reject durable handle reachability during image save.
- Permit several registry entries/handles, but one active request per entry.
- Wait for `ready` under the boot deadline; EOF or error before ready is boot
  failure followed by terminal bounded cleanup or `#reapPending` ownership.
- Test inherited stdout/stderr and environment, absence of unrelated descriptors,
  relative-to-absolute image resolution, peer-close writes without SIGPIPE, and
  stage-1/source-fallback parity.
- Add image preflight coverage proving a durable Store path to a handle reports
  `#nonDurableIsolateHandle`, while an unreachable or stack-local handle does not
  contaminate an otherwise valid save.

Narrow gates: `zig build test-isolate`, `zig build test-image`, and
`zig build test-source-fallback`.

#### I1a.5 — Implement deadline-bearing request and stop

- Expose `request:timeout:` and a `request:` shorthand using the configured default.
- Start the service deadline before encoding, check it during bounded traversal,
  pass only the remaining service budget to write/read, match the reply request
  id, and reject stale or unsolicited frames. A pre-write expiry is nonterminal;
  a post-write expiry starts the separate bounded cleanup budget fixed in §3.5.
- Reject an unencodable/oversized value before writing and keep the worker ready.
  After any frame byte is written, timeout or uncertain transport is terminal.
- Make a post-write request timeout terminal: close, TERM, KILL if required, then
  answer a `#timeout` failure after either one reap or transition to
  `#reapPending`.
- Implement `stop` as stop/stopped-or-EOF/waitpid with the same bounded escalation
  and request-id echo rules.
- Make stop after exit/reap idempotent.
- Preserve application failures separately from encode, protocol, timeout,
  transport, busy/stopped, and child-exit results through `IsolateFailure`.

#### I1a.6 — Make orphan and process-exit cleanup honest

- Attach a non-allocating handle finalizer that only marks/enqueues the native
  registry token.
- Drain orphan cleanup at top-level host boundaries, before/after isolate calls,
  and through the installed bounded orderly-runtime-exit defer; never block inside
  GC sweep and never call Birnam from a finalizer or shutdown hook.
- Keep the registry Oop-free. Test collection during a request and after dropping
  ready, exited, and reap-pending handles.
- Test drop-without-stop, crash, protocol failure, timeout, repeated stop, parent
  shutdown, delayed reap, and simultaneous independent handles for descriptor and
  zombie leaks. A shutdown that exhausts its final reap budget reports the PID and
  returns; it does not make an unbounded wait promise.

#### I1a.7 — Integration, measurement, and phase exit

- Add cross-process tests for ready, scalar/vector/map round trips, application
  error, crash, malformed frame, timeout, clean stop, and two independent handles.
- Record startup latency, round-trip latency by payload size, and steady-state RSS
  as non-gating benchmark output.
- Add a user guide for the shipped blocking isolate API and trusted-worker model.
- Run `zig build verify` and a repeated process test that checks no unreaped
  children remain.

### 7.3 I1a exit criteria

- Worker boot never runs image `main` or REPL.
- Worker ready implies a validated current generation and runtime phase `#running`.
- The substrate, not the application, owns the receive loop.
- Wire-v1 malformed input is bounded and terminal without corrupting the parent.
- Every terminal path closes descriptors within a bound; each child has one native
  reap owner, and each successful reap is recorded exactly once. Repeated drains
  leave no zombies in the process tests.
- Multiple handles coexist; every handle remains single-flight.
- Handles are token-stable, non-durable, and stale handles cannot address a newer
  worker.
- Every public failure class is distinguishable without parsing message text.
- No promise, callback, reactor, pool, or candidate-delivery API exists.
- `zig build verify` is green.

### 7.4 Implementation status

Shipped 2026-07-14. The runtime now loads `libbirnam_id_isolate`, stages the
`#isolate` facade in G0, routes `--worker` before baked/ordinary image entry,
recovers without invoking `main` or the REPL, enters `#running`, and hands the
dedicated descriptor to the substrate-owned loop. Wire v1, structured failures,
opaque token handles, service deadlines, bounded stop/termination, orphan drains,
shutdown cleanup, and explicit non-durable image rejection are implemented.
`test-isolate-process` exercises real worker image boot, copied replies,
application errors, terminal timeout cleanup, two independent handles, and clean
stop/reap; it is part of `zig build verify`.

## 8. L1b — file-backed revision and image-baseline workflow

### 8.1 Goal

Make the L1a generation lifecycle usable from ordinary source files: exact export,
candidate-safe import, divergence detection, staged inspection, host-safe
activation, and full saved baselines suitable for I1b.

### 8.2 Implemented increments

#### L1b.0 — Add project policy module and revision API

- Add `src/id/live-modules.birnam`, loaded after `compiledUnit`, with the
  `LiveModules` facade.
- Implement queries for current generation, module revision, source text, base
  digest, staged candidates, and durability status.
- Keep parsing, candidate compilation, Store MOP, and image encoding delegated to
  existing owners.

#### L1b.1 — Implement exact fileOut and import

- Implement `[LiveModules fileOut: module to: path]` with the one-line base
  envelope and exact authored bytes.
- Implement `[LiveModules import: path]` as an inert operation: parse envelope,
  parse canonical `Module named:requires:`, validate name/base, compute revision,
  and return a staged revision/candidate or kinded error.
- Distinguish identical import, current-base successor, missing base, stale ancestor,
  sibling divergence, malformed envelope, wrong module name, unsupported top-level
  form, dependency error, and artifact mismatch.
- Never update current source or `__modsrc__` while importing/preparing.

#### L1b.2 — Compile, inspect, and retain staged candidates

- Compile candidate-safe source through the existing candidate ABI into a fresh
  temporary output, then publish it to the existing checksum-addressed content
  path.
- Expose additions/replacements/removals, dependency changes, expected/current
  digest, mapping state, and rollback availability before activation.
- Provide explicit dependency preparation and mapping; preserve the original
  `Module load:` error.
- Clean abandoned temporary artifacts and transient staged records without
  unloading mapped dylibs.

#### L1b.3 — Add a host-boundary activation intent

- Extend the host-driven console step protocol with a single returned evolution
  intent for `:live stage PATH`, `:live activate PATH`, `:live rollback`, and
  `:live save PATH`.
- The host receives the intent only after `consoleStep` returns, enters L1a's
  quiescence guard, performs the synchronous operation, exits the guard, then
  prints the result before the next form.
- Do not add a queue, callback, future, or background watcher. An ordinary send to
  an activation method outside this host path still answers `#quiescenceRequired`.
- Keep staged inspection callable normally because it is inert.

#### L1b.4 — Produce and verify saved worker baselines

- Add `[LiveModules saveBaseline:]` as policy over full `saveImage:` plus explicit
  generation/durability reporting.
- Refuse append as a live-generation durability action; retain data-only append
  only under L1a's equal-digest gate.
- Boot each produced baseline normally and rebuilt, verify identical base/source
  digests, and run an application-defined smoke request/function.
- Add fixtures for two edits, rollback, save, boot, edit again, and save again.

#### L1b.5 — Documentation and phase exit

- Add a live-modules user guide with file envelope, stage/activate/rollback/save,
  divergence, and explicit exclusions.
- Add self-hosted tests to `modules/run-tests.birnam` and cross-process image tests.
- Run design 49 §13.2 as a named matrix and finish with `zig build verify`.

### 8.3 L1b exit criteria

- `fileOut` plus no edits imports idempotently and preserves authored bytes.
- Invalid/stale/divergent files never change current authority or mirrors.
- Activation occurs only after the host regains a top-level boundary.
- A full saved baseline boots mapped and rebuilt with identical digest/behavior.
- No initializer, migration, file watcher, source-aware `adoptLive`, or async API
  is introduced.
- `zig build verify` is green.

## 9. I1b — single-worker rolling replacement

### 9.1 Goal

Prove an idle selected worker A can be replaced by a healthy worker B built from an
L1b baseline, while every B failure leaves A usable. This is one blocking handoff,
not a pool.

### 9.2 Implemented increments

#### I1b.0 — Add the Birnam replacement policy

- Add policy to `src/id/isolate.birnam`; keep PID/channel/timeout mechanics in
  `src/id/isolate.zig`.
- Expose one operation that accepts idle A, B's image/entry, a health request, and
  explicit deadlines, and answers B or a kinded replacement error.
- Require A to be ready with no request in flight before starting B.

#### I1b.1 — Implement the A-to-B handoff

1. retain A as the caller's selected handle;
2. start B and wait for transport ready;
3. send exactly one application health `request:timeout:`;
4. on any B failure, terminate/reap B and answer an error containing A unchanged;
5. on health success, return/select B between calls; and
6. stop/reap idle A before declaring replacement complete. If A becomes
   `#reapPending`, keep B selected and answer a partial-cleanup result naming B as
   current and A as the sole pending reap; never claim A remained selected or
   switch traffic back to a worker whose termination began.

The health request may have external effects; failure handling never claims to
reverse them.

#### I1b.2 — Prove every failure branch

- B image missing/corrupt;
- receiver or selector missing;
- ready timeout;
- application health error;
- malformed health reply;
- health timeout/crash;
- B succeeds but A stop needs TERM/KILL; and
- B is selected but A exhausts the reap budget, then a later drain reaps A; and
- repeated successful replacements A→B→C.

For every pre-selection B failure, send another ordinary request to A and assert it
still answers. For the post-selection cleanup case, assert B answers and A cannot
be reused. Assert one reap owner and one recorded successful reap for both handles,
with no hidden pool state.

The shipped I1b acceptance matrix is named and executable:

| Contract | Evidence |
| --- | --- |
| missing/corrupt image, missing receiver/selector, ready timeout | `scripts/isolate-replacement-smoke.birnam` |
| application error, malformed reply, health timeout/crash; A answers after each | `scripts/isolate-replacement-smoke.birnam` |
| distinct L1b generation digests/behavior and repeated A→B→C | `scripts/isolate-replacement-{v1,v2}.birnam`, `scripts/isolate-replacement-smoke.birnam` |
| forced TERM/KILL, B remains selected, A is sole `#reapPending` owner, later drain and repeated stop | `scripts/isolate-replacement-cleanup-smoke.birnam` with the environment-only native fault fixture |
| user recipe remains runnable | `examples/isolates/replacement.birnam`, `zig build test-live-feature-examples` |

`zig build test-isolate-replacement` runs the matrix. The fault fixture changes
no public selector or result shape and is enabled only by
`BIRNAM_TEST_ISOLATE_FAULTS` in the process acceptance script.

#### I1b.3 — Baseline integration and phase exit

- Generate B through the L1b full-baseline API rather than a test-only image path.
- Verify A and B can have different generation digests and behavior.
- Document the blocking replacement recipe, trusted health check, and absence of
  busy draining/concurrent routing.
- Run `zig build verify` plus the repeated replacement leak test.

### 9.3 I1b exit criteria

- A remains selected/usable on every B startup or health failure.
- Selection changes only between blocking calls.
- Successful handoff ends with B ready and A reaped.
- A post-selection reap-pending outcome keeps B selected, exposes A's cleanup
  status, and becomes complete only after a later drain records A's reap.
- No pool, retry loop, concurrent route, busy drain, or reversible-health claim is
  present.
- `zig build verify` is green.

## 10. L2 — blocking mailbox-safe live rollout

### 10.1 Goal

Deliver one live-module candidate to one stateful worker, compare a value-like base
digest, prepare and commit entirely inside that worker between ordinary requests,
and return an independent receipt. Rollback is another blocking local control.

### 10.2 Implemented increments

#### L2.0 — Extend wire-v1 with fixed evolution controls

- Enable frame kinds 9/10 and the exact activate/rollback/receipt schemas from
  design 49.
- Keep all fields within the existing transferable codec; reject Oops, local
  generation objects, prepared tokens, pointers, handles, and rollback snapshots.
- Validate lowercase SHA-256 text, operation symbols, optional source, and
  checksum-addressed artifact location before mapping.
- Add codec/golden tests before enabling child dispatch.

#### L2.1 — Intercept controls in the substrate-owned child loop

- Recognize evolution controls only while no application handler is running.
- Resolve the worker-local `LiveModules` facade; answer
  `#evolutionUnavailable` if the image does not contain it.
- Compare expected base digest with the worker's current generation before any
  compile/map/dispatch effect.
- Prepare and validate the revision/artifact locally. The parent never sends a
  prepared capability.
- Enter the same Codegen quiescence guard used by L1a, commit synchronously, exit
  the guard, encode the receipt, then read the next frame.

#### L2.2 — Add parent blocking rollout and receipt APIs

- Expose `evolve:timeout:` on a worker handle using the same terminal timeout
  rules as ordinary requests.
- Return a value-like receipt containing operation, module, accepted base,
  prior/current digest, outcome, error kind, and message.
- Preserve application, evolution-policy, protocol, timeout, transport, and exit
  errors as distinguishable results.
- Do not retry, fan out, or infer success from transport ready.

#### L2.3 — Implement local rollback by expected digest

- Retain only the worker-local process capability created by its latest accepted
  generation.
- A rollback control names the expected current digest, not a token or local
  generation number.
- Between requests, verify current digest and binding post-state, enter the guard,
  apply the inverse, republish the predecessor, and return a receipt.
- Reject stale digest, intervening generation, drift, lost capability after boot,
  and second rollback without changing dispatch.

#### L2.4 — End-to-end behavior and independence tests

- Ordinary request under G1; activate G2; next ordinary request observes G2.
- A request already executing cannot be interrupted by a control frame.
- Wrong base, invalid source, missing artifact, checksum mismatch, map failure, and
  commit drift leave G1 current.
- Rollback G2 restores G1 for the next request.
- Two workers may independently accept/reject and retain different digests; no
  test or API claims cross-worker atomicity.
- Save/boot after accepted rollout uses the worker's current generation, while the
  old process-local rollback capability does not survive.

The shipped L2 acceptance matrix is named and executable:

| Contract | Evidence |
| --- | --- |
| fixed activate/rollback/receipt schemas, canonical digests/content addresses, malformed records | `src/id/isolate_test.zig` |
| a queued evolution frame cannot interrupt an executing request; malformed frame is terminal protocol input | `src/id/isolate_test.zig` worker-loop socket test |
| zero budget, wrong base, invalid source, missing artifact, checksum mismatch, map/ABI failure leave G1 | `scripts/isolate-rollout-smoke.sh` |
| prepared commit rejects binding drift without partial publication | `src/id/codegen_test.zig` commit-time drift test |
| G1→G2, copied receipt fields, independent worker, rollback/second rollback, G2→G3 intervening digest | `scripts/isolate-rollout-smoke.sh` |
| accepted save/boot preserves G2 but loses the old rollback capability | `scripts/isolate-rollout-smoke.sh` |
| user rollout and rollback recipe remains runnable | `examples/live-modules/rollout.input`, `zig build test-live-feature-examples` |

`zig build test-isolate-rollout` runs the real-process rows; `zig build test`
runs the codec, mailbox-order, candidate integrity, quiescence, and commit-drift
rows. The complete composition is `zig build verify`.

#### L2.5 — Documentation and phase exit

- Extend the isolate/live-module guides with blocking rollout, receipt fields,
  local rollback lifetime, and partial-outcome policy.
- Add design 49 §13.3 as a named integration matrix.
- Run `zig build verify`, repeated rollout/rollback process tests, and malformed
  evolution-frame tests.

### 10.3 L2 exit criteria

- Controls run only between application calls under the enforceable guard.
- The worker binds preparation to its own current generation.
- The next request sees the committed or rolled-back generation deterministically.
- Receipts are copied values and expose no process-local capability.
- Different workers may produce different correct outcomes without global claims.
- No promise, pool-wide commit, concurrent rollout, or automatic retry exists.
- `zig build verify` is green.

## 11. Cross-phase test matrix

| Contract | L1a | I1a | L1b | I1b | L2 |
| --- | :---: | :---: | :---: | :---: | :---: |
| Invalid input is inert | ✓ | ✓ | ✓ | ✓ | ✓ |
| Exact authority/base comparison | ✓ |  | ✓ |  | ✓ |
| Mapped/rebuilt image equivalence | ✓ | boot fixture | ✓ | baseline | post-rollout |
| Quiescent commit enforcement | ✓ | boundary only | ✓ |  | ✓ |
| Bounded resource/timeout behavior |  | ✓ |  | ✓ | ✓ |
| Single-owner eventual process reap |  | ✓ |  | ✓ | ✓ |
| No process-local value crosses wire/image | image | ✓ | image | ✓ | ✓ |
| Full `zig build verify` | ✓ | ✓ | ✓ | ✓ | ✓ |

## 12. Expected file ownership

| Area | Primary files |
| --- | --- |
| Generation/revision policy | `src/id/compiled-unit.birnam`, later `src/id/live-modules.birnam` |
| Candidate/digest/phase mechanism | `src/id/codegen.zig`, `src/id/codegen_test.zig` |
| Image authority and reports | `src/id/image.zig`, `src/id/image_test.zig` |
| Host safe points and worker mode | `src/id/main.zig`, `modules/repl.birnam` |
| Process/codec/handle mechanism | new `src/id/isolate.zig`, `src/id/isolate_test.zig` |
| Isolate/replacement policy | new `src/id/isolate.birnam` |
| File-backed project policy | new `src/id/live-modules.birnam` and self-hosted tests |
| Build and gates | `build.zig`, `modules/run-tests.birnam`, integration scripts |
| User documentation | `README.md`, `docs/guide/04-images.md`, `05-compiled-units.md`, new live/isolate guides |

## 13. Explicit non-goals through L2

- reactor integration or promises;
- one-way events or multiple in-flight requests per handle;
- pool abstraction, load balancing, busy draining, or automatic restart;
- global/cross-worker atomic rollout;
- initializer transactions or exactly-once initialization;
- application-state migration;
- image-native lossless source editing or source-aware `adoptLive`;
- append durability for generation changes;
- file watching;
- multi-module atomic candidates;
- dylib unloading;
- hostile-worker sandboxing; and
- Linux support before the existing platform gate changes.

## 14. Final completion condition

The five-phase body of work is complete only when all five phase exit criteria are
green, the complete verification gate passes from a clean build, mapped and rebuilt
workers reproduce canonical generation digests, repeated worker/rollout tests leave
no descriptors or zombies, and current user documentation describes shipped APIs
rather than the plan's future names.
