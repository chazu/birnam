# Closure machinery: purposes, necessary behavior, and excess guarantees

Implementation follow-up: [compact queries and direct capture construction](closure-construction-simplification.md)
are now implemented and measured. The audit below describes the pre-change machinery.

Investigation date: 2026-10-07. This is a source/trace audit, not an implemented
optimization. It uses the warmed sixteen-addition trace described in
[the whole-send analysis](integer-send-totality.md), including that dirty worktree's
unfinished input-root changes. No long benchmark or production-path change was
needed for this investigation.

**Conclusion:** the logical job of a closure is small: remember its executable
meaning and lexical values, support ordinary invocation/observation, and keep those
values and code alive. The current implementation instead makes a disposable
ObjectModel container with a private behavior, fills and validates it, copies it
into a Block with another private behavior, and retrieves captures individually
on every invocation. Most of this protocol is replaceable. Exactly how few sends
are sufficient depends on the boundary and observation contracts we choose;
this audit does not establish an optimal numerical lower bound.

## 1. An exact accounting of the 2,791 closure-related sends

All 128 constructed Blocks in the trace obey:

```
construction sends = 168 + 23 × capture count
```

Eight constructions with 39 total captures therefore cost `8×168 + 39×23 = 2,241`
sends per addition. Another 550 sends retrieve captures. The following partition
is disjoint and covers all 2,791 sends:

| Current task | Sends/addition | Why it exists |
| --- | ---: | --- |
| Allocate temporary reference containers | 368 | Carry captures between generated code and the Block owner |
| Populate those containers | 390 | Store 39 lexical values, with checks/admission/roots for each write |
| Reread their contents in the Block constructor | 273 | Extract 39 values through the storage owner, then copy to Block payload |
| Export/obtain portable bytes for program shape | 672 | Look up CodeObject, export Bytes, obtain their byte span |
| Validate container size and all-reference layout | 96 | Establish capture count and reject an unsuitable storage representation |
| Create Block behavior, install its method, allocate Block | 96 | Provide the closure's callable identity and owned payload |
| Constructor entry and allocation admission | 16 | Eight constructor sends and eight collector admission sends |
| Generated ObjectModel/Block factory lookup | 220 | Obtain the current owner before storage writes and construction |
| Generated roots for returned preparation objects | 110 | Keep owner/storage/Block values alive through later callbacks |
| Capture access during method/Block invocation | 550 | Project 69 values through the Block owner with lookup/admission/roots |
| **Total** | **2,791** | **72.6% of all addition sends** |

The program-export row includes the 568-send `canonicalBytesOf:` subtrees from the
previous report, plus 104 sends to find the owner and obtain the returned byte
span. Revalidation/parsing also consumes Rust CPU without additional Id sends.
These figures are costs of the current tasks, not additive promises of savings.

The 23 extra sends per capture are: ten for the temporary write, four for its
adjacent factory lookup, two generated returned-object roots, and seven for its
constructor reread (two name sends plus five storage-read/check sends).

The construction side allocates **five Id objects per Block** on this path:
reference-container behavior, reference container, exported Bytes wrapper, Block
behavior, Block. That is 40 objects/addition before arithmetic values and other
objects. Bytes also owns a Rust byte buffer. This is not the whole allocation census.

## 2. What is actually indispensable

These are necessary *properties*, not prescriptions for a particular send count:

1. **Correct executable meaning and environment.** Invocation must run the right
   body with the right captured values, binding order, aliases and shared Cells.
   Invalid externally supplied code/captures must not lead to unchecked memory access.
2. **Lifetime.** Needed captures, the Block, method/native entry context, entered
   code generation and borrowed buffers must survive their actual uses and callbacks.
   Garbage collection must know the closure's outgoing references.
3. **Current Id dispatch at retained boundaries.** Cross-owner operations remain
   messages. Replacing the current constructor, projection or conditional operation
   must affect future sends to that operation. An owner may read its own payload;
   another module may not bypass that owner.
4. **First-class behavior when observed.** Escaping Blocks can be passed, retained,
   invoked repeatedly and reflected upon. Meaningful identity, behavior changes and
   supported logical Frame/recovery observations must remain coherent.
5. **Real effects and failures.** Retained operations and explicit collection/resource
   actions have their actual contract. Simplification cannot replay completed effects
   or discard an actual resource cleanup duty.

Neither Id nor these properties requires a separate heap container to convey each
capture set, a unique method installation for each instance, a full portable-code
export per construction, or a separate retain token for every projection.

