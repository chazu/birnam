# Runtime and ABI simplification

Date: 2026-10-07.
Status: design inventory and proposed work plan. Existing root/guard reductions
and diagnostic tooling are distinguished from work still to do. This document
introduces no implementation or language change. Restored from the original
inventory after the user's 2026-10-07 clarification: simplify or eliminate
unnecessary systems; backward compatibility is not a requirement. The expanded
reviewed version and its active review records are discarded.

The goal is to remove costs and dependencies that follow from today's machinery,
while preserving Birnam's message semantics. Id remains the floor. Smaller Rust
files, fewer crates, or fewer messages are useful only when they produce simpler
responsibilities and correct behavior. There is no requirement to recreate every
existing helper, intermediate Object, root token, signature lookup or allocation.

Latest checkpoint, 2026-10-08: [reusable branches and native inputs](reusable-branches-and-native-inputs.md)
reduce actual successful addition to 338 sends (339.375 including fixture work),
with 116 Lean theorems and 57 bounded TLC configurations checked. The earlier closure, capture, Boolean, outbound ABI
retirement and Method-admission slices are linked from that note's predecessors.
The inventories and cost measurements below retain their original design-time
context; the general outbound adapter has since been deleted. Integer now reuses handlers;
general compiler closure virtualization remains a separate investigation.

## Decision: simplification first, no compatibility preservation

For every mechanism ask: what useful function does it provide, is that function
necessary, what is the smallest sufficient design, what can be deleted, and what
must be measured/proved before deletion? Existing machinery is evidence to inspect,
not a contract to preserve. Do not pay for removing a temporary and then keep an
elaborate protocol proving the old temporary could have existed.

Breaking internal/public APIs and rebuilding source artifacts or images explicitly
is acceptable in this work. Remove obsolete selectors, adapters, flags and formats
once their actual current consumers are migrated. No legacy shims, old-runtime
support, accepted-version ranges or parallel implementations solely for compatibility.
Reject an incompatible artifact clearly. Never silently recompile supplied machine
code on startup; explicit rebuild and absent-code cases retain their chosen paths.

Preserve Id as the floor, current dispatch for the operations the new implementation
uses, logical values/identity/control state, effects of actual program operations,
real resource cleanup and safe lifetimes. Rewriting a library need not reproduce
calls to its removed helper/Vector/signature machinery. Such a removed implementation
hook is not automatically a perpetual extension point. An optimizer operating on
unchanged source must still account for actual source sends and effects; an explicit
library/API redesign may remove that dependency. Never use this distinction to
skip current lookup for an operation that remains, ignore mutation of live data,
read foreign payloads or hardcode Birnam signed policy into a Rust shortcut.

## Scope and terminology

This consolidates the discussion about Integer performance, retain/release traffic,
compiler optimization/recovery, ABI conversion, ownership and modularity. Bootstrap
and image-loading decisions are included as constraints and adjacent cleanup;
completed native-module extractions are not reopened here.

Use precise terms:

- **Birnam source** is `.birnam` code. It can compile to native machine code.
- **Rust implementation** identifies source/implementation language.
- **Native ABI** is Id's tagged operand/result convention, not a second dispatch
  system. It supports Objects, raw Words, byte spans and other boundary carriers.
- **Carrier adaptation** converts between those operands and the Object values
  currently used by compiled Birnam method bodies. The phrase “authored-to-native
  bridge” obscured this distinction and should be retired in new documentation.
- **Representation responsibility** identifies the module that interprets storage.
  **Reachability** keeps Objects alive. **Code lifetime** keeps an entered method's
  code/context alive. **Borrow lifetime** describes validity of a byte span. These
  are different contracts; “ownership” alone is too ambiguous.

The native floor already accepts an operand pointer and count. The general
Execution adapter takes an Object argument sequence, signature String and result
factory. `tryNativeArguments:` already accepts direct operands for a compiler
optimization, paying guards to preserve its original Vector expression; both paths
share `invoke_values`. Reuse that core when needed, not another conversion engine.
Keep the dynamic adapter only for actual current dynamic consumers, not old code.

## Governing semantic contract

[Design 90](90-functional-modules.md), especially
[optimization versus allocation history](90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history),
is the organizing authority. The user's later direction authorizes this performance
work and retains all Rust needed for a cold bootstrap; older paused-optimizer or
crate-deletion milestones are not current implementation orders.

Preserve:

1. Current Id lookup, method replacement, inheritance, custom lookup and DNU.
   Explicitly captured Methods and already entered code keep their established
   invocation semantics; they do not freeze future dependency sends.
2. Language results, actual program effects, errors and their defined evaluation
   order. Retained operations honor their constructors/getters/providers. New
   implementations need not manufacture removed helper effects or failure points
   just to imitate the old algorithm; real cleanup and observable program actions
   must still be preserved.
3. Observable identity, aliases, shared mutable Cells, supported logical Frames,
   continuation state and entered-generation ownership. Recover current state
   before observation; do not replay completed effects.
4. Object reachability, native code/context lifetime, byte-borrow validity,
   explicit collection and actual resource/finalization duties.
