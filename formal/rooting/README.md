# Duplicate rooting: TLA+ model and Lean proof

This package establishes a conditional rule for compiler root coalescing:
**one scope-owned registration can cover repeated registrations of the same
object until scope exit, provided an owner-managed permission lease authorizes
the omitted effects.** The last covering registration cannot disappear while
its value remains needed. A returned or escaped value must be protected in the
receiving scope before the producing scope releases its protection.

The lease/coalescing model remains a specification: today's public Root registry
does not satisfy that proposed omission contract. The implemented optimizations
use stronger, narrower equivalences instead: exact ordered cleanup batching,
inventory-index marking, and callback-free byte-copy elimination. They are
described in `docs/design/formally-checked-performance.md`. In particular, a live
method/epoch check alone cannot authorize persistent coalescing: observer
visibility can change before the next check.

## Run the checks

```sh
# First use: fetch pinned tools into .cargo-target/formal/tools.
python3 scripts/check-rooting-model.py --fetch-tools

# Subsequent checks are offline and do not build Birnam or run benchmarks.
python3 scripts/check-rooting-model.py
# Equivalent task:
mise run formal-rooting
```

Requirements: Python 3.9+, Java 17+, and `tar`/`unzstd` for the optional Lean
archive installation. The runner supports macOS and Linux on arm64/aarch64 and
x86_64. The first Lean download is roughly 445–460 MB, extracting to about 2 GB.
It installs no global toolchain and changes no shell configuration. Existing
tools can be supplied with `--lean`, `--tla-jar` and `--java`; Lean version and
TLC jar checksum must match `tools.json`. Downloads use official release URLs
and verified SHA-256 digests. Lean 4.24.0 needs only its bundled Std library,
with no Mathlib or third-party Lake dependencies.

Every check uses one TLC worker, a 512 MiB JVM heap, low process priority and a
30-second timeout per command. Results, counterexample traces, mutation logs
and source/tool hashes are written to a new ignored directory under
`.cargo-target/formal/results/`. Successful output names its `summary.json`.
The runner fails on parse errors, timeout, unexpected counterexamples, missing
expected violations, proof holes, custom axioms or successful broken proofs.

The Lean package can also be opened or built independently:

```sh
cd formal/rooting/lean
lake build
```

That command uses the pinned `lean-toolchain` through an existing Lean/Elan
installation. The combined runner instead performs fresh direct Lean checks,
without trusting stale `.olean` build outputs.

## Model and abstraction

`logical[scope, object]` is the number of registrations in ordinary generated
execution. `physical[scope, object]` is the number actually held by the proposed
optimized root-scope owner. These are token counts, not object reference counts.

For a canonical scope, physical and logical counts must be equal. For an
optimized scope, a zero logical count implies zero physical registrations; a
positive logical count requires between one and the full logical number.
Normally it has one. During incremental materialization it can have more.
This invariant preserves the set of rooted objects and therefore transitive
reachability for any reference graph. It is deliberately stronger than the
minimum number of roots needed at a particular safepoint.

The TLA+ state includes a nested call stack, permission witnesses/epochs,
revocation state, and separate canonical/optimized heaps. It admits duplicate
keeps, callback reentry, parent handoff, scope exit/unwind, precise collection,
permission acquisition, and incremental deoptimization. Collection can occur
between materialization steps. All actual root registrations remain abstract
owner operations; the model grants no access to another owner's payload.

`Needed` conservatively includes every logically retained object until scope
exit. It is an input obligation from the ordinary rooting discipline, not a
computed minimal liveness set. TLA+ uses fixed objects and edges; it does not
model allocation, fresh-object ingress, moving GC, weak references, finalizer
execution, concurrency, arbitrary token identities or visitation order. Those
remain separate implementation obligations. There is no conservative native
stack root in this model: missing explicit protection cannot be masked by it.

## The permission and recovery contract

The current Root registry is replaceable through Id, and its `rootsDo:` visits
each retained entry, including duplicates. Thus coalescing is **not** enabled
merely because the built-in `retain:` and `release:` are still installed.
An arbitrary visitor or replacement method could observe omitted calls.