## 3. The reason for each check, and what it protects

### A. Revalidating immutable code: useful admission, excessive repetition

CodeObject owns a `ValidatedProgram`: complete scope/encoding/nested-program
validation occurs when the program enters the owner. Its immutable record retains
checked counts; `view()` reconstructs the checked view without validation.
Nevertheless, `new_block_input` asks the current CodeObject owner to export the
whole program into a new Bytes object, asks Bytes for its span, and invokes the
full portable-program inspector again. That inspector validates nested programs,
not merely the two counts the constructor uses.

**Actual need:** distinguish admitted code, obtain arity/capture count, reject a
mismatch before publishing an invalid closure. Resumable Methods also compare
these counts with their registered descriptor. For synchronous Methods this path
has no resumable descriptor to compare, so it derives counts from CodeObject.
It does not prove the supplied synchronous native body implements those bytes.

**Not inherently needed:** reserialize, allocate a Bytes wrapper/buffer, register
its finalizer, root all those intermediates and revalidate the complete admitted
program on every construction. Native code is already subject to its unsafe ABI
contract; this is not a sandbox against dishonest native modules.

**Recommended boundary:** query admitted shape from the current CodeObject owner
through a compact message, retaining local kind/count checks. Validation of newly
supplied raw bytes stays at admission. Replacing that new message remains effective;
we need not preserve interception of an exporter that this path deliberately stops
using. Calling replaceable convenience accessors and blindly trusting them would
be a different, weaker authority contract, and is not required by this proposal.

### B. Temporary capture storage: mostly transport machinery

`allocateReferenceSlots:` creates a delegated behavior, registers an all-Oop layout,
allocates an ordinary object, validates allocator size/behavior, registers the
instance extent, and ties the behavior to its owning instance. The compiler then
writes every capture. The Block constructor checks size/layout, reads every slot
and copies the values into Execution-owned Block storage. It does **not** retain
the temporary reference container as the Block's environment.

**Actual need:** convey ordered live Object references to the Block owner and copy
or adopt them coherently. A generic ObjectModel container needs its own allocation,
layout and reference-enumeration rules while it exists.

**Not inherently needed:** choosing that generic container as the transport, giving
it a fresh behavior/layout/ownership record, and doing a write-then-read round trip.
Those rules protect the chosen intermediate representation, not closure semantics.
The storage/write/read rows together cost **1,031 sends/addition**.

**Candidate:** a current Block-owner construction message receiving method, code
and capture Object operands directly through the existing Id operand-array/count
ABI. Execution would validate/copy the carriers into its own storage; no module
reads foreign payloads. This is a proposed native message shape, not an extra Id
floor API, executable service table or implemented Birnam variadic source feature.
Decide how ordinary source callers supply variable capture sets before deleting
all collection-based construction routes. Keep a route because a live caller needs
it, not for old-artifact compatibility.

### C. Per-slot identity/layout checks: genuine generic safety, repeated boundary work

Each write costs ten sends: the write itself, two retains/releases each,
`prepareAllocation`, three identity queries and a behavior query. The identities
verify the container's instance record, its behavior's layout record, and the
incoming edge. Reads cost five: read, instance identity, behavior, layout identity,
and `allocationAt:` to turn a stored word into a live allocation reference.

**Actual need:** avoid out-of-bounds or foreign-storage access, invalid reference
edges and stale weak records after allocation-address reuse. Identity checks are
not GC retains; they check allocation incarnation. Passing an arbitrary valid Oop
is not enough to authorize reading it as reference storage.

**Not inherently needed:** proving the same container/layout independently for
all 39 writes and again for all 39 reads after creating it for this purpose.
Removing the intermediate removes these boundary checks entirely. A bulk operation
could instead validate once inside a callback-free owner interval. A validation
cannot simply be cached across a send that may collect or mutate that state.

The current code is not uniformly defensive: ObjectModel validates an allocator's
returned size/behavior; Execution's Block allocation records and uses the requested
extent without that same validation. We must choose a coherent allocator contract,
rather than claiming every current query enforces a universal safety guarantee.
Recommendation: retain boundary validation where an owner is about to access
externally provided storage; trust an admitted owned representation until mutation,
retirement or exposure invalidates the relevant premise.

### D. Collector admission: phase protection versus arbitrary collection points

`prepareAllocation` both rejects forbidden collector phases and can trigger an
automatic conservative collection when the allocation threshold is exceeded.
The shared Execution operation entry invokes it before choosing the operation.
Consequently **69 capture reads/addition** run it despite doing no allocation or
payload mutation. Construction adds 55 more: 39 slot writes, eight constructors
and eight container-owner registrations.

