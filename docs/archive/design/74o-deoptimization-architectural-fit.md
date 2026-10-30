# 74O — Deoptimization architectural fit

> **Allocation policy superseded (2026-09-24):**
> [Design 90](../2026-10-runtime-refactors/90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history)
> retires baseline allocation/count/GC-timing equivalence, dead-payload replay and
> prepaid deferred identities. Preserve language effects, observable identity,
> recovery and actual resource lifetimes. Older algorithms, receipts and commands
> below record history; they do not require restoring that machinery. See the
> [Self implementation review](../2026-10-runtime-refactors/90-self-optimization-review.md).

Status: bounded feasibility experiments complete, 2026-09-11. General
reconstruction is test-only; no production admission or performance claim.

> Superseded implementation gap: [60P](../2026-10-runtime-refactors/60p-v2-control-completion.md) completes
> V2 suspend/capture publication and result transfer, and removes Frame layout
> V1. The original 74O receipt and findings below describe the earlier feasibility
> snapshot. The live continuation probe now expects valid Call-edge publication
> to succeed and invalid self-edge publication to fail. General reconstruction
> admission remains future work.

## Finding

Birnam's explicit Frames, compiler layouts, GC roots and generation ownership
can support a bounded reconstruction mechanism. Ordinary Birnam code can produce
the tested recovery policy, including after source-independent compilation.
These results justify designing a production recovery contract; they do not
justify treating a general optimizer as already supported.

There are fundamental conflicts with **specific optimizations**: replacing an
escaped identity, removing an observable native effect, exposing an absent Frame,
or eliminating a real finalization/resource effect changes the program.
Matching automatic collection timing and hypothetical heap statistics is no longer
required (Design 90). Later reconstruction cannot repair an observation already made. These
are eligibility limits, not evidence that deoptimization itself requires a less
malleable language. Arbitrary opaque native activations also remain outside the
reconstructable state: call them with concrete state and preserve entry barriers.

At the original 74O snapshot, one implementation gap was V2 continuation action
publication. Existing
continuation storage preserves reconstructed compiler Frames, including roots and
pending call transitions, but capture/suspend publication still called the V1-only
`set_program_counter`. The experiment distinguishes these two paths; it does not
silently bypass the rejected public operation and call that production support.

## Scope and reproducibility

The user requested bounded experiments to determine deoptimization's broad fit
with core tenets, invariants and the current implementation. The predeclared
dimensions were logical call-chain reconstruction, observation/native boundaries,
object elimination/identity, and compiler/floor expressibility. The public IdApi,
Method ABI, primitive semantics and durable formats remain fixed. Internal
test-only extensions are permitted. Earlier uncommitted 74M/74N work is retained.

Run from the repository root on supported arm64 macOS:

```sh
python3 scripts/deopt-architectural-fit.py --verify
```

The [runner](../../../scripts/deopt-architectural-fit.py) builds an ordinary disposable
prefix outside the source tree to execute the Id producer; it does not replace
the user's installation or install the test-only reconstruction helper. It
records exact commands, outputs, source hashes, generated portable programs,
compiler descriptors/C/facts, producer source/output and prefix binary hashes in
the [receipt](../../measurements/74o-deoptimization-fit.json). Frozen ABI, image,
descriptor, registration and baseline emitter authorities are compared with
`24e200a3`. The full gate runs `mise run verify`.

Debug and release each pass five tests: 22 execution rows (nine expected-equal
pairs and two deliberate counterexample pairs), plus object and continuation
summaries. Both profiles produce identical recorded observations. Full gate:
453 Rust tests passed, 11 diagnostic tests ignored; native-prefix, cold-archive
and relocated-runtime gates passed.

No timing or speedup is inferred. These probes run under instrumented conditions,
with a deliberately restricted native prefix. They do not supersede 74M/74N's
inconclusive independent native-tier performance results.

## 1. Reconstructing an absent logical call