5. Opaque representations. Cross-module operations use current Id messages;
   no foreign payload/context inspection, executable service tables or universal
   primitive catalog is introduced to make a path cheaper.
6. Independent cold bootstrap, valid current boundary/artifact contracts and image
   loading that maps supplied machine code without compilation. Explicit
   `--rebuild` or an image lacking machine code may select compilation. A mapping
   failure is an error, not an excuse for implicit recompilation.

Not required to match:

- Physical allocation counts, bytes, automatic GC thresholds reached at identical
  points, automatic cleanup timing, or a baseline heap of unallocated temporaries.
- Charges, prepaid identities or synthetic finalization history for omitted objects.
- The old inventory of implementation helper sends, private root registrations,
  argument containers, getters or interception points removed by a deliberate API/
  library redesign. This does not grant blanket elision of source sends in an
  unchanged program; those require semantic analysis/current-meaning admission.
- A physical heap Object or physical Frame for every logical intermediate, when
  supported observation and recovery can reconstruct what the program requires.

This does **not** permit deleting explicit program effects or real cleanup duties.
Unknown/replaced operations require ordinary execution or an accounted-for proof.
An explicit Root token returned to a caller and its public operations have a
contract; compiler-internal lifetime protection need not reproduce every duplicate
registration. Determine that boundary before removing sends.

Matched allocation counts in recent probes were experimental controls. They are
not a blanket language invariant. Existing formal models can express stronger
trace contracts than an optimization needs; their observation relations must be
instantiated against this language contract rather than baseline allocation history.

## Evidence and status

Current local evidence is linked, not extrapolated into a universal minimum:

| Evidence | Finding and limit |
| --- | --- |
| [Integer investigation](integer-performance-investigation.md) | The 2026-10-06 checkpoint measured 54 objects per successful addition, including eight Blocks, sixteen behavior objects and thirteen Words. Branch/capture, metadata, dispatch and roots explain the overhead. Historical checkpoints are not current startup timings. |
| [Root reduction](root-send-reduction.md) | Batching lowered individual retain/release sends while preserving logical registration semantics. Overall send improvement was about 2.8%; its short timing control found no meaningful difference. Duplicate-root elimination remains pending. |
| [Execution/guard work](execution-region-semantics.md) | Fixed ABI guard coalescing lowered the post-collection addition census from 3911.0625 to 3890.0625 sends, 0.54%, without a meaningful timing gain. This slice is present in the worktree, not a claim of pushed delivery. |
| [ABI investigation](abi-overhead-investigation.md) | A boxed Integer read has a 90-send outgoing bridge subtree and 67 further wrapper/capture sends. Its raw native carrier control uses 7 sends. Different carrier contracts make this a cost contrast, not a rewrite equivalence proof. |
| [Warmed ABI receipt](../measurements/abi-overhead-20261007.json) | Three outgoing bridges contribute 278 of 3889.75 sends per addition, about 7.15%. This is a message fraction, not a CPU share or all incoming/boxing work. |
| [Formal framework](../../formal/semantics/README.md) | Reusable reachability, dispatch, recovery and conditional cost lemmas plus negative models. The [guard receipt](../measurements/abi-binding-admission-20261007.json) records their audits. These are scoped models plus implementation checks, not a verified whole runtime. |

The single-read bridge partitions as follows: argument-container omission permission
30 sends; signature-owner access 20; Word boxing 19; actual Integer read 7;
name resolution 10; entry and binding/epoch checks 4. Its argument Vector is
already omitted. Much of the remaining complexity is proving that omission safe.

The Object-only incoming control adds no Id sends and shows no meaningful timing
change. The matched Word echo control adds 48 sends and one Word Object; Bytes echo
adds 55 sends and one Bytes Object. Incoming signatures are parsed at registration,
not repeatedly on every invocation. Byte copying is not on Integer's successful
`"Wo"` / `"ow"` bridge path.

### What the cost evidence prioritizes

The warmed successful addition census is 3889.75 sends. Individual `retain:` and
`release:` contribute 1031.25 (26.5%), with a further 62.1875 batch messages. There
are 738.5 logical registrations per addition. Across sixteen additions, 11816
registrations include 3072 same-message and 5251 other-activation live duplicates.
About 70% overlap some already-live registration, but that is not a proven 70%
elimination budget: scopes can end at different times or values may be replaced.

`intern:` and its nested `symbolNamed:` each occur 524 times per addition. They
are 524 operations/1048 sends, not independent saving budgets. At least 207 intern
operations are direct children of CodeObject export and Block construction; capture
projection also drives Namespace traffic. Root, intern and ABI classifications
overlap. Do not add them or equate send share to CPU share.

Named outgoing ABI subtrees account for 278 sends (7.15%). The largest first
opportunity is therefore to simplify invocation/lifetime protection and the
Block/capture causes of repeated work, not assume the ABI alone explains the whole
slowdown. Direct ABI arguments remain a small concept-removal target, not a new
plan/certificate framework prerequisite for reducing roots.

## Complexity inventory

“Nonessential” below means the mechanism is not fundamental, not that deleting it
from the existing interface is automatically behavior-preserving. “Investigate”
identifies a missing contract, proof or implementation comparison.