**Actual need:** prevent prohibited allocation/mutation during collection and keep
collection/resource management correct. The allocating operations already go
through Collection's allocation wrappers and accounting.

**Not inherently needed:** making every pure read an allocation-admission point,
or triggering automatic collection at exactly those old reads. The accepted
[allocation-history contract](90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history)
does not freeze automatic thresholds or timing.

Recommendation: admit allocation/mutation at the operations that perform them.
A read-only capture projection does not need an allocation permit. Explicit
collection and actual cleanup duties remain. Whether reads should be permitted
while marking/finalizing is a separate reentry contract; inspect reference/root
iteration before broadening phase access. Removing this send is not permission to
remove mutation-phase checks elsewhere.

### E. New behavior/method for every Block: convenient dispatch representation

Each Block gets a delegated behavior and installs a `value` selector bound to its
Method; Id sends then select that Method naturally. This also changes the method
epoch for every construction. The behavior/method/allocation row is 96 sends;
it excludes reference-container behavior costs already charged above.

**Actual need:** ordinary Id dispatch chooses the correct code for a Block and
keeps its Method/code alive. A materialized first-class Block may need distinct
identity and independently alterable behavior.

**Not inherently needed:** install an identical Method again for every closure
instance from one literal. A shared per-body behavior or a shared owner-dispatched
entry could select code from Execution's own Block payload while still entering
through Id. Neither alternative may call cached foreign code behind Id.

**Open decision:** behavior identity and direct behavior-dictionary mutation are
observable. Sharing behaviors is not automatically equivalent. Investigate private
behavior materialization on exposure/mutation, or retain private behaviors if that
is simpler. This is a lower-priority representation decision than deleting the
capture round trip and full program export.

### F. Capture projection, name resolution and roots: necessary access, expensive packaging

The 69 projections are not 69 distinct lexical variables. The four new selected
Blocks read 28 captures; pre-existing method/helper Blocks read the other 41.
The latter includes sixteen captures for four Boolean conditionals, eight for two
`integerBitsOf:` calls, and captures for arithmetic/conversion/validation helpers.
Captures needed only by nested branches are propagated through their enclosing
Blocks so those children can be constructed. The compiler already selects referenced
enclosing names; there is no evidence here that all large capture sets are simply
unused-variable mistakes. Nested shadowing and path-specific liveness merit a
separate source audit before claiming minimal capture sets.

The 550 sends comprise 276 inside 69 Namespace factory lookups, 69 roots of the
returned factory, 69 projection sends, 69 admission sends, and 67 roots of returned
captures. The other two projected results match protected inputs and avoid a root.
The Namespace lookup itself decodes the Symbol name through its owner.

**Actual need:** owner-mediated access to the right values, current dispatch and
protection through the next callback. The Block already traces its captures; an
invocation roots its receiver. This often provides lifetime coverage for a projected
value, but any allowed capture mutation/replacement can invalidate the simple
reachability argument. A replaced projection can also return a fresh object.

**Not inherently needed:** repeat selector-name interning, Namespace decoding and
factory lookup for each slot, eagerly read every capture at entry regardless of
path, or retain every result independently if a proved covering edge/scope exists.
Caching selector *identity* is distinct from caching a selected executable; fresh
method dispatch can remain. Removing/replacing a projection API need not preserve
its obsolete per-slot interception schedule.

Recommendation: simplify capture access as a protocol, not by freezing the current
factory or blindly borrowing fresh projection results. A whole-environment access
or on-demand projection should be compared with the simpler owner-local execution
representation before adding guards. Cross-owner access must still use Id.

## 4. Which safety guarantees should we retain or reconsider?

| Guarantee | Recommendation | Reason |
| --- | --- | --- |
| No out-of-bounds/foreign payload access | Keep | Memory/ownership correctness, not optional overhead |
| Needed objects and entered code survive collection | Keep | Prevents use-after-free and corrupted execution |
| Malformed external bytes rejected at admission | Keep | Establishes validity before any executable representation is published |
| Admitted immutable code fully revalidated for every closure | Remove repetition | Owner already retains checked facts; query them via Id |
| Generic external storage checked before access | Keep boundary check | A valid Object carrier does not imply an authorized representation |
| Every slot independently repeats the same container/layout proof | Replace with one owned interval, or remove container | Guards representation transport rather than language behavior |
| Every pure read is a GC admission/safepoint | Reconsider/remove | Read does not allocate; automatic timing is not frozen |
| Every projected value gets its own root token | Keep liveness, reconsider tokens | Coverage may already exist; fresh/replaced results still require protection |
| Every Block instance receives a newly installed Method | Investigate | Dispatch is necessary; per-instance installation is a representation choice |
| Every helper interceptor observes exactly the old private algorithm | Reject as a redesign constraint | Removed APIs need no compatibility imitation; retained sends stay current |
| Every Block argument is eagerly materialized even when unselected | Investigate separately | Current conditional can inspect/retain both; virtualizing requires exposure handling |

