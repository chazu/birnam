# Behavioral semantics and owner-message cost

This package builds reusable foundations for Birnam optimization. Addition is
one acceptance example, alongside boxed String byte length and Vector access.
It preserves the organizing rule: Id owns the floor and foreign owners remain
opaque. A cheaper implementation cannot bypass that rule by reading another
owner's payload or collecting arbitrary language policy into Id.

Run `mise run formal-rooting` from the repository root. It now checks both this
package and the existing rooting package, using pinned tools, one TLC worker,
fresh Lean compilation, kernel axiom audits and required negative cases. The
command downloads nothing by default. `scripts/check-rooting-model.py
--fetch-tools` prepares pinned tools when needed. The standalone Lean package is
also buildable with `lake build` and its local rooting dependency.

## Observation contract

`Semantics.lean` defines observation data for lookup, mutation, identity,
allocation hooks, complete ordered root snapshots, frame snapshots, finalizers,
user effects, errors and returned values. A frame observation includes entered
generation and continuation data; root snapshots include order and duplicates.
Observation traces must match in order and content. A simulation relation also
specifies which public state and lifetime facts must match.

The producer of a proof must instantiate that relation honestly. The framework
does not silently declare Root events or allocation effects unobservable, nor
prove that arbitrary language programs cannot observe them. The finite total
Action model is for bounded regions; divergence and an unbounded language
semantics still require an extension. Choosing a weaker observation contract
changes the theorem, not the language's obligations.

## Dispatch and validity

A certificate lists relevant binding facts and required owner permissions.
Validity means current meanings equal their captured meanings and current
permissions are granted. Binding facts must include all resolution dependencies:
lookup/metabehaviors, relevant parents, selectors, receiver association and
namespace/factory bindings where the candidate depends on them. The current
runtime's global epoch is a conservative invalidation implementation, not the
mathematical definition of validity.

Theorems establish:

- Certified dispatch selects the captured meaning while its dependency facts hold.
- Unchanged relevant facts preserve validity, even if the global epoch changes.
- Changed relevant meanings invalidate the certificate.
- Revoked permissions invalidate it even with an unchanged epoch.
- Valid root coverage protects inputs using the existing reachability theorem.

The model does not infer method purity, footprints, or truthful permissions from
a name. An owner must supply that contract; the compiler must retain fallback
when it cannot establish it. A message is a potential callback boundary unless
lookup and the selected owner operation are both established as callback-free.
Allocation, collection, custom lookup and reflection cannot be assumed pure.
Data-read dependencies also matter: unchanged method meaning alone does not
license caching a mutable String length or collection element.

## Refinement, composition and recovery

An action returns state, observations and an actual-send cost. `Refines` requires
a maintained simulation relation, equal observation traces, and a non-increasing
send count. `compose_refinement` and `sequence_refinement` prove that individual
rules compose under the same relation. A proof for equal return values alone is
insufficient; `visible_effect_cannot_be_erased` demonstrates that explicitly.

`recovery_preserves_prefix` resumes a reference continuation from the current
recovered state while keeping the already committed observation prefix. It does
not replay the prefix, allocate historical identities, forge accounting or
synthesize past user callbacks. Faithful concrete recovery is an explicit premise
that must be proved for each runtime representation. This package does not enable
Frame or Block allocation omission.

The TLA+ region model schedules guard acquisition, callback-free owner batches,
materialization before exposure, observation, mutation, permission revocation,
resume/revalidation and precise collection. The entered code generation remains
fixed while future dispatch changes. Its code lease keeps the entered code live.
The `physicalValue`/`physicalPC` fields represent reconstructible execution state,
not permission to invent an object identity or remove a real allocation hook.

## Lower bounds and allowed interfaces