| ID | Complexity and function | Necessary part | Nonessential part / disposition |
| --- | --- | --- | --- |
| C01 | Uniform tagged native operands, outcomes and module boundary | Distinguish Objects, Words, byte spans, callable/context data and failures; validate consumed metadata | Keep the working floor. Do not change public struct layouts for small speed guesses. |
| C02 | Object-only calling convention for compiled Birnam bodies | Preserve logical values and per-object message behavior | Physical boxing of every local is a lowering choice. Investigate typed/unboxed locals and materialization without changing source value semantics. |
| C03 | General Execution carrier adapter and existing direct-operand compiler path | Convert genuinely mismatched carriers and support actual dynamic consumers | Reuse `invoke_values`; remove the general path from fixed calls and retire it if no current dynamic use remains. Do not retain it for old images or invent another conversion engine. |
| C04 | Vector/sequence of ABI arguments | Supply ordered operands and arity | A general collection is not required by Id. Redesign the interface to omit it and its omission guards. Only transparent optimization of an unchanged source Vector expression must preserve that expression's effects. |
| C05 | Signature Strings and repeated String/Bytes owner checks | Know carrier conversion, nullability, arity and result handling | Fixed helper kinds or existing code metadata may suffice. Eliminate per-call text access for fixed calls; add a runtime plan only if current dynamic/reuse consumers justify it. |
| C06 | Mandatory result factory on every outgoing call | Current boxing/construction policy for Word/Bytes results | Object, discard and Unit results do not use that factory. Make it conditional in a new interface; do not drop evaluation of an old argument expression. |
| C07 | Word/Bytes box → carrier → box cycles | Materialize observable values, identity and valid owner operands | Intermediate boxes consumed only by compatible operations may be virtual/carriers. Requires escape, effect, lookup and recovery proof. |
| C08 | Incoming adapter for native callers | Validate carriers, wrap values when the body needs Objects, propagate results/failures | Object-only buffering and redundant conversion can be simplified. The measured Object-only control makes this a lower performance priority. |
| C09 | Signature cloning, copied inventories, normalized-argument and operand vectors | Keep stable inputs through callbacks and accommodate supported arities | Repeated host-heap allocation/copying is optional: share immutable metadata, use bounded stack storage and general fallback. Id allocation counts do not measure these Rust allocations. |
| C10 | Copying borrowed byte spans before callbacks and on return | Prevent invalidation, reuse, mutation or reclamation of an exposed borrow | Copies within a proven callback-free interval or between already owned immutable snapshots may be avoidable. Keeping an Object alive alone does not stabilize its buffer. |
| C11 | Explicit “owner” parameters and metadata | Representation authority, GC reachability and code/context lifetime are real requirements | Repeated exposure at every call is optional. Retain dependencies in the callable/descriptor/scope once; name each lifetime contract precisely. Shared Objects do not imply exclusive Rust ownership. |
| C12 | Many retain/release registrations, including duplicates | Protect every needed value at safepoints; preserve explicit token semantics | Registering every intermediate repeatedly is not a universal requirement. Investigate caller coverage, liveness and internal duplicate elimination before enabling coalescing. |
| C13 | Per-value root sends and cleanup loops | Correct acquisition/failure/cleanup order and live inputs | Batches already exist. Expand only when total guard/copy/fallback cost beats the individual path; never queue unprotected results across a callback. |
| C14 | Layered permission/lookup/allocator checks | Real collector safety, retained operation effects and current mutable conditions | First delete checks whose only job is omitting removed machinery. Combine remaining owner-local facts; do not replace a removed permission chain with another compatibility protocol. |
| C15 | Global behavior epoch as a validity gate | Invalidate every relevant changed binding, association or permission | Global epoch equality is conservative. Investigate relevant dependency facts; unchanged method epoch alone does not imply unchanged permission or mutable data. |
| C16 | Repeated interning and Namespace lookup | Canonical live Symbols and current binding semantics | Retain fixed Symbols at their actual lifetime holder. Redesign internal naming paths instead of preserving old per-call interning hooks. Keep mutable Namespace values/current data live; explicit source interning remains an operation. |
| C17 | Exporting complete CodeObject bytes to inspect arity/captures repeatedly | Validate admitted code shape and consistency with the Method | Full export/allocation/inspection per Block is a representation choice. Let CodeObject expose compact owned facts through messages; account for replaced exporters. |
| C18 | Capture slot construction, per-slot writes and rereads | Correct capture order/aliases, layout, bounds, roots and actual owner effects | Repeated storage validation and copying may be batchable inside ObjectModel/Execution. Do not read ObjectModel storage from another module. |
| C19 | Branch Block/private behavior/Frame construction | Current control dispatch, lexical state, logical Frames and observable identities | Eager construction is not inherently required. Virtualize/materialize where proved safe; nonescape alone is insufficient. |
| C20 | Integer helper layering, operation Map and reusable closures | Signed policy, operand/Error/Float precedence and supported replacement hooks | Source organization can be simplified. Removing a Map/helper dispatch can change effects or extension behavior; investigate rather than assuming equivalence. Signed arithmetic stays in Birnam source. |
| C21 | Direction-specific signatures and deferred incoming `v` | Preserve meaningful Unit/nil/nullability/failure and native-result forwarding | Outgoing args `o/w/b`, results `o/w/W/b/u/x`; incoming args `o/O/w/b`, results `o/w/W/b/B/u/x/v`. Simplify encoding and separate control policy where useful; keep real distinctions, not old spelling compatibility. |
| C22 | Guards recognized only by native method/context witnesses | Certify actual selected meaning, including context; preserve fallback | Rust-specific recognition should not permanently force future Birnam implementations onto slow paths. Investigate implementation-independent owner/CodeObject contracts without foreign executable caches. |
| C23 | Optimization/recovery metadata and formal machinery | Certify admitted regions, keep logical state/lifetimes, recover before exposure | A universal exact send count, allocation journals and whole-runtime proof prerequisites are unnecessary. Build small reusable proof rules and concrete correspondence. |
| C24 | Defensive checks and numerical limits | Check real lengths, arities, extents, versions, overflow and trusted boundary contracts | Particular fast-path capacities need general fallback; arbitrary global emission limits or speculative native-code containment are not semantic requirements. Audit each limit instead of deleting validation wholesale. |
| C25 | Native module/crate proliferation | Complete functional responsibilities and independent Id interfaces support replacement | Crate/file count and SLOC alone do not establish responsibility. Avoid new wrapper-only modules, shared-state facades or a universal marshalling service. |
| C26 | Rust/Birnam duplication during migration | Keep Rust actually needed to build/start from a clean checkout | A redundant production implementation is removable. Record bootstrap seeds separately; do not call an entire module port complete when only its policy moved. |
| C27 | Startup/tool snapshots, compilation and image admission | Compiled startup, native relocation/linking, valid restoration and explicit rebuild | Recompiling supplied machine code at load is prohibited. Tool snapshot creation and image loading are different workloads; evaluate snapshot packaging separately from loader correctness. |
| C28 | ImageFormat field-view Objects, numeric boxes and repeated block invocations | Canonical container validation, ordered field interpretation, reference/checksum checks and compatible bytes | A view Object and general callback per field are implementation choices. Prefer source cleanup and generic owned-byte/row operations where equivalent; do not revive a native whole-ImageFormat bypass. Recheck residual work after the already recorded startup fixes. |
| C29 | Optimization flags, tiers and opt-in/default paths | Guarded fallback and reproducible experimental controls | Permanent opt-in status and duplicate old paths are optional after qualification. Keep useful diagnostic toggles separate from semantic requirements; fewer sends alone does not qualify a default optimization. |
| C30 | Native executable size and linked bootstrap machinery | Executable entry/loading, required cold-start code, code metadata and public exports | A reported binary size alone does not identify unnecessary code. Audit sections/dependencies before stripping or removing implementations; preserve diagnostics, symbol loading and native/image contracts. |