There is no honest necessary/superfluous count yet. **1,031 transport sends and
672 shape-acquisition sends** are the strongest concrete replacement targets:
1,703 sends/addition, 44.3% of all sends, before paying their simpler replacements.
Their removal budgets overlap neither each other nor the 550 capture-access bucket.
The 925 sends for constructing uninvoked Blocks overlap those targets and must not
be added as another independent saving.

## 5. Recommended decisions and smallest next slice

I recommend keeping the safety properties above, while explicitly rejecting
revalidation of already admitted immutable code and per-intermediate transport
proofs as perpetual guarantees. Id-mediated malleability remains required; the old
helper inventory and automatic collection schedule do not.

Start with a **simpler closure-construction protocol**, implemented incrementally:

1. Replace full canonical export/reparse with a compact current-owner shape query.
   Keep wrong-kind/count/mismatch rejection. This is the smaller first experiment
   within the larger closure target, and does not require changing branch semantics.
2. Remove the capture-container write/read round trip using direct Id Object
   operands or a comparably small owned transport. Retain ordered aliases and one
   appropriate boundary validation; account for all replacement roots/admission.
3. Examine capture access after those changes. Leave shared behaviors and virtual
   unselected Blocks for later so they cannot postpone the larger simple deletions.

For step 1, Lean can model one immutable admitted program with stored shape and
prove queried counts equal counts obtained by canonical export/inspection; it must
also model needed code/capture reachability. The mutable method dictionary is
outside immutable program state: calling a replaced owner operation is still a
real send with its result/failure contract. The proof does not certify arbitrary
replacement methods or native code.

For step 2, prove construction preserves capture order, aliases and ownership,
without the temporary container. TLA+ should explore callbacks/GC during owner
queries and allocation, mutation/replacement, failure before publication and
retirement/address reuse. Include broken models missing an input root, accepting
wrong counts, publishing before initialization, and using stale weak metadata.
Runtime probes should cover zero/one/many/shared captures, wrong arguments,
replaced new operations, precise collection in callbacks and first-class invocation.
An argument deliberately collected in a callback must be rooted by the actual
caller/invocation contract; native Bytes are not an implicitly traced Oop array.

These are investigations/proposals, not new invariants or implementation claims.
No whole-runtime proof or optimal universal send-count theorem is required.

## Evidence and reproduction

[Closure receipt](../measurements/closure-machinery-20261007.json) checks the formula
and constructor subtrees for all 128 Blocks and records capture-entry ancestry.
It derives from the exact hashed trace in the prior
[send receipt](../measurements/integer-send-totality-20261007.json).

```sh
python3 scripts/analyze-closure-machinery.py \
  /tmp/birnam-send-totality/trace.json \
  /tmp/birnam-send-totality/analysis-final.json \
  --output /tmp/birnam-send-totality/closure-analysis.json
```

Source evidence:
[emitter](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-native-backend/src/native_sync.rs),
[generated roots/lookups](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-native-backend/src/native_prelude.c),
[Block construction/projection](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-execution/src/machine.rs),
[Execution scopes/reference tracing](../../rust/birnam-execution/src/lib.rs),
[ObjectModel](../../rust/birnam-object-model/src/native.rs),
[collector admission](../../rust/birnam-collect/src/construction.rs),
[CodeObject owner](../../rust/birnam-portable-code/src/lib.rs),
[admitted program](../../rust/birnam-artifact/src/portable_program.rs),
[capture selection](../../rust/birnam-codegen/src/resumable.rs), and
[immutable-code protocol](60m-portable-code-object-protocol.md).

Existing materialization and Word-helper tests exercise replacement and collecting
providers on old operations. They are evidence of current dependency behavior,
not instructions to retain obsolete helper APIs after explicit redesign. This
turn validates trace accounting and source correspondence; it does not rerun or
claim new runtime safety evidence for an unimplemented replacement.