The modeled owner lease requires all of the following:

1. Permission covers root multiplicity and registration/release effects for the
   entire optimized extent, including collection and callbacks. Arbitrary
   observers that distinguish those effects cannot run under that lease.
2. The generated scope owns the tokens it coalesces; token ids do not escape.
   It retains the logical registration information needed for recovery.
3. Each optimized reuse checks current permission and its owner witness/epoch.
   Scope entry can acquire permission only with no existing registrations.
4. A requested revocation starts a barrier. The old authorization remains valid
   while missing logical registrations are materialized through owner bookkeeping.
   Collection remains safe; application actions in the TLA+ model wait.
5. Only after physical registrations equal logical registrations may revocation
   become observable. Subsequent operations take the canonical path. Recovery
   must not replay previously omitted arbitrary user callbacks.

This is a proposed stronger owner contract, **not an existing Birnam API**.
All ways of changing relevant behavior must respect that barrier, including
Id method replacement, permission verdict changes without an epoch change,
custom lookup and changed collector/visitor behavior. If they cannot, the
compiler must decline coalescing. Full equivalence of concrete token ids,
swap-remove visitation order, hook failures and event traces must be shown by
the eventual implementation; equality of modeled counts alone does not prove it.

## What TLC checks

The ordinary and optimized counts evolve in the same transition system. TLC
compares root reachability and precise-GC survivors, checks protection of every
needed value, validates representation/permission invariants, and requires
canonical multiplicity whenever omission is not authorized.

| Configuration | Required result |
| --- | --- |
| `Safe` | All invariants; 2 objects, depth 2, 2 copies, 2 epoch changes |
| `Canonical` | Ordinary execution only, with no optimization permission |
| `Graph` | Same checks with cyclic references between the two objects |
| `Nested` | Same checks at call depth 3 |
| `Recovery` | Safety plus eventual revocation completion under weak fairness |
| `OmitFirst` | Counterexample: missing the only root loses needed reachability |
| `EarlyRelease` | Counterexample: dropping a covering root before scope exit |
| `IgnorePermission` | Counterexample: root multiplicity changes without permission |
| `BadRevoke` | Counterexample: epoch/permission change before materialization |
| `LeakExit` | Counterexample: a forgotten release retains an otherwise dead object |
| `UnfairRecovery` | Counterexample: recovery can stall forever without progress fairness |

The negative configurations deliberately change the algorithm or its progress
assumptions. They are required failures, not failures of the correct model.
The runner checks the specific violated property and a counterexample trace.
TLC exhausts these finite state spaces; that is not an unbounded theorem.
Weak fairness means that continuously enabled recovery eventually executes.
Neither fairness nor deadlock freedom of a concrete owner is assumed proven.

## What Lean proves

`Rooting.lean` has no bounds on object/scope count, reference-path length,
registration multiplicity or finite event-trace length. Its state invariant
allows partially materialized scopes. The main results are:

| Theorem | Established property |
| --- | --- |
| `root_equivalence` / `reachability_equivalence` | Coalesced and logical roots reach exactly the same objects |
| `collector_equivalence` | Identical precise-GC survivor predicates for any heap and graph |
| `needed_survives` | Canonical compiler liveness coverage transfers to optimized GC |
| `guardedKeep_preserves` | Checked reuse or canonical fallback preserves the invariant |
| `guard_failure_restores_exact` | A refused/stale guard restores exact counts before continuing |
| `materializeOne_preserves` | Each guarded partial materialization preserves root coverage |
| `materialization_progress` | Each added registration strictly reduces its count deficit |
| `changePermission_preserves` / `revocation_restores_exact` | Permission changes restore canonical counts first |
| `clearScope_preserves` | Scope cleanup maintains equivalence; handoff remains the compiler's obligation |
| `run_preserves` / `compiler_gc_equivalence` | The rule composes over arbitrary finite event sequences |
| `nonessential_iff_coverage` | Under prior coverage, a change is lifetime-safe exactly when it destroys no needed reachability |

