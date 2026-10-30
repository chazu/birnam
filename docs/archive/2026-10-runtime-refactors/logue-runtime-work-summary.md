# Logue runtime work: current state and continuation

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

**Current checkpoint:** the compiler/Execution continuation integration passes
its native/source gate, including the original custom-lookup tail assertion
(2 frames in both modes), lookup suspension/re-entry, interleaving, automatic GC
and native callback boundaries. See the [implementation and remaining scope](oop-continuation-id.md).
Full control/recovery, default compiler orchestration, upper-module migration and
installed runtime qualification remain unfinished. Earlier receipts below retain
only their recorded coverage.

Latest continuation: `Codegen compileResumable:` now runs through the retained
Machine frame/registration engine. Bounded runnable stops survive GC and method
replacement without replaying effects, and completed/abandoned frames release
their code mappings. The native fallback was corrected to preserve direct Id
dispatch; custom lookup is not replayed. See the
[resumable milestone and limits](oop-resumable-milestone.md). This remains isolated,
uninstalled work; explicit suspension/control/recovery and full runtime acceptance
are still pending. The entries below describe preceding checkpoints.

Earlier continuation: the explicit synchronous compiler milestone now passes:
real source -> Lang seed/PEG -> existing lowering -> NativeBackend -> TCC ->
actual Execution. It covers collection, live replacement, captures, Error/failure
distinctions, direct dispatch and final mapping retirement. The temporary frame
engine harness and TCC checkpoint stand-in are removed. See the
[integration record](oop-compiler-execution-integration.md) and
[qualified native receipt](../../measurements/oop-connected-compiler-20261023.json).
Ordinary compiler/resumable integration remains unfinished; nothing is installed.
The checkpoints below describe earlier stages.

Continuation, 2026-10-23: the Oop branch is now the committed development direction.
`c23240cf` migrates TCC and validates generated Oop C, collection, captured code,
object loading and mapping retirement. `4b5e8a00` moves the retained frame engine
to Method references plus allocation identities; 22 pure engine checks pass.
See [compiler/Execution integration](oop-compiler-execution-integration.md).
Execution message entrypoints, emitted language C and the compiler consumers still
need migration. These checkpoints do not qualify a complete language or installed
runtime. The narrative below describes the preceding `683c9fc2` checkpoint.

This summary covers the Logue investigation, retained language-wide optimizations,
Oop-only experiments and migration, and the dispatch correction through runtime
commit **`683c9fc2`**. It describes `codex/oop-only-id-slice` in
`/Users/chazu/dev/zig/birnam-oop-id`. The migration is isolated, unmerged and
uninstalled. The complete language is not yet migrated or qualified.