`ownerRuns` counts maximal consecutive runs of responsible owners in a required
operation sequence. `OwnerSchedule` permits one positive-count homogeneous batch
per Id send. The proof `owner_boundary_lower_bound` establishes that every such
schedule costs at least the run count. It prohibits putting foreign operations
into a caller's local calculations or an arbitrary universal service.

`BoundedOwnerSchedule` additionally constrains each owner's batch capacity. This
is important: proving an ideal owner batch safe does not establish that the
current owner interface offers that batch.

| Acceptance kernel | Required payload-owner sequence | Ideal owner-run floor |
| --- | --- | ---: |
| Successful checked addition | Integer reads twice, five unsigned Word operations, Integer result construction | 3 |
| Boxed String byte length | Bytes length, Integer construction | 2 |
| Collection access | Integer index decoding, Vector-owned access | 2 |

Lean checks witness schedules at those abstract floors. A capacity-restricted
addition witness with unary Integer access uses four sends instead of three.
These counts deliberately exclude entry dispatch, guards, root management,
control/Frames, ABI adaptation, allocation/GC admission, node validation and
possible collection. They are **not proven total-send minima for actual Birnam**.
The arithmetic scenario also assumes exact Integer inputs, valid live Error
classification, successful non-overflow arithmetic and the requisite method/
lookup/observation permissions. Error, mixed-type and observed cases need their
own operation graphs and fallback paths. The Vector scenario fixes the index
convention and receiver/representation validity; it does not remove current
Id metadata validation without a separate admission proof.

No single universal constant bounds addition under arbitrary callbacks or GC.
The useful quantity is conditional and interface-specific: required operations
and owner cuts supply a lower bound, a verified emitted plan supplies an upper
bound, and guard/recovery/collection costs are accounted for separately. The
current operation vocabulary and observing context must stay fixed during a
minimum search.

## Model-checking cases

`RegionAddition`, `RegionString` and `RegionCollection` must pass all invariants.
There is one observation/callback boundary and at most two binding mutations;
TLC exhausts that finite state space without a cost-based state constraint.
Required violations are:

| Variant | What it catches |
| --- | --- |
| `RegionStale` | Resuming an old dispatch certificate after replacement |
| `RegionPermission` | Using epoch equality despite revoked permission |
| `RegionExposure` | An observer sees execution state before materialization |
| `RegionReplay` | Recovery resets already-completed state instead of continuing |
| `RegionCodeLease` | Current code replaces ownership of the entered generation |
| `RegionOwner` | Correct values obtained by violating the owner boundary |

The runner also mutates observation preservation, permission checking and owner
send cost and requires Lean to reject all broken claims. Standard Lean kernel
axioms are audited; no proof holes or project axioms are accepted.

## Connecting the model to current implementation

These are framework proofs, not proofs that all current generated C or Rust
instantiates them. Relevant concrete anchors are:

- `rust/birnam-id/src/id/id_dispatch.rs`: live lookup, inheritance, custom lookup,
  native invocation and DNU. Id owns dictionary/header interpretation.
- `rust/birnam-id/src/id/vtable.rs` and `native_id/objects.rs`: mutation and
  receiver reassociation must invalidate relevant dispatch facts.
- `rust/birnam-execution/src/lib.rs`: invocation scopes and acquired code lifetime.
- `rust/birnam-execution/src/recovery.rs`: concrete recovery/commit authority.
- `rust/birnam-word/src/expression.rs`: owner-admitted pure Word plans.
- `rust/birnam-string/src/native.rs`: String delegates backing storage to Bytes.
- `rust/birnam-vector/src/lib.rs`: Vector owns nodes and index/error semantics.

The optional Id census now records parent/child selector edges and receiver
behavior addresses at message ingress. It reads only Id-owned metadata and
performs no extra sends. These are causal labels, not foreign method-context
inspection or a guaranteed semantic-owner classification. Aliases/custom lookup
can make ingress behavior differ from the owner of the selected method. Detached
or unknown receivers are labelled with behavior zero rather than dereferenced.
The default Id build excludes the tracing state and recording path.