The pure specification's `acquire`, `step` and `run` are noncomputable over
arbitrary object types because testing an entire scope for emptiness needs a
classical decision. They are mathematical definitions, not an extracted runtime
implementation. The guarded count updates themselves are executable definitions.

`Examples.lean` checks duplicate reuse, stale guards, exact deoptimization,
partial materialization and parent handoff. It proves concrete counterexamples
for omitting the first root and changing permission without recovery.
`Audit.lean` reports kernel dependencies of every theorem. The runner admits
only Lean's standard `propext`, `Classical.choice` and `Quot.sound` axioms; no
`sorryAx` or project axioms are allowed. It also mutates first-root creation,
scope release and recovery and requires Lean to reject all three broken programs.

The compiler-liveness premise is explicit. We have not proved the Birnam
compiler's control-flow analysis, actual C/Rust code, Id dispatch, object-owner
tracing or the proposed lease implementation. This is a reusable proof of the
transformation contract, not a claim of a verified production compiler.

## Which operations are essential?

| Operation | Conclusion within this scope |
| --- | --- |
| First covering retain for a still-needed value | Essential when no alternative root reaches it |
| Repeat retain in the same scope | Lifetime-redundant with covering protection; omittable only with authorization |
| Retain in a nested call | Not established redundant; this model keeps separate scope ownership |
| Handoff before producer cleanup | Essential if the result otherwise loses its last root |
| Release of a redundant logical registration | Can coalesce with the scope's final release under the same authorization |
| Final scope release after the value becomes dead | Required to preserve reclamation; omitted release can leak |
| Materialization before observable revocation | Required for restoring the canonical count contract |
| Epoch/permission query alone | Insufficient without the owner revocation barrier |

The 70.4% overlap census is still an opportunity estimate. These results do not
prove that 70.4%, or even the same-activation 26%, of today's sends can be removed.
The next implementation slice must establish actual generated `Call` scope
lifetimes, an owner authorization/revocation protocol, concrete observable
behavior and forced precise-GC regression tests. Until then, production preserves
logical registrations and cold-bootstrap Rust requirements. Guarded cleanup can
use one owner message for the exact reverse-ordered releases; it does not merge
logical root registrations.

## Source correspondence and references

- `rust/birnam-native-backend/src/native_prelude.c`: `keep` creates a token for
  every Object and `finish` releases scope tokens. The candidate rule applies
  to that owned scope; message-activation overlap is not itself a scope proof.
- `rust/birnam-root-registry/src/lib.rs`: unique token creation, exact release,
  swap removal and callback-visible `rootsDo:` enumeration.
- `rust/birnam-collect/src/collection.rs`: explicit roots and transitive tracing.
- `docs/design/60f-resumable-compiler-lowering.md` §7: safepoint liveness and
  handoff requirements. This package does not replace its Frame-root analysis.