## Proposed calling shape

Keep the tagged Id boundary. Use direct ordered operands and fixed conversion
information where the source operation is fixed; send the actual target operation
through current Id lookup and adapt only the required result. For dynamic calls,
retain a general path only for identified consumers. A dynamic selector does not
necessarily mean a dynamic conversion signature.

Start with two illustrative helper forms if they earn their place:

```text
optionalWordResultOf: target selector: selector
  objectArgument: value resultFactory: factory    (o → W)
objectResultOf: target selector: selector
  wordArgument: bitsObject                        (w → o)
```

These reuse `invoke_values`; they do not require an argument Vector, per-call
signature String or a new first-class plan Object. Zero-argument helpers currently
use live Namespace Word/Bytes factories and are not interchangeable with an
explicit factory/subclass or optional/discard contract. Keep correct factory
choice in the replacement interface. Byte forms need owned snapshots through
callbacks; they are separate from the first Word forms.

An API change can remove old bridge/Vector/getter interception behavior. That is
accepted implementation evolution here, not a requirement to preserve both paths.
Keep target/factory/bits/bytes operations that the new implementation uses live
through Id, and preserve actual program effects and safe failure/lifetime behavior.
A removed argument container need not fail allocation before the target as its
old incarnation did; actual collector-phase restrictions and real error/effect
order in the chosen new operation still need a coherent contract and tests.

Do not start with another loaded module, shared primitive catalog, foreign payload
access, cached target executable, or a conversion-plan lifecycle merely because
those abstractions can be described. Typed/unboxed locals and recovery are later
representation investigations, not a new value language assumed implemented.

## Work items and ordering

All unchecked items are proposed. Each item must record its implementation,
acceptance evidence, measured gain and keep/reject decision here when completed.
A failed performance experiment can be closed as rejected with its reasoning;
it must not remain enabled merely because it lowers one counter.

### Simplify invocation protection and incidental calling machinery

- [ ] **W01 — Define the minimal observation and lifetime model** (C02, C07, C12, C19, C23).
  Model logical values/Frames, actual program effects, dispatch and reachability;
  omit baseline private registration/allocation history from the required equality.
  Keep explicit program Root/resource operations meaningful. Separate correctness
  from cost: use a behavior-only relation where fallback costs more; retain existing
  costed `Refines` for its appropriate domains. Supply the caller-coverage premise
  from a real invocation route rather than assuming every C local is protected.
  This is one small model for the chosen transformation, not an all-system ledger
  prerequisite to every deletion. The first scoped proof covered W09's entry roots;
  the whole-tree census now prioritizes closure/capture work (W07/W08/W12).