The [runtime experiment](../../../rust/birnam-runtime/src/runtime/architectural_fit_experiment.rs)
encodes two portable Blocks, invokes the production lowering, liveness, coloring,
descriptor and baseline C generators, and compiles actual native code. Its caller
normally sends to a callee whose body keeps two aliases, observes them, and
returns twice their value.

The experimental candidate executes a bounded prefix with entry values, aliases,
and one explicitly trusted pure identity leaf. It then reconstructs the missing
callee at PC 1. The caller remains its original allocation with its existing
pending call. Recorded Frame depth goes from one to two. Ordinary baseline code
resumes through the unchanged real stackless driver, receives native answers and
commits the caller's original return destination. Recovery itself performs no Id
send, Birnam allocation or callback while installing state.

The [arena helper](../../../rust/birnam-runtime/src/execution/inline_recovery_experiment.rs)
checks current caller/descriptor allocation, pending-call kind and return slot,
callee PC, physical slot bounds, source spill bounds, duplicate destinations and
explicit initialization of traced locations before mutation. Seven malformed
recipes are rejected: duplicate target, missing live locations, invalid source
spill, invalid destination, invalid callee PC, wrong return slot, and a different
caller descriptor allocation with the same public identity. Complete logical
arena snapshots include pending edges, root-state PCs, words, scratch contents,
return destinations, ownership entries and handles; all remain unchanged.

This is not a general metadata-admission validator. Descriptor/entry association
and spill provenance come from the trusted fixture/compiler. A production API
must validate its authority and acquire/release roots and code pins transactionally.
The experiment reconstructs one absent callee, not arbitrary-depth virtual call
chains, virtual closure environments or cyclic materialization graphs.

## 2. Observations and native participation

| Case | Observed result |
| --- | --- |
| Plain | Both executions return 28 with identical effects and Frame observations; only the candidate reconstructs. |
| GC | A precise root-provider callback writes boxed 22 into both aliased recovered locations. Both executions return 44; unrelated garbage is reclaimed. |
| Active replacement | Existing Frames finish in their original generation and return 28; a future send uses the replacement and returns 99. Registry retirement is refused while active; entries and pins release after completion. |
| Machine override | An override installed after recovery receives six calls in both executions. |
| Custom lookup | An override installed after recovery receives 26 calls in both executions. |
| Observable native identity | Candidate guard declines optimization; the arbitrary native method runs with its effect and correct callee Frame. |
| Native reentry | Nested stackless execution reaches depth three and returns correctly without disturbing the outer computation. |
| Native entry barrier | An actual nested drive rejects capture and suspension across its entry barrier, then both computations finish. |
| Bad maps | Seven rejections preserve live arena state; the subsequent valid recovery completes. |

Frame inspection uses ordinary Id `currentFrameView`, `generationIdentity` and
`programCounter` sends. The saved original caller stays usable across recovery;
saved views are stale after return. These observations include the existing
distinction between a public next PC and the root state of an in-progress call.

Two deliberately unsound controls are essential:

- Skipping an arbitrary native method that happens to return its receiver keeps
  the numeric answer but loses its effect and Frame observation.
- Calling that observer before reification keeps both answer and effect sequence,
  but the observer sees caller generation 74001 instead of callee generation 74002.

Thus arbitrary native methods need no new ABI or metadata to remain callable.
They cannot be assumed pure or be called with hidden logical state that they may
observe. The fixture's pure-leaf authority is explicit and test-owned. It retains
the exact installed method pointer; separately taking Rust function addresses
was insufficient in a release build and is not an identity contract.

Native code ownership in this new fixture is tested at registration/pin lifetime
boundaries. Its local Unit owners are deliberately retained until fixture cleanup.
Actual unloading is exercised by the existing generated-deoptimization regression
tests in the full gate, not newly demonstrated by this fixture's local Unit drop.

## 3. Object elimination and observable allocation

The object test sends the ordinary Id identity selector `==`:

- A fresh integer box with the same numeric value differs from an escaped box.
- Materializing once and supplying that Oop to both aliases preserves identity.
- Independently materializing each alias creates different identities.

