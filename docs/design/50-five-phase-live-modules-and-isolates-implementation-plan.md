# 50 — Five-Phase Implementation Plan: Live Modules and Isolates

> **Status:** L1a implemented; I1a next. Revised 2026-07-14.
>
> Executes [48 — Concurrency](48-concurrency-reactors-and-isolates.md) and
> [49 — Live Module Evolution](49-live-module-evolution.md) after the
> [combined adversarial review](48a-live-evolution-isolate-combined-adversarial-review.md).
> The implemented starting point remains
> [47 — CompiledUnit Lifecycle](47-compiled-unit-lifecycle.md).

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
7. Every externally blocking process operation has a deadline. Timeout makes a
   worker terminal and ends with exactly one reap.
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
- `modules/process.birnam` still uses shell/text `popen`; I1a replaces that seam
  with the binary isolate substrate.

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
one blocking request per handle, distinguish all failure classes, and close/reap
the child on every terminal path. Several independent handles may coexist, but
there is no pool or replacement policy yet.

### 7.2 Planned increments

#### I1a.0 — Add the native mechanism and test target

- Add `src/id/isolate.zig` and `src/id/isolate_test.zig`; register
  `libbirnam_id_isolate` in `build.zig` and the normal runtime load stack.
- Add `src/id/isolate.birnam` for the small Birnam-facing facade and documentation;
  include it in the stage-1 self-hosted set.
- Keep `modules/process.birnam` unchanged and explicitly separate.
- Add fixture helpers that save a worker image with a named `handle:` receiver.

#### I1a.1 — Implement wire-v1 frame and value codecs

- Implement the exact 20-byte header, kind values, tag table, network byte order,
  scalar map-key restriction, duplicate-key rejection, and fixed error schemas
  from design 48.
- Enforce frame, nesting, container, decoded-node, and string/resource limits before
  allocation.
- Loop on partial reads/writes and EINTR; distinguish clean EOF, truncated header,
  truncated payload, unknown tag/version/kind, and trailing bytes.
- Add table-driven round trips, every malformed case, f64 bit patterns, embedded
  NUL strings, symbol spelling, and cyclic/non-transferable encode rejection.
- Fuzz the decoder with bounded random bytes and assert no unbounded allocation or
  crash.

Narrow gate: `zig build test-isolate`.

#### I1a.2 — Add worker boot mode without normal entry

- Add `--worker IMAGE RECEIVER SELECTOR --channel-fd N` parsing in
  `src/id/main.zig` before ordinary image routing.
- Reuse L1a's recovery-only function; suppress global `main` and REPL.
- Resolve receiver/selector after recovery, construct the child driver, and send
  `ready` only after all three succeed.
- Make the substrate own the frame loop and invoke the selector once per ordinary
  request. Applications receive decoded values, never envelopes or stop/control
  frames.
- Export one narrow worker-driver entry from the isolate module. `main.zig` passes
  it the IdApi, channel, resolved entry, and the already-resolved Codegen phase
  hooks; this keeps codec/process mechanics in `isolate.zig` while preserving
  host-only authority for later L2 controls.
- Encode caught application errors as `replyError`; boot/protocol/transport errors
  remain distinct.

Narrow gates: `zig build test-main` and `zig build test-isolate`.

#### I1a.3 — Implement spawn, handle ownership, and ready handshake

- Use `posix_spawn`, `socketpair` (or two pipes if required), file actions, and a
  dedicated inherited descriptor. Never run Birnam in a post-fork parent copy.
- Mark unrelated descriptors close-on-exec and pass only explicitly requested
  capabilities/environment.
- Add an opaque handle backed by a native registry entry containing PID, channel,
  lifecycle state, request id, image/entry diagnostics, and reap state.
- Permit several registry entries/handles, but one active request per entry.
- Wait for `ready` under the boot deadline; EOF or error before ready is boot
  failure followed by mandatory reap.

#### I1a.4 — Implement deadline-bearing request and stop

- Expose `request:timeout:` and a `request:` shorthand using the configured default.
- Poll under a monotonic deadline; match the reply request id and reject stale or
  unsolicited frames.
- Make request timeout terminal: close, TERM, KILL if required, reap once, and
  answer a distinct timeout/exit result.
- Implement `stop` as stop/stopped-or-EOF/waitpid with the same bounded escalation.
- Make stop after exit/reap idempotent.
- Preserve application error values separately from protocol, timeout, transport,
  and child-exit results.

#### I1a.5 — Make orphan and process-exit cleanup honest

- Attach a non-allocating handle finalizer that only marks/enqueues the native
  registry entry.
- Drain orphan cleanup at top-level host boundaries, before/after isolate calls,
  and during orderly runtime exit; never block inside GC sweep.
- Ensure the registry owns no unregistered Oops. If the implementation retains
  any, register one GC root provider and test collection during a request.
- Test drop-without-stop, crash, protocol failure, timeout, repeated stop, parent
  shutdown, and simultaneous independent handles for descriptor/zombie leaks.

#### I1a.6 — Integration, measurement, and phase exit

- Add cross-process tests for ready, scalar/vector/map round trips, application
  error, crash, malformed frame, timeout, clean stop, and two independent handles.
- Record startup latency, round-trip latency by payload size, and steady-state RSS
  as non-gating benchmark output.
- Add a user guide for the shipped blocking isolate API and trusted-worker model.
- Run `zig build verify` and a repeated process test that checks no unreaped
  children remain.

### 7.3 I1a exit criteria

- Worker boot never runs image `main` or REPL.
- The substrate, not the application, owns the receive loop.
- Wire-v1 malformed input is bounded and terminal without corrupting the parent.
- Every exit path closes descriptors and reaps exactly once.
- Multiple handles coexist; every handle remains single-flight.
- No promise, callback, reactor, pool, or candidate-delivery API exists.
- `zig build verify` is green.

## 8. L1b — file-backed revision and image-baseline workflow

### 8.1 Goal

Make the L1a generation lifecycle usable from ordinary source files: exact export,
candidate-safe import, divergence detection, staged inspection, host-safe
activation, and full saved baselines suitable for I1b.

### 8.2 Planned increments

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

### 9.2 Planned increments

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
6. stop/reap idle A before declaring replacement complete.

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
- repeated successful replacements A→B→C.

For every pre-selection B failure, send another ordinary request to A and assert it
still answers. Assert exactly-once reap for both handles and no hidden pool state.

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
- No pool, retry loop, concurrent route, busy drain, or reversible-health claim is
  present.
- `zig build verify` is green.

## 10. L2 — blocking mailbox-safe live rollout

### 10.1 Goal

Deliver one live-module candidate to one stateful worker, compare a value-like base
digest, prepare and commit entirely inside that worker between ordinary requests,
and return an independent receipt. Rollback is another blocking local control.

### 10.2 Planned increments

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
| Exactly-once process reap |  | ✓ |  | ✓ | ✓ |
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