- [x] **W02 — Remove argument containers from fixed ABI calls** (C03, C04, C06).
- [x] **W03 — Eliminate repeated fixed-signature work** (C05, C11, C22).
- [x] **W04 — Move current consumers and remove their old dependency** (C03–C06, C20).
- [x] **W05 — Delete displaced calling machinery** (C03–C06, C25).
  Completed across the Integer slice and remaining seven production sites.
  Five fixed argument/factory forms and existing zero-argument queries preserve
  current owner messages and callback-safe lifetimes. The outbound signature API,
  compiler Vector-elision pass, unused permission helpers and ABI-proof toggle
  are deleted. Incoming authored/Alien signatures remain. No dynamic production
  caller of this outbound API was found. See [retirement and evidence](outbound-bridge-retirement.md).
  Bytes conversion/Boolean printing probes improve; arithmetic remains at 829.375
  sends/addition. Full image/worker suites are not claimed by this checkpoint.

### Owner-local reductions

These are independent simplification candidates, not instructions to run concurrent benchmarks.

- [ ] **W06 — Batch shared allocation-permission facts** (C14, C15).
  Delete guards for argument containers or intermediates the new operation no
  longer constructs. Measure remaining Word/ObjectModel/GC consumers; combine
  necessary callback-free facts while retaining real phase/permission conditions.
  Prefer reducing repeated facts at those owners to layering more caller queries. Acceptance:
  denied/revoked permission without epoch mutation, changed allocator/prepare
  methods, custom lookup, collecting/reentrant providers, ordinary fallback order
  and no mutation on a declined batch. Compare total messages and CPU; the existing
  ABI guard slice's 0.54% send reduction is not a template for a large speed claim.

- [x] **W07 — Expose compact CodeObject shape** (C17).
  Implemented 2026-10-07: current-owner `parameterCountOf:`/`captureCountOf:` Word
  messages replace canonical export/reparse in Block construction and image shells.
  Canonical export remains for actual byte consumers. Focused replacement, mismatch,
  invalid-carrier, precise-GC and synchronous/resumable tests pass. See the
  [combined construction result](closure-construction-simplification.md).

- [x] **W08 — Simplify capture construction/access** (C18, C13).
  Construction passes ordered Object operands directly to the owner. Access now
  uses direct Id reader messages with one current-owner resolution per invocation;
  pure reads omit allocation admission, and the canonical reader borrows captures
  under live Block coverage. Replacement/custom readers retain fresh results.
  The [access result](capture-access-simplification.md) records callback/mutation/GC,
  bounded task and formal evidence: total sends fall another 20.9%, to 1,367.375
  per addition. Path-specific liveness/unused binding reads are optional later work,
  not part of this completed protocol simplification.

- [ ] **W09 — Replace duplicated invocation protection with one covering scope** (C11–C13).
  **Paused scoped work.** `Execution::invoke_body` already installs an Execution scope
  for the Method, receiver and Object arguments; generated synchronous code also
  emits `keep_entry` registrations for receiver/arguments. Audit named adapters,
  Blocks and typed incoming conversion to prove which exact values/intervals overlap.
  Remove the generated registration for inputs covered throughout that invocation,
  then extend only where measured evidence supports it. Keep protection for new
  results and captures not covered by another live edge.
  Prove coverage at every safepoint, callback, exceptional exit and suspension,
  plus safe return-value transfer. Code owners stay live. Explicit user Root API
  operations remain real; compiler-private Root calls and their incidental hooks
  need not be reenacted. Do not create a token/lease/observer protocol to reproduce
  removed private registrations. Use W01's scoped Lean/TLA+ model and the experiment
  sequence below; current code/proofs are evidence, not a delivered removal.

- [ ] **W10 — Separate immutable dependency facts from mutable verdicts** (C15, C16, C22).
  Investigate relevant-fact certificates instead of invalidating everything on
  every global epoch change. Include metabehavior/default lookup, parents,
  associations, factory/Namespace bindings and any data actually cached. Retain
  fixed Symbols where their identities are protected; honor old interning effects
  when optimizing an old path. Acceptance: same-epoch revocation, unrelated mutation,
  relevant mutation/reassociation, stale address/context reuse and custom lookup.
  Use Id's authority; do not replicate its dictionaries in another module.

### Then: representation and control optimizations needing recovery

- [ ] **W11 — Keep a nonescaping Word round trip as a carrier** (C02, C07).
  Prove the Integer read → Word use → Integer construction region for exact valid
  operands first. Use current owner operations and generic unsigned Word policy;
  do not add native signed Integer arithmetic as a shortcut. Acceptance: boxing-
  constructor/bits replacement, observed identity/aliases, reflection, Frame stop,
  collection, mixed/error/overflow paths and recovery before exposure. Materialize
  or decline when needed. If avoiding boxing requires changing value/message
  semantics, record that as a separate design decision, not a transparent optimizer.