The allocation control uses a legal native finalizer, including the String
storage cleanup it owns, and explicit precise collections. Retaining the
allocation produces one finalizer effect at the checkpoint. Eliminating it
produces zero. Materializing and collecting later reaches the same eventual
count but cannot change the earlier checkpoint. Keeping the allocation before
observation reproduces the baseline result.

These are runtime semantic counterexamples, not a production escape-analysis or
scalar-replacement implementation. Unescaped, unobserved virtual objects remain
possible candidates, provided a compiler can prove the relevant conditions and
materialize a shared identity before exposure. Cycles, mutable captured Cells,
raw storage and finalizer-bearing object virtualization are not implemented here.

There is a further legality obligation even for the prefix used in section 1:
it omits some baseline Machine operand-box allocations. The experiment does not
prove equivalence under every allocation-triggered collection schedule or every
GC observer. Production optimization must establish that omitted allocations
are unobservable, preserve required effects/checkpoints, or decline optimization.
A passing selected-checkpoint comparison is not permission to weaken allocation
or native-observation semantics.

## 4. Compiler policy and floor mechanism

The [Id-authored policy](../../../scripts/fixtures/deopt-recovery-policy.birnam)
receives the actual production compiler's physical slot count, nil spill index,
and live `(physical slot, spill index)` facts. Using ordinary collections and
sends, it constructs all slot assignments. The runner executes the policy both
ordinarily and after `Codegen materializeResumable:` of its source-independent
CodeObject. Both produce the same recipe. Debug and release runtime probes then
consume those produced bytes as their actual recovery slot map; an independent
comparison checks the compiler's liveness/layout mapping.

This establishes that the bounded mapping policy and its data need not be
Rust-only. File transport is experimental. It does not establish an ordinary Id
operation for admitting generation capabilities, a complete self-hosted optimizing
compiler, or a production recovery ABI. Those remain concrete admission work.

| Responsibility | Existing support and required work |
| --- | --- |
| Compiler / Id policy | Choose legal speculation; identify observation checkpoints; compute logical calls, resume PCs, live values and shared identities. Existing lowering/layout facts and the Id recipe producer demonstrate a bounded part. General recovery-map generation and escape/effect analysis remain missing. |
| Optimized native entry | Keep live Oops rooted and original code owned; spill state and exit before an assumption-dependent observation. The bounded prefix has no callback/allocation while its callee is absent. General spill/root maps are not implemented. |
| Floor / Runtime mechanism | Validate recovery authority and install complete Frames without user callbacks midway; preserve pending calls, roots, aliases and generation ownership. The test-only installer and existing driver demonstrate the basic transfer. Production admission and multi-Frame atomicity remain missing. |
| Ordinary Id protocol | Describe policy/data through objects and sends, retaining malleability. The policy runs today; the checked reconstruction operation is not exposed today. An added guarded Machine operation is a possible design direction, not a shipped API. |
| Continuation mechanism | Existing storage/restore handles reconstructed V2 state and one-shot ownership. Action publication needs an explicit V2 transition contract. No serialization of opaque native activations is proposed. |

No additional IdApi table field is required by these experiments. That negative
ABI result alone does not prove a satisfactory final protocol; its authority,
override behavior and failure semantics must be designed and tested explicitly.

## 5. Continuation boundary experiment

A separate test constructs production V2 descriptors, stages a pending caller
call, and reconstructs the callee. It first proves that ordinary capture and suspension
publication return `InvalidProgramCounter` and leaves logical state unchanged.
It then tests the existing private continuation storage operation directly.

The stored segment contains both Frames. A precise collection with no native
stack scanning retains a value reachable through the detached V2 roots. Restore
preserves descriptor allocations, PCs, root-state PCs, pending edges, return
destinations, values and scratch state. Old Frame handles become stale, as
continuation capture already requires. A second restore is rejected as consumed.