- `docs/design/integer-performance-investigation.md`: motivating measurements.
- [TLC tools](https://lamport.org/tla/tools.html?unhideBut=hide-tlc&unhideDiv=tlc),
  [pinned TLA+ release](https://github.com/tlaplus/tlaplus/releases/tag/v1.7.4),
  [Lean reference](https://lean-lang.org/doc/reference/latest),
  [pinned Lean release](https://github.com/leanprover/lean4/releases/tag/v4.24.0).

## Production-oriented extensions

- `BatchCleanup.tla`: exact swap-removal order and custom callback traces, with
  replacement between guard and execution and between fallback sends. `BatchSafe`
  passes; `BatchStale` and `BatchOrder` must fail their specific properties.
- `GC.lean`: reverse-order batch equivalence for the complete root state, and
  equivalence of hash-set marking to indexed flags under an injective immutable
  allocation index. Admission order, LIFO pending order and dead inventory order
  are preserved, not merely the final reachable set.
- `BorrowInspection.tla` / `Borrow.lean`: copied and borrowed inspection agree
  while the helper runs without callbacks. `BorrowCallback` must fail if a
  callback can mutate the borrow before extraction of owned shape scalars.
- `Memo.lean`: a reusable exact-key pure memoization proof. The initial byte-key
  cache was rejected after measurements showed a regression; this proof is
  retained for future candidates, not claimed as a delivered runtime cache.

These proofs still do not verify all Rust, the standard library allocator, Id
introspection or generated C machine code. Implementation-side differential tests
and current-owner replacement/precise-GC tests bridge those boundaries.

## Exact registration batches

`RegistrationBatch.tla` / `Registration.lean` cover acquisition in input order,
exact sequential ids, null/capacity rejection before mutation and ordinary
prefix failure on fallback. Required failures demonstrate unrooted incoming
values and partial mutation on a declined batch. The caller must protect input
Oop owners for the invocation extent; the compiler uses incoming receiver and
arguments, and Word uses its invocation inputs. No interim message result is
queued unprotected across another send. Capacity is rechecked after any proof
dispatch can reenter.

Id `nativeBindingsMatch:` consumes native-endian Word quadruples (receiver,
selector, method address, opaque context address), not a portable descriptor.
It reads only Id-owned metadata and declines custom lookup without calling it.
A successful proof and the immediate Root mutation have no intervening foreign
send. Root owner APIs keep the complete logical token sequence; physical root
coalescing and its stronger observer lease remain unimplemented.

## Capture-result coverage

`Capture.lean` proves reachability equivalence when additional result roots are
already reachable from covering roots, including a live Block's capture edge.
Fresh results need a separate coverage premise. `CaptureReads.tla` checks pure
borrowed reads, protected replacement results and cleanup, with required failures
for invalid grants, missing fresh roots, early scope exit and broken edges.
The receiver-query slice adds transitive receiver coverage and pure read-batch
survival theorems. The bounded model includes direct and factory-fallback routes,
roots the factory only through the fallback query, and checks two invocations
with changed reader/query bindings. `CaptureCached` must lose a fresh result when
the second invocation reuses an earlier positive grant. The factory result and
Id result handoff remain ABI premises; actual query failures/type fallback are
covered by native integration tests rather than encoded as a full dispatch model.
The runtime correspondence and limits are recorded in
[the capture-access result](../../docs/design/capture-access-simplification.md) and
[the receiver-query slice](../../docs/design/capture-receiver-query.md).
These are abstract models, not a proof of native providers or the emitted ABI.

## Bulk capture handoff

`Capture.lean` adds list identity-copy/order/alias preservation, receiver-covered
batch survival and an admission lemma requiring the current bulk provider itself.
`BulkCaptures.tla` checks collection, duplicate slots and overwriting the owner
buffer after copying. `BulkAdmission`, `BulkCopy`, `BulkOrder` and `BulkScope`
require counterexamples for stale admission, delayed copying, reordered slots and
premature owner release. The Lean mutation dropping the bulk-provider check must
fail kernel checking. This models a serialized borrowing contract; it does not
verify native byte encoding, C `memcpy`, arbitrary provider promises or concurrent
mutation. Actual encoding, immediate copying and fallback failures are checked by
native integration tests. See [the contract](../../docs/design/bulk-capture-handoff.md).

## Fixed names and protection within a call

`SymbolNames.lean` proves that retaining the correct Symbol identity permits
dispatch in any current world, and that an owner root/edge keeps the Symbol alive.
It does not preserve repeated internal interning hooks or authorize caching a
selected executable. Namespace, Execution, Word and Codegen retain fixed helper
Symbols; every actual operation still uses current Id dispatch. Explicit source
`intern:` operations and dynamic-name fallbacks remain operations.

`LocalReuse.lean` proves reachability equivalence for already-covered aliases,
coverage after a successful first acquisition, safe eviction of a hint without
dropping its root, and collection survival. It rejects treating a stale pointer
after scope exit as protection. `LocalReuse.tla` checks two activations with a
one-slot cache to exercise eviction: unrooted hints, dropping roots on eviction,
stale hints in the next call, broken capture edges, early release and leaked
cleanup all require counterexamples. The generated cache has 32 entries; bounded
eviction changes only optimization opportunities, not lifetime coverage.

This models compiler-private lifetime protection, not changes to explicit program
Root operations or a general permission lease for public token coalescing.
First-result acquisition/handoff, collector contracts and immutable admitted
capture edges remain premises. Integration tests force precise GC with fresh
locals and capture aliases, exceed the cache capacity, check cleanup across calls,
and verify failure/zero-token acquisition prevents later body operations.

## Reusable branch handlers and native input coverage

`Branches.lean` relates the nested Integer decision tree to shared handlers with
explicit parameters. Under default branch meanings, it preserves operand/error/
Float checks, overflow decisions, result category and retained-operation order,
including noncanonical truth outcomes. It also instantiates `Invocation.lean`'s
unbounded coverage rule for native numeric inputs. This does not prove signed
arithmetic or arbitrary replacement handlers; the numeric oracle and replacement
tests supply those runtime checks.

`ReusableBranches.tla` checks selection without invocation, current handler lookup
after replacement, argument protection through callbacks/collection, failures and
no replay. Running both branches, caching an old entry, dropping argument cover
or replaying a completed effect each requires a counterexample. Existing invocation
negative cases reject missing input coverage, early scope exit, unprotected fresh
results and lost entered-code lifetime. Native tests collect inside numeric
validation: receiver/arguments stay alive without extra numeric tokens, while
freshly allocated outputs need their first root before that callback.

## Owner scopes and root-free final-result handoff

`Invocation.lean` now compares an exact owner-reported temporary inventory with
registry protection, proves collection coverage and nested suffix cleanup, and
requires zero pending tokens for a direct final-result return. The compiler
resolves the target selector before checking that condition. With no local
cleanup, no further callback can intervene between the final Id result and its
caller. Calls with local roots, cold-cache registrations or failures keep the
existing path. `SymbolNames.lean` supplies the existing Unit-edge coverage rule
for reusing retained literal Symbols, without retaining executable bindings.

`NativeOwnerScopes.tla` checks precise collection across nested owner scopes,
child and parent exit, and immediate result handoff. Required broken models omit
reported locals, pop a parent too early, drop parent protection with a child,
leak scopes or introduce a collecting handoff gap. Root-provider reporting and
serialized calls remain implementation premises, not proofs of arbitrary native
providers or concurrency. Owner/registry callback histories and multiplicity are
outside this private-lifetime equivalence; authored Root operations are unchanged.
The direct-return model does not verify the unchanged nonempty cleanup fallback.
See [the implementation](../../docs/design/helper-lifetime-and-boxing.md).

## Atomic representation checks and borrowed native inputs

`Admission.lean` adds metadata-query admission, positive identity/shape soundness,
unknown-identity refusal and unchanged legacy-result selection on fallback.
`RepresentationChecks.tla` checks current getter/query meanings, immediate raw
reads, fresh admission after mutation and legacy getter order. Required failures
ignore getters, ignore the query's own binding, reuse a verdict, insert a callback
between certification and reading, or reorder fallback getters.

`BorrowedSnapshots.tla` distinguishes caller-covered Map/key inputs from a local
node snapshot. Rehash replaces the parent edge, so retaining the Map alone cannot
protect the old node. Missing caller roots, missing snapshot protection and early
snapshot release must fail. Native tests collect during rehash and verify the
old node survives the lookup and becomes collectible afterward. Existing
invocation/input and root-free handoff lemmas apply to Map inputs, Symbol hashing,
fixed reflection/permission queries and generated named-method adapters.
These are conditional abstract rules; native provider promises, byte reads and
runtime correspondence are implementation obligations. See
[the implementation](../../docs/design/representation-validation-and-lifetime.md).

## Operation selection and global resolution

`Branches.lean` checks direct operation receivers against the former five-Integer/
four-Float handler tables, and verifies the same default source-resolution result
and getter order. `GlobalResolution.tla` checks saved nil/Globals references while
callbacks rebind both exports. Required failures omit either root, compare against
a newly rebound nil, end protection early or reorder getters. Existing invocation
and branch proofs remain in use; public retained calls still dispatch through Id.
See [the slice and remaining candidates](../../docs/design/operation-dispatch-and-global-resolution.md).