- [ ] **W12 — Virtualize eligible branch Blocks and logical Frames** (C19, C23).
  Start with one concrete branch shape after W01/W07/W08. Preserve current Boolean/
  Block invocation, captures/Cells, supported inspection, continuation capture and
  entered code lifetime. Acceptance: replacement at each boundary, materialization
  before a callback/observer, paused/recovered state and no repeated effects.
  Unknown constructors/finalizers keep ordinary execution. Do not fabricate a
  baseline allocation journal or claim nonescape alone proves this safe.

- [ ] **W13 — Simplify Integer source orchestration** (C20).
  Simplify helper/capture layering and operation Map/closure selection. Preserve
  signed results, validation, Error/Float precedence and current lookup for retained
  operations. The new library implementation can stop using a deleted helper/Map;
  it need not preserve that obsolete interception surface. Do not silently elide
  an actual source dispatch in an unchanged program without a semantic proof.
  Acceptance: an independent numeric oracle, representative
  failure/mixed cases and public-path counts; no private core-only speedup claim.

### Bounded cleanup and architecture investigations

- [ ] **W14 — Simplify incoming temporary storage** (C08, C09).
  Share immutable signature storage and avoid redundant buffers/copies for validated
  Object-only/small calls. Preserve nullable normalization and invalid-carrier
  rejection before executing the body. Keep larger-arity fallback and own all byte
  inputs before any factory callback. Acceptance: incoming-carrier/alias tests and
  the Object/Word/Bytes controls. Low priority unless CPU evidence beats the already
  near-equal Object-only control.

- [ ] **W15 — Review byte copies and returned-buffer contracts** (C10, C11).
  Classify each copy by mutation, reuse, GC and reentrancy boundary. Reuse owned
  immutable snapshots or an owner-provided stable borrow only when its contract
  proves validity. Acceptance: nested byte-return calls, mutating/collecting
  providers, multiple arguments/results and lifetime after scope exit. A retained
  owner alone is insufficient. This is relevant to byte workloads, not a presumed
  fix for Integer addition.

- [ ] **W16 — Reduce ownership exposure** (C11, C25).
  Document which references retain representation data, code/context or borrow
  storage. Have callable/plan/scope construction retain what it needs once; remove
  repeated parameters only where the relationship can be derived reliably.
  Acceptance: replacement during an entered call, Method aliases, detached/suspended
  Frames, final code retirement and byte-buffer reuse. Never infer that a native
  context pointer is an Oop or that pinning code traces all native references.

- [ ] **W17 — Review deferred results and ABI variant vocabulary** (C21).
  Inventory outgoing arguments `o/w/b` and results `o/w/W/b/u/x` separately from
  incoming arguments `o/O/w/b` and results `o/w/W/b/B/u/x/v`.
  Consider placing deferred thunk execution in the execution/control contract or
  a generic result-forwarding lowering. Acceptance: native Word/Bytes/Unit/entry
  results and failures, single branch evaluation, borrow lifetime through cleanup,
  and callback-visible Frame/order behavior. Unit, language nil, nullable Objects
  and failure are not interchangeable. Keep variants whose distinctions have
  real consumers; clarity alone is not a measured performance gain.

- [ ] **W18 — Audit limits and disproportionate defensive scaffolding** (C24).
  Record each limit's purpose and fallback: stack-buffer capacity, ABI quantity,
  format extent, actual resource boundary or obsolete convenience cap. Preserve
  consumed-length/overflow/version checks and concrete trusted-native contracts.
  Do not add a native-code sandbox or global coupling to contain hypothetical C
  violations. The arbitrary image-emission cap was already removed; do not recreate
  it under another name. Acceptance: larger supported calls use the general path;
  malformed quantities fail before memory access.

- [ ] **W19 — Keep module and bootstrap simplification honest** (C22, C25, C26).
  Use the [decomposition roadmap](native-module-decomposition-roadmap.md) and
  [value foundation](native-value-foundation.md). Do not repeat completed splits
  merely to shrink SLOC. Each new independent module needs a complete function
  and state, not a facade over another module. Identify each retained Rust seed's
  cold-start consumer and each redundant production path before deletion.
  Acceptance for removals: clean-checkout source bootstrap without the removed
  implementation, production tests and updated inventories. Remove redundant
  production APIs; retain only genuine bootstrap requirements.

- [ ] **W20 — Review snapshot packaging separately from image loading** (C27).
  Document bootstrap/tool snapshot contents, consumers, dependencies and whether
  tooling can load on demand. Investigate removal/consolidation only after proving
  those consumers have a replacement. Keep the existing image compilation policy.
  Acceptance: a machine-code image loads with compiler routes disabled; a mapping
  failure stays a failure; explicit rebuild and no-code images still compile.
  Snapshot creation's compilation requests are not loader recompilation. Do not
  run repeated full snapshot builds as routine performance probes.