This isolates a publication/integration gap from representational compatibility.
It is not a successful language-level suspend/resume journey, delayed reentry
through an arbitrary C activation, or a new pin/unload test. Supporting V2 capture
and suspension through normal compiler/Machine operations requires explicit
resume/result transition semantics, not simply deleting the V1 check.

## 6. Tenet assessment and next step

| Tenet / invariant | Classification supported by the experiments |
| --- | --- |
| Explicit stackless Frames and driver | Demonstrated compatibility for one reconstructed callee and original caller. General call-chain reconstruction is implementation work, not established by this test. |
| Full malleability and reflective state | Demonstrated at tested boundaries; speculation must stop or reify before arbitrary observation. Exposing absent state is a fundamental conflict for that optimization. |
| Native participation | Compatible with concrete call boundaries and existing barriers. Recovering an opaque native activation without its cooperation is outside the supported model. |
| Id expressibility | Bounded policy/data demonstrated through ordinary and source-independent Id compilation. Production admission and full compiler policy remain gaps. |
| Identity and aliases | Single materialization can preserve a virtual identity; replacing an already escaped identity or splitting aliases is fundamentally invalid. General materialization graphs remain untested. |
| GC and allocation effects | Recovered/detached roots survive precise GC and callback mutation in the probes. Erasing already observable allocation/finalizer effects is invalid; general allocation-elision eligibility remains unresolved. |
| Code-generation lifetime | Existing registration guards retain active generations through replacement; cleanup releases pins. Existing unload regressions cover actual native mapping retirement. |
| Suspension / continuations | V2 storage is compatible in the bounded test; normal V2 action publication is an implementation gap. Native barriers remain required. |
| Frozen ABI and durable formats | Authorities remain byte-for-byte unchanged; all new recovery access is test-only. No production transport is implied. |
| Concurrency / owner authority | Experiments stay on the existing owner thread. Deoptimization neither supplies concurrency nor requires changing that model; parallel safety is not qualified here. |
| Native speed | Unanswered by this feasibility work. Correct reconstruction does not establish worthwhile optimization or amortization. |

Proceed with a bounded **production recovery-contract design and implementation**
before adding general inlining or scalar replacement. It should admit an
Id-described recipe against owned compiler metadata, validate and transfer roots
and code pins atomically, and define the V2 continuation transition. Its first
consumer should use an optimization whose omitted observations are explicitly
proved safe. Retain the adverse cases here as admission criteria. Performance
testing follows that semantic gate; hotness/preparation tuning remains a separate
74N follow-up.

This assessment does not find a demonstrated need to sacrifice a core tenet for
deoptimization. It does find hard limits on optimizations that cross observable
identity/effect boundaries, and it leaves general-case eligibility and production
admission unproven. A bounded experiment cannot establish universal compatibility.

## Completion audit

| Predeclared requirement | Evidence / scope |
| --- | --- |
| Missing logical callee, production layouts, real driver, caller/return state | Section 1; nine paired matrix cases, depth counters and independent baseline execution. |
| Observation, native reentry, Machine/lookup, GC, capture barriers | Section 2; paired observations plus two deliberately wrong native controls. |
| Object identity, aliases, allocation/collection counterexamples | Section 3; real Id identity sends and explicit finalizer checkpoint comparisons. |
| Inspectable recovery data and Birnam-authored policy | Section 4; compiler facts, ordinary/portable producer outputs, consumed runtime recipe. |
| Invalid maps preserve prior state; code lifetime | Seven full-arena snapshot rejections, active-retirement/cleanup assertions, existing unload regressions in full verification. |
| Debug/release, source receipts, unchanged authorities, full gate | Linked measurement receipt and reproducible runner. |
| Responsibility map, tenet classification, broad recommendation | Sections 4–6 distinguish demonstrated fit, fundamental invalid cases, implementation gaps and unresolved generality. |

The feasibility investigation is complete within these stated bounds. The
unimplemented production mechanisms are findings and recommended future work,
not features this investigation claims to have delivered.