The design authority is [Current Invariants](language-design.md#current-invariants),
with working instructions in [AGENTS.md](../../../AGENTS.md) and terminology in
[glossary.md](../../../glossary.md). Historical implementation reports cannot amend
those requirements. The [Oop-only plan](oop-only-id.md) defines the remaining
acceptance sequence; the [migration log](oop-invariant-migration-20261023.md)
records individual pieces and their receipts.

## 1. What Logue exposed

Slow floor changes were dominated by full-map execution, while occasional movement
hitches correlated with automatic collection. The later installed-baseline
[reprofile](logue-reprofile-20261022.md) observed full-map drawing around
1.2–1.8 seconds, ordinary redraws around 20–22 ms and collection-associated phases
around half a second. Presenting an already-built terminal buffer took about
0.09 ms. These were controlled game/PTY workloads, not keyboard-to-display timings.

Generic probes exposed expensive Range traversal, repeated call preparation,
frame-index boxing and allocation churn. The work targeted shared language and
runtime costs, keeping Logue as an acceptance workload rather than replacing its
application semantics. See the [language-wide diagnosis](runtime-costs-exposed-by-logue.md).

The copy profile counted **250,939,167 copies of 24 or 40 bytes across eight full
redraws**, approximately **31.4 million per redraw**. Those sizes matched Operand
and Outcome. The evidence supports generated ABI packet traffic as the main copy
source; it does not mean that one redraw performed 251 million copies, nor does a
copy count directly measure CPU share.

## 2. Retained work before the Oop migration

These results belong to the earlier installed/runtime baselines. Percentages
compare each slice with its immediate predecessor and must not be added or applied
to the present Oop branch.

| Change | Commit | Recorded result |
| --- | --- | --- |
| Reduce repeated default-lookup checks | `5018aad2` | 4–6% Logue CPU reduction in the original paired workload |
| Collector-owned edge scratch and simpler retirement | `1498a182` | 5.6–14.7% lower CPU in focused collector cases; mixed Logue results, not a solved hitch |
| Traverse live Range bounds directly | `bcd31b31` | Roughly 44% faster empty loops and 31% fewer allocations; measured generation/redraw/roundtrip reductions of 25.1%/14.2%/4.0% |
| Admit call plans once; coarse Machine preparation | `7011ed2a` | Measured generation/redraw/roundtrip reductions of 37.1%/30.1%/34.6%; an authored Birnam implementation exercised the operation |
| Reuse Word-region analysis for nonescaping shared temporaries | `4a398e20` | Focused case 9.5% faster, three fewer allocations; no demonstrated Logue speedup |
| Share immutable frame arrays while retaining binding-local state | `62aeedb7` | Four host allocations per binding instead of eight; deterministic storage savings, no application-speedup claim under external build load |

The accepted Range contract permits traversal to avoid per-iteration `size`/`at:`
sends while retaining current bounds/accessors, mutation, fresh element identities,
errors, suspension and GC. Coarse call preparation retains actual logical frames
and source-level sends. Collector scratch remains in an explicit owner context;
this was not a move of language state onto the Rust stack.

Two later storage experiments were **not retained**: argument-copy normalization
regressed a nullable case, and broader collector workspace reuse did not establish
a speedup. Grouping frame arrays behind another layout object was also rejected;
the retained change shares the arrays directly. Their patches and measurements
remain historical evidence, not implementation instructions.

Evidence: [simplification results](runtime-simplification-results.md) and
[storage experiments](runtime-storage-simplification.md).

## 3. The Oop-only direction and initial proof

The user made removal of Operand/Outcome the priority. Every Id receiver,
selector, argument and result must be an Oop. Do not optimize, rename or box the
old carriers into another universal value representation. Allocation-size and
C-string exceptions were conditional on there being no performant strict option;
the [calling-convention and allocation/interning probes](oop-only-id-probes.md)
did not establish that necessity. A 64-byte name was a probe workload, not a Name
size limit.

The implemented native convention uses an Oop result and a borrowed array of Oop
arguments plus a count. The physical array/count and immutable object-file code
metadata do not form another runtime value system. Bootstrap size immediates and
variable-length Name objects let allocation and interning work before the upper
Integer/String owners exist; failed native returns mark an object reference,
while caught errors remain ordinary objects.

The first native proof (`4c8e5937`) booted Id from explicit native declarations,
loaded independent modules and performed precise GC. Its native microbenchmarks
regressed; it was never full-language or performance acceptance. The subsequent
[diagnosis](oop-id-slowdown-diagnosis-20261023.md), recorded in `c9abff39`, found
repeated allocation/Method/name checks, extra metadata and temporary name work.
It did not establish that GC roots or validation could simply be removed.

`4a17ee7c` removed separate tables for Method data, Fault data, Name length and
reference storage, put contents in their objects, borrowed stable arguments in
active call records, and removed redundant snapshot/retirement work. Its allocation
Storage classification was subsequently rejected and removed in `f0f965ed`.
**The Storage diagnostic module remains.** Removing a payload classification did
not mean removing the module or its observation responsibilities.

Malleability, opaque Id payloads and terminology were made explicit in the project
instructions (`a5034786`, `13797a8d`). Ordinary object allocations are contiguous;
owners may manage separate storage, but escaped Oops cannot simply be moved by
`realloc`. Payload meaning belongs to owners above minimal Id, without a generic
allocation/dispatch/GC type catalog.

## 4. Current migrated native closure

The following implementation commits are checkpoints. Their retained behavior
is subject to the later correction `683c9fc2`; descriptions of removed application
or byte-forwarding machinery in their original notes are superseded.

| Area | Checkpoints | Current retained work |
| --- | --- | --- |
| Id/Siphon/Roots/native loading | `4c8e5937`, `4a17ee7c`, `f0f965ed`, `2e384626` | Oop calling convention, bootstrap size/Name/Method objects, live construction, owner-private module records, entered-call roots and independent native callers |
| Namespace | `0e0b3b0b` | Ordered Oop bindings, alias rules, current lookup and collecting enumeration |
| GC ownership and retirement | `53f5efbf`, `b4face60` | Live/inherited owner changes, marking new owners, failure containment and dead-Oop retirement before release |
| Allocation policy and errors | `eaed1bef`, `b9dc7c94`, `9ba2f35c` | Activation/accounting, live permission checks, and failure-return markers separate from ordinary caught Error references; no per-allocation failure classification |
| Storage diagnostics | `80f8c0a1` | Oop observation, callback lifetime and nested/expired-callback checks |
| Boolean and nil | `1dfd5f62` | Identity, selected branches, current calls and captured printing dependencies |
| ObjectModel | `cf655b68` | Oop slots/layouts, inherited layouts, clone snapshots, private behavior and image-slot transport |
| Word | `e51c3856` | Full-width values and guarded expression evaluation; current data reads and local evaluation after correction |
| Bytes and String | `a4972be8`, `bb174bc7` | Owned byte storage, removed mirrors/backing duplication, GC/finalization, indexed byte access and live constructors |
| Float and Integer | `21890acb`, `72940a14` | Full-width signed/IEEE transport, arithmetic, ordinary errors, mixed operations and Method-based optimization witnesses |
| Vector and Map | `120213f2`, `f8973472` | Persistent structures, owner-local scoped roots, live key/callback behavior, sorting, snapshots and rehashing |
| Prelude/System | `f1201cae` | Ordinary System construction, ownership and repeated-load lifetime |
| Input/environment/output/file I/O | `8aacd64f`, `29091c07`, `9b514a0d`, `b1e58690` | Binary I/O, exact-type policies, lazy errors, filename snapshots, bounds, current constructors and local implementation functions |
| PEG | `e2cd980b` | Oop patterns/grammars/CSTs, scoped graph roots, direct retirement; matcher algorithms unchanged |
| Artifact inspection | `95595bf6` | Ordinary thirteen-slot descriptors, current list/byte operations and scoped roots; descriptor codecs unchanged |

Other important regression fixes included preserving inherited ObjectModel layouts,
full-width image values, nil/absent/failure distinctions, Vector's CollectionVt
parent, metadata cleanup before address reuse, and filename snapshots before
contents callbacks. The detailed log records their tests and limitations.

## 5. The rejected application protocol and its removal

The implementation incorrectly introduced `apply` as a required lookup on every
selected Method, then extended it into argument/byte forwarding and private
callback/evaluator Methods. It also added a separate native-Method metavtable.
Tests that exercised those mechanisms were treated as architectural evidence.
That was an invariant violation, not an accepted tradeoff for Oops or opaque payloads.

**`683c9fc2` removed that design.** The ordinary path is now:

```text
send(receiver, selector, arguments) -> current method lookup -> selected implementation
```

There is no Id `apply` protocol, renamed equivalent, or forwarding family. The
extra native-Method metavtable and application-dependent Method checks are gone.
Byte consumers and evaluators are local owner functions using ordinary indexed
data messages. The existing explicit captured-Method reflection operation handles
its bootstrap owner's own argument Sequence; it is not an implicit dispatch stage.

The old generic argument-application change (`6d338d8b`) and extra-metavtable
solution (`bda60da3`) are superseded. Tests endorsing them were removed or replaced
with tests of actual data access and ordinary sends. The proposed TCC callable
work was not implemented; it is not an approved design to resume later.

The new contract test confirms that no `apply` method is installed, then makes
**all lookup on the selected Method fail**. An ordinary send must still succeed.
This catches a second lookup regardless of its selector's spelling. The invariants
and AGENTS instructions now require a before/after operation sequence for core
changes, explicit identification of new required mechanisms, and tests of forbidden
alternatives. Passing implementation tests or agent-written notes cannot authorize
an invariant change. See the [correction](id-send-correction-20261023.md).

## 6. Latest acceptance evidence and its limits

The [correction receipt](../../measurements/id-send-correction-20261023.json) records
`bash scripts/oop-id-slice-test.sh` passing for the corrected implementation. It
builds the migrated native closure, audits actual exports, and exercises independent
C and TinyCC callers, global/metacircular lookup, DNU, replacements, entered and
captured lifetimes, tracing/finalization/retirement failures, allocation policy,
callback GC, values/collections, real binary I/O, PEG and artifact inspection.
It also requires `DISPATCH_CONTRACT_OK`. See [validation](validation-history.md#oop-only-native-migration).

This is not a complete workspace build, authored-language/bootstrap run, installed
candidate, image/worker qualification or a Logue performance result. The Error
provider in the native integration fixture is independent C, not qualification
of the authored Error module. TinyCC caller coverage does not establish that the
TCC owner, generated language code or reclaimable compiled mappings are migrated.

The older paired timings remain useful only for their recorded revisions and
workloads. There is no new paired whole-language/Logue speedup claim for `683c9fc2`.
Indexed byte copying includes per-element sends, so its cost is a remaining
measurement concern. Do not restore the rejected protocol to hide that cost.

## 7. Continuation boundary

1. Start from the current invariants and this committed state, not a superseded
   application-protocol description. Preserve the dispatch contract test.
2. Finish the remaining bootstrap/compiler/execution work together where their
   signatures and ownership interact: TCC, native backend, Codegen, Generation,
   generated C, portable code, Execution and the language seed. Preserve actual
   logical frames, suspension, recovery, live lookup and entered code ownership.
3. Finish persistence, native-code lifetime, host/project/CLI and worker/Isolate
   integration, including remaining Store/Relational and Alien consumers. Rebuild
   incompatible generated code and images explicitly; do not restore carriers as
   a compatibility shortcut.
4. Run the full plan's cold-checkout, language, image, worker, code-lifetime and
   installed-candidate acceptance checks before merging or installing the cutover.
5. Only after the migration is complete, recover broad performance and reprofile
   Logue. Continue fixing obvious duplication/regressions between coherent commits;
   do not weaken invariants or conceal shifted complexity.

Maintain the requested cadence: implement a coherent piece, check it, review
obvious regressions, record evidence and commit. A native slice pass must never
be reported as completion of the whole language.