- [ ] **W21 — Reassess residual ImageFormat field/row work** (C07, C09, C10, C28).
  Treat the historical field-view/boxing/callback cost as evidence to inspect,
  not proof that the already fixed startup slowdown is still present. Profile a
  bounded representative record/field microcase; simplify Birnam loops and use
  generic Bytes-owned operations over caller-defined durable schemas. Acceptance:
  independent frozen-format readers/fixtures, malformed/truncated data, field and
  reference ordering, callbacks/borrow lifetime, and a focused image admission
  check when the interface changes. No foreign object-layout access, new native
  image-format policy implementation or per-iteration full snapshot benchmark.

- [ ] **W22 — Retire obsolete optimization controls and duplicated paths** (C23, C29).
  Inventory the compiler pass flags and execution toggles, separating useful
  comparison/fallback controls from old rollout scaffolding. Defaults must retain
  guards, recovery and current-message semantics; removing a toggle is not the
  same as removing fallback. Acceptance: ordinary/default behavior, negative
  admission cases, compiled/image metadata and short measured comparisons. Do
  not reset already qualified default Integer policy to opt-in, or retain a
  redundant implementation solely for its historical benchmark role.

- [ ] **W23 — Audit binary composition before size cleanup** (C26, C30).
  Inspect actual native sections, debug/unwind data, required exports and linked
  code. Identify duplication and unused dependencies before considering linker
  options or removal; record the size/diagnostic tradeoff. Acceptance: cold
  bootstrap, native module symbol resolution, mapped images and useful error/
  stack diagnostics. This is a low-priority investigation, not a claim that the
  earlier reported one-megabyte executable is currently measured or excessive.

## Proof, measurement and completion discipline

Use Integer addition as one end-to-end acceptance case, not the definition of the
language model. String length, collection access, native carrier calls and Block
construction are independent witnesses for reusable dispatch/lifetime/recovery rules.

For each candidate, record:

- Required logical operations and their responsible modules.
- The current interface versus a proposed changed interface. A cheaper new API
  is not a lower bound for the old API's semantics.
- Every dependency, effect/observation boundary, permission and recovery recipe.
- An emitted admissible plan as an upper bound; lower bounds only under the same
  vocabulary and constraints. The existing ideal owner-run floors of 3/2/2 are
  partial kernels, not total-send minima for addition/String/Vector.
- The scope of the Lean theorem, TLA+ state space, required negative cases and
  concrete implementation checks. No theorem count substitutes for that mapping.
- Total sends, allocations and CPU, including guards, copies and fallback cost.
  A successful guard may save sends while a declined guard adds one. Compare real
  behavior before attributing any gain to an optimizer.

Use the existing short microbenchmark tooling at reduced priority. Keep timed
ordinary builds separate from instrumented counts, first-post-collection separate
from warmed calls, and inclusive subtrees separate from disjoint attribution.
Native/reference controls with different carriers or wrappers remain cost contrasts.
If a sample is too short, adjust a bounded batch or mark it inconclusive; do not
require an idle machine or a day of repeated benchmarks. Full startup/images are
milestone validation when affected, not the inner iteration loop.

A work item is complete when its contract, implementation/removal, relevant tests,
measurement and keep/reject rationale are recorded. Preserve independent numeric/
codec oracles, current-message and callback/GC checks, and image/bootstrap gates
when those paths change. Compilation, focused checks, full tests, installation,
commit and push are separate evidence states. No new feature is called delivered
merely because its framework proof or private probe passes.

Implementation anchors for the first slices:

- [Id operands/results](../../rust/birnam-abi/src/id_protocol.rs) and
  [signature vocabulary](../../rust/birnam-abi/src/native_signature.rs).
- [Execution conversion](../../rust/birnam-execution/src/native_abi.rs),
  [binding proofs](../../rust/birnam-execution/src/binding_proof.rs) and
  [Block construction](../../rust/birnam-execution/src/machine.rs).
- [Synchronous lowering](../../rust/birnam-native-backend/src/native_sync.rs) and
  [generated call/root helpers](../../rust/birnam-native-backend/src/native_prelude.c).
- [Root registration](../../rust/birnam-root-registry/src/lib.rs),
  [ObjectModel](../../rust/birnam-object-model/src/native.rs),
  [GC admission](../../rust/birnam-collect/src/construction.rs) and
  [CodeObject](../../rust/birnam-portable-code/src/lib.rs).
- [Integer policy](../../modules/integer.birnam),
  [native-value conversion policy](../../modules/native-values.birnam) and
  [ImageFormat policy](../../modules/image-format.birnam).

## Highest-leverage next target: closure and capture machinery

The [whole-tree send analysis](integer-send-totality.md) accounts for all 61,542
sends in sixteen warmed additions. Closure construction costs 2,241 sends/addition
(58.3%) and capture retrieval another 550 (14.3%). Together they cause **72.6%**
of sends, including much of the lifetime and name-handling work that the earlier
leaf histogram attributed to separate cost families. Eight branch Blocks are
constructed; four are invoked. Constructing the unused four costs 925 sends.

Prioritize W07/W08/W12: compact CodeObject shape instead of whole-program export,
simpler capture construction/access, then investigation of virtual branch Blocks.
Preserve current Id dispatch and observable Block/capture behavior; these numbers
identify targets, not automatic elision permissions or CPU savings. Fixed ABI
simplification remains independent concept-removal work.

