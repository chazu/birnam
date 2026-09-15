# 79C — Materialization ownership and failure inventory

Execution inventory for Design 79D. Source at entry: `2a9d9422` (the Codegen
runtime differs from the original review only by A/B). This is a gate/evidence
record, not a change to the language or lifetime contracts.

## Owners and observation points

TCC owns each Unit's native compiler/relocation storage. `disposable` permits
finalization; it does not mean immediate physical release. The floor owns
resumable registration, active-generation exclusion and retirement eligibility.
Codegen's registries own explicit Oop leases and publication state. A generation
integer cannot replace a Unit root. Ordinary native Methods retain their existing
no-metadata dispatch and barrier contract.

The three compilation paths deliberately have different setup and failure
ordering. A shared compile/bind/register transaction is **not justified** by
this inventory; D4 will retain these bodies separately.

| Entry / stage or failure | Unit, registration and roots | Locks / callback boundary | Compensation and release |
| --- | --- | --- | --- |
| Synchronous: namespace, portable compilation, source-size or missing TCC refusal | No Unit yet; pure owned bytes only | No registry guard | Return `None`; no disposal or registration |
| Synchronous: `new`, compile, shims, runtime binding, relocation, identity binding | Newly created TCC Unit is a local Oop across the existing calls; no materialization row yet | Sends and String construction remain in the original function/stack extent | Invalid `new` returns; subsequent failure calls `dispose_unit` |
| Synchronous: owner/entry symbol, cache admission, zero entry, Block vtable creation, staged-vtable update | Generated owner word contains the real Unit; staging roots Unit and, once set, vtable | Cache guards end before allocating/sending; symbol access stays at its original stage | Pre-stage failure requests disposal; post-stage failure removes row then requests disposal |
| Synchronous: `disposable` refusal / successful return | Existing staging row and native vtable association; returned `(vtable, unit)` is committed by the caller | Preserve original method/layout installation before `disposable` | Refusal removes materialization; do not add an extra disposal call |
| Named: plan/capability/new/generation/admission refusal | Native Unit becomes a CodeBinding staging root **before** compile | Registry admission ends before String allocation and sends | Failure before staging disposes; failed staging disposes without removing another row |
| Named: compile/bind/relocate/identity/own-generation/lease/begin failures | Staging CodeBinding roots target, vtable, selector, implementation, provenance and Unit | No binding guard across these sends or registration calls | Remove staging then dispose; preserve each early return and no-cancel-before-begin rule |
| Named: context/prepare/end/method-staging failure | Floor staging is active; CodeBinding staging still roots native owner | Existing generated callbacks, in existing order | End context after failed prepare; cancel failure aborts; otherwise remove staging then dispose |
| Named: finish or disposable failure | Registration may be committed; Unit cannot be treated as unregistered scratch | Binding mutation and retirement callback remain separate scopes | Mark staging row retiring, request floor retirement, retain until quiescence |
| Resumable Block: lookup/plan/new/compile/bind/relocate/cache-stage failure | Local Unit before cache staging; explicit staging root afterward | This path stages **after** compilation, unlike named materialization | Existing disposal only; no attempt to make ordering uniform |
| Resumable Block: lease/identity/own-generation/begin failure | Cache staging holds Unit; generated registration not yet committed | Registry guards end before API/capability calls | Remove staging then dispose |
| Resumable Block: context/preparation/vtable/capability/new-block failure | Owner carrier is written before nested Blocks escape; staged vtable and Unit roots cover allocation | Preserve `captures` borrow through the synchronous capability call | End where currently required; cancel failure aborts; otherwise remove staging then dispose |
| Resumable Block: finish/disposable/cache-commit failure | Floor may have committed; cache and real Block carriers remain distinct owners | Keep original finish/disposable/commit short-circuit ordering | Remove cache then dispose; do not adopt named-path retirement policy |
| Image binding: stage absent, wrong index/state, malformed inventory, invalid/zero entry | Mapped or rebuilt image stage owns the carrier | Entry-address getter currently runs while the image-stage guard is held; preserve this | Return nil before Store publication |
| Image binding: private-vtable/reservation/Store failure | Committed image stage keeps owner live while binding is installed | Reserve under binding guard, release it, then Store `addMethod:on:with:` | Refusal leaves image stage; reservation and publication must remain separate operations |
| Image binding: post-Store binding publication | Current row acquires semantic/native graph; predecessors become retiring | Second binding guard is acquired after Store returns | A missing binding registry now aborts; do not convert it to an ordinary pre-publication refusal |
| Image finish / cancel | Rebuilt carrier is a TCC Unit; mapped carrier has a mapping finalizer | Stage is taken before context/registration callbacks; restoration and poison recovery stay path-specific | Preserve `end`, `finish`/`cancel`, and release ordering; committed owner finalization differs by carrier kind |
| Cache eviction / failed admission | Ready entries are cache leases; staging entries remain rooted; live Blocks retain their own carriers | Cache lock covers selection/removal, then artifact-cache cleanup/retirement runs outside it | Admission can evict ready rows and still fail. Do not make it transactional or clear Unit-owned caches |
| Binding replacement / withdrawal / reaping | Staging/current/retiring rows retain the same six Oops | Snapshot retiring generations; release guard; ask floor; reacquire and remove confirmed rows | GC runs only after floor-confirmed retirement. Unknown/rejected retirement never authorizes release |
| Candidate activation / rollback / discard | Candidate, prepared-token, initializer-write, capture-root and retained-generation registries are separate owners | Their individual mutexes and each callback's current scope remain separate | Store compensation reservation/consumption and installed-code retirement stay explicit; no destructor performs fallible compensation |