The microbenchmark fixtures `public-integer-retains`, `semantic-string-length`
and `semantic-vector-access` run real current owner interfaces. Edge-count sums
must match selector totals, outputs are checked and every observed root token is
balanced. These untimed probes do not run long benchmarks or rebuild images.
They connect the abstract obligations to real call trees; they do not establish
that the entire call tree is removable.

Next: annotate the measured graph with owner, observation, lifetime and
invalidation obligations; build a small proof-checked lowering vocabulary for
protected regions; validate each emitted candidate against the current source
policy. Exact minimum claims require that vocabulary and a complete feasibility
proof, not just small owner-run counts or an optimization's passing tests.

## First concrete lowering: ABI binding admission

`Admission.lean` connects the framework to Execution's fixed ABI guard. A native
entry includes both method and context. Canonical query meanings, standard lookup
and an exact current entry allow one Id query in place of two separate queries.
The guard is read-only and retains no executable dispatch shortcut. Decline runs
the original guard order; the model explicitly charges the extra failed query.
Thus successful admission saves one abstract query, while declined admission
costs one additional query. Behavioral equivalence and send improvement are
separate claims; non-increasing cost is proved only on the successful domain.

`BindingAdmission.tla` checks current method/context, canonical query services,
legacy callback traces, a later signature/permission boundary and the final epoch
fence. Its negative cases require failures when context or service checks are
omitted, permissions are cached, or the final epoch fence is removed. Canonical
services are an explicit model premise, not inferred from method names.

Concrete correspondence:

- Id `nativeBindingQuerySemantics:for:` identifies only its own known entry in the
  requested query role, including context. GC `internQuerySemantics:` identifies
  its own interning wrapper. These describe immutable code semantics, not mutable
  allocation permissions. Unknown/replaced entries at startup disable the batch.
- Execution `binding_proof.rs` encodes five native Word rows: four original
  services (`usesDefaultLookup:`, `bind:to:`, `intern:`, `symbolNamed:`) and the
  canonical ABI target entry. Every attempt asks Id to validate current lookup,
  reassociation and exact entries. No foreign context is dereferenced.
- The fixed ABI selector is already rooted in Execution. GC's described interning
  wrapper is nonallocating for that existing Symbol while Id's canonical symbol
  query remains selected. Unknown dynamic selector handling is outside this rule.
- Permission checks and signature access remain current messages; `try_send` keeps
  its later epoch fence, byte copies, input protection and current target send.
  The dynamically named Word-helper guard and real Block construction are unchanged.

The proofs do not verify the Rust/C translation, an arbitrary owner declaration,
or all possible Id replacements. Runtime checks cover canonical role/context
admission, replaced guards and interning, permissions changed without an epoch,
collecting/mutating signatures, target replacements and incoming ABI carriers.
The observable contract includes replacement callbacks; passive census events
are diagnostic costs, not an assertion that the full public operation has a
minimal message trace. No root coalescing, Block/Frame omission or arithmetic
specialization follows from these guard proofs.

## Authored first-match search

`CollectionSearch.lean` proves that replacing the full fold with early return
preserves the result, predicate state effects and ordered predicate calls under
canonical conditional/nil contracts and a fixed element sequence. The abstract
predicate may update state used by its later invocations; non-nil Error results
remain ordinary values. A nil match continues. The theorem deliberately does
not equate internal traversal sends, allocation observations or arbitrary
replaced accessors: this is an authored library algorithm change, not a compiler
rewrite of a user program.

`CollectionSearch.tla` checks a bounded search with parked predicates, GC,
replacement, cancellation and resumption. The entered predicate retains its
version and element; the next read uses current contents. Negative cases require
counterexamples for replay, stale reads, missing roots and premature nil return.
Executable collection-search tests connect these abstract contracts to installed
Birnam, including one-step Machine budgets and mapped/rebuilt saved images.