The [closure necessity audit](closure-machinery-necessity.md) identifies 1,031
capture-transport sends and 672 program-shape acquisition sends per addition as
replaceable protocol costs, before accounting for their simpler replacements.
Retain boundary/lifetime correctness; reconsider repeated immutable validation,
per-slot transport checks and allocation admission on pure reads.

### Scoped invocation-root experiment: paused

The prior root-first recommendation over-weighted a leaf cost bucket. Its narrow
input-root work and formal/runtime checks remain in the worktree; they do not
justify deleting the entire lifetime bucket or further prioritizing it over the
larger structural cause. The following scoped plan remains useful for completing
that work later. It is not the current next optimization target.

### 1. Targeted trace experiment

Use a tiny generated synchronous method that returns an argument, then one that
makes a nested call and one that creates a result. Identify which input objects
Execution roots and which `keep_entry` roots duplicate them. Add callback/collection
points before body work, inside the nested call, during failure and at cleanup.
Use precise GC to avoid accidentally relying on conservative C-stack roots.
Record current and proposed protected-object sets, actual sends, peak roots,
correctness and CPU. Distinguish protected inputs from newly produced values.

The first candidate removes only input registrations backed by the proven covering
invocation scope. It keeps per-result protection until a separate proof exists.
Raw native/initializer routes must be checked; an unprotected direct entry is not
made safe by calling it “compiled.” Keep a necessary ordinary scope for routes not
covered by the new invocation rule, not a backward-compatibility implementation.

### 2. Lean: prove the necessary lifetime property

Model a heap/reference graph, live invocation scopes, future-needed values and
code/context references. State the actual coverage premise: every needed object
is reachable from a live scope/Frame/owner edge at each collection point. Prove
that omitting a duplicate input registration preserves needed-object reachability
while its covering invocation scope remains live. Compose nested calls and show
child exit does not remove the parent's protection. Model result transfer explicitly.

Reuse `Reach`, `needed_survives` and composition lemmas from the existing packages.
Their premises must be discharged against `invoke_body`/generated entry code;
abstract theorem counts do not validate an emitter. A needed-values survival
statement is often the right property; do not demand identical baseline garbage
or exact private token history. A stronger equal-survivor theorem may be useful
for a narrower fixed graph but is not a universal requirement. Do not weaken real
program observations just to make elision pass.

Separate behavioral correctness from optional cost theorems. Costed `Refines`
currently includes non-increasing sends; use it only on matching domains and use a
behavior-only relation for fallbacks whose setup costs more. No global proof of
all Birnam or universal theoretical send minimum is a prerequisite to this slice.

### 3. TLA+: search the dangerous interleavings

Use a bounded model with caller scope, callee scope, aliases, input/new-result
slots, code generation, callbacks and collection. Explore reentry, exception
cleanup, method replacement, parent/child exit, return handoff and suspend/resume
where the chosen path supports it. Check that no needed object/code is reclaimed,
future sends see current meanings, and required logical state survives exposure.

Require counterexamples for intentionally broken variants: remove the first real
root, expire the covering scope too early, forget a new result, leave a return
handoff gap, rely on a mutated reference edge, or drop entered code after replacement.
TLC exhausts that small scenario space; it does not prove an arbitrary runtime.
If removal merely shifts required lifetime work elsewhere, count that work too.

### 4. Change one lowering and delete what it displaces

Apply the proved rule to one generated-call route, run the small GC/callback cases,
then the existing sixteen-addition and independent String/collection probes.
Measure actual total sends and short paired CPU samples at reduced priority. Keep
instrumented counts separate from timing; use 2–10 second measurement budgets.
No repeated full image build, idle-machine demand or day-long benchmark campaign.

When the replacement covers all current routes, delete obsolete generated input
registration/batching flags and cleanup bookkeeping it makes unnecessary. Do not
retain both versions for old artifacts; rebuild explicitly when needed. Stop or
change direction if the real gain is small or the replacement introduces more
machinery than it removes. Removing a conceptual layer is valuable, but its costs
and remaining protection must be reported honestly.

## Boolean selection follow-up (2026-10-07)

The [identity-selection slice](boolean-selection-simplification.md) replaces
Boolean's comparison-Word allocation/unboxing with one generic data-selection
message. Policy remains in Birnam; the native operation never invokes a branch.
Total addition sends fall another 20.8%, to 1,083.375; allocations fall from thirty
to twenty. Replacement/GC/carrier, bounded-task and formal checks pass.
ABI bridges (281 sends) are now the largest measured category. This completes the
specific Boolean intermediate-box removal, not W12's branch virtualization or
all typed/unboxed compiler-local work.

## Method admission follow-up (2026-10-07)

[Method code admission](method-code-admission.md) moves shape-owner queries out
of generated literal construction into one immutable binding per portable Method.
Methods trace the admitted code; aliases retain it. Explicit external code pairings
keep validation and supplied CodeObject identity. Generated constructors now pass
only Method plus captures. Total addition sends fall another 13.5%, to 717.375,
with lifetime/replacement/materialization/bounded-task checks passing. Per-instance
behavior sharing and virtual Blocks remain separate investigations.