## Registry access inventory

These are the production access sites at entry; tests are excluded. Moves must
leave each registry private to one owner and expose operations, not its mutex.

| Registry | Access sites / intended owner |
| --- | --- |
| `MATERIALIZATIONS` | Root segment; `cached_materialization`, `stage_materialization`, `set_staged_materialization_vtable`, `commit_staged_materialization`, `remove_materialization`, count and eviction methods. Cache module owns these together with age and limits. |
| `CODE_BINDINGS` | Root segment; stage/method/remove/retire/publish/withdraw/provenance/current-implementation operations; image-install reservation/publication; reaping; count/byte telemetry; image snapshot. Binding module owns storage, with image orchestration using separate reservation/publication operations. |
| `MODULE_SOURCES` | Root segment; native/Birnam source recording, source lookup, artifact source registration, registered/ordered generation sources, initial authority/projection. Source owner must preserve the long-lived root-callback guard. |
| `CANDIDATES` | Root segment; successor projection, inspection, managed-method change, map/query/content path, prepare/commit activation, effective operations/writes, reconciliation, composition adoption, image-dispatch recovery, prepare/commit rollback, discard/release. Publication owner. |
| `PREPARED` | Root segment; prepare/commit activation and rollback; discard prepared. Publication owner; token scope remains process-local. |
| `INITIALIZER_TRANSACTION` | Root segment; begin/take transaction; `birnam_global_set`. Publication owner retains compensation state and original ordering. |
| `CANDIDATE_ROOTS` | Root segment; generated root set/get; take roots. Publication capture owner. |
| `VTABLE_CACHE_ROOTS` | Root segment; generated cache get/set; take/restore live cache roots. Keep artifact roots distinct from Unit-owned caches. |
| `RETAINED_GENERATIONS` | Root segment; reap, dispose-or-retain, count and byte telemetry. Publication lifetime owner; floor confirmation precedes physical release. |
| `PORTABLE_IMAGE_STAGE` | Root segment; begin/prepare/finish/cancel/install image operations. Image recovery owner, preserving explicit carrier-kind policy. |

## Root callback contract

The single registered callback visits Runtime fields, live Units, materializations,
CodeBindings, image stage, module sources, candidates, initializer transaction,
retained generations, prepared plans, candidate captures, artifact vtable-cache
roots and active generation, in that order. Preserve each element's order,
zero filtering, lock failure behavior and registration count.

`MODULE_SOURCES` is special: its guard lasts through every later visit. Failure
to lock returns from the **whole callback**. A source helper that simply returns
to its caller would violate this contract. A later source extraction must either
retain the guard across delegated continuation or leave this segment in place.

## Evidence and remaining gates

- New `codegen-state` characterization passes in three fresh test processes:
  LRU/failed-admission effects and staging retention; binding publication and
  retained predecessor/successor roots; exact root visit order and source-guard
  duration, including intentional source-lock poisoning and whole-callback exit.
  Receipt: `/tmp/birnam79-d/baseline-state.json`.
- These tests use opaque pointer tokens with a recording mark callback. They do
  not invoke the collector on those tokens and make no native-allocation or
  actual GC-lifetime claim.
- Normal native baseline passed on A's rebuilt prefix. The route includes
  portable resumable, CodeBinding and image-v8 lifecycles, stackless control,
  ordinary native block/API contexts, mutation, capture, eviction and GC.
- `portable-code-binding-lifecycle-smoke.sh` uses separate REPL turns to prove
  publication/replacement/self-removal and retirement. `stackless-control-smoke.sh`
  exercises the 100k callback loop in normal and source-recovery entry.
- The complete D series still requires fresh release/language acceptance and
  the existing evaluator/native-storage matrices. Source-identical function
  moves will be checked independently by parsed declaration/method tokens,
  including bodies, literals and local scopes. No compile/publication setup is
  merged on that evidence; D4 remains deferred because the table proves differing
  staging, cancellation and retirement behavior.

Current gate: storage/root characterization is sufficient for bounded D1/D2
ownership extraction. Remaining path inventories and their integration evidence
must be closed before the broader D series is accepted. Passing these tests
cannot authorize new roots, new native metadata requirements, effect replay,
new allocation accounting, broader layout restrictions or optimizer promotion.
