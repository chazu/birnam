# 60KA — Adversarial Review: Reified Portable CodeObjects

> **Status:** Review complete 2026-08-24; Design 60K accepted 2026-08-25 after
> the decision-by-decision discussion. All blockers found in the first pass
> were incorporated into
> [60K — Reified Portable CodeObjects](60k-portable-codeobjects.md). The result
> is the accepted semantic contract; implementation was gated on Design 60L's
> exact physical formats and K1-K6 completed 2026-08-27.
> The review's recommended portable/physical authority split was accepted
> 2026-08-25. Immutable CodeObjects and CodeNodes, with successor construction
> and binding replacement as the edit boundary, were accepted the same day;
> ordinary Oop identity with digest-only integrity/cache use was accepted as
> well. The `[CodeObject, captures...]` portable Block layout and explicit
> non-callable family-prototype exception were also accepted. Reflect was
> confirmed to remain
> protocol-based: arbitrary callable implementation objects stay legal, with
> imageability enforced as a separate honest persistence contract. Authoring
> source was accepted as optional, nonsemantic provenance whose removal may
> reduce textual fidelity but cannot change execution or rebuildability. A
> clean image-v7 cut was then accepted; v5/v6 remain frozen legacy formats
> rather than accepting ambiguous CodeObject extensions. Reachable Blocks were
> accepted as materialization owners for the first implementation; optional
> safe-point re-homing is a later optimization, not a reclamation precondition.
> Current CodeBindings were accepted as direct image-v7 reconstruction
> authority, with source retained only as optional provenance and live vtable
> dictionaries remaining dispatch authority.
> A separate [Design 60L](60l-portable-program-and-image-v7-byte-grammar.md) was
> accepted as the sole physical-format authority for PortableProgram v1 and
> image-v7 before implementation.
> PortableProgram was confirmed not to add a fallback interpreter: execution
> requires an admitted compiler/materializer or a compatible native unit, and a
> stripped deployment with neither rejects honestly.
> Public CodeNode inspection, transformation, and construction were accepted
> for v1, making the compiler a producer rather than a privileged code owner.

## 1. Scope and method

The review challenged 60K against:

- Birnam's five invariant goals in
  [The Destination](30-the-destination.md);
- object-canonical dispatch and Store-arbitrated mutation in
  [42 — Object Canonical](42-object-canonical-relational-derived.md);
- compiled-unit and candidate lifecycle authority in Designs
  [45](45-compiled-units.md) and
  [47](47-compiled-unit-lifecycle.md);
- stackless entry, lowering, image, and physical-format contracts in Designs
  [60E](60e-resumable-registration-and-entry.md),
  [60F](60f-resumable-compiler-lowering.md), and
  [60I](60i-stackless-artifact-byte-grammar.md); and
- the indefinite-process space bound in
  [60J](60j-runtime-identity-and-history-compaction.md).

The implementation seam was checked as well: generated Blocks currently store
only captures, registered BlockBody entry copies payload words directly as
captures, image-v5/v6 identifies private block vtables by manifest index,
ordinary Reflect creates unique `__rust_mblk_N` Globals facts and TCC units, and
`LIVE_UNITS` roots those units for process life.

## 2. Initial verdict

**Reject the naive proposal.** “Put portable AST bytes on every Block” was
directionally correct but insufficient. It left authority, closure identity,
malleability, image ordering, native retirement, arbitrary callable objects,
and compatibility ambiguous. Eleven findings had to be resolved before the
proposal could be discussed as one coherent design.

## 3. Findings and incorporated resolutions

### 1. Canonicalizing stackless IR would contradict 60F

**Finding:** The existing `LogicalProgram` looks close to executable portable
code, but 60F explicitly defines it as compiler-owned. It contains lowering and
nested-AST relationships that another valid compiler need not reproduce.
Freezing it would turn one compiler implementation into a language ABI and make
future lowering changes image migrations.

**Resolution:** 60K places PortableProgram at a normalized language-semantic
layer above synchronous/resumable lowering. PCs, liveness, slot coloring,
FrameDescriptor bytes, generated C, and native disposition remain derived.

### 2. An opaque byte String would preserve portability but weaken malleability

**Finding:** A CodeObject containing only an opaque byte String would be
imageable, but ordinary code could not inspect or transform its structure
without a privileged compiler decoder. That would reify a handle, not code as
an ordinary object.

**Resolution:** `program` answers an immutable CodeNode graph through ordinary
sends, and ordinary constructors validate and create new CodeObjects. Canonical
bytes remain the independent interchange representation; decoded views may be
weak caches.

### 3. Mutable CodeObjects would invalidate active execution

**Finding:** If a CodeObject's body changes in place while a Frame retains an
old StepMethod and descriptor, one object would claim two meanings. Updating
active Frames would violate 60E/60F; leaving them unchanged would make
reflection lie.

**Resolution:** CodeObjects and CodeNodes are immutable values. Malleability is
expressed by constructing a successor and replacing a Block or CodeBinding.
Active Frames retain their captured materialization and meaning.

### 4. Code and closure state were conflated

**Finding:** Captures are not part of reusable code. Putting capture values in
the CodeObject would prevent Blocks from sharing code, confuse content digest
with live state, and duplicate capture graphs.

**Resolution:** CodeObject holds only capture schema. Block payload is exactly
`[CodeObject, captures...]`; nested literals carry capture operands and create a
new closure at evaluation time.

### 5. Content digest risked becoming a new intensional floor

**Finding:** Mandatory interning by digest would silently make SHA-256 the Oop
identity rule, contradict the repository's separation between object identity,
extensional policy, and weak indexes. A digest collision would become semantic
aliasing.

**Resolution:** Digest is integrity/cache metadata. Oop identity remains
ordinary. Deduplication is optional, weak, and requires complete byte equality.

### 6. “Compiled units are primary” appeared to conflict with CodeObjects

**Finding:** Calling CodeObject the executable authority could be read as
demoting mapped compiled units to a cache or routing dispatch through semantic
objects/Store, contrary to Designs 42 and 45.

**Resolution:** 60K explicitly separates portable executable meaning from the
primary physical executable layer. Native dictionaries and compatible compiled
units remain the live dispatch authority. CodeObjects provide reconstruction
and reflection, never a per-send Store or decoder path.

### 7. Quiescent calls do not make a reachable Block dead

**Finding:** Retiring code when active calls reach zero would leave a reachable
Block with a vtable naming unmapped code. “No call is running” does not mean “no
future call is possible.”

**Resolution:** A live Block vtable is a materialization owner. Removal of one
method binding releases only that binding's ownership. Unload waits for Blocks,
bindings, readers, Frames, Continuations, and leases, or first re-homes live
Blocks to an admitted successor carrier.

### 8. Reflect accepts implementation objects, not only literal Blocks

**Finding:** Requiring every implementation to have a CodeObject would narrow
the ordinary `value:` protocol and cut callable proxy objects, foreign wrappers,
and other malleable implementation objects off from `defineMethod:on:as:`.

**Resolution:** The named adapter continues to late-bind the implementation
object's `value:` family send. Blocks receive the portable CodeObject contract;
other objects remain legal but must supply their own honest persistence contract
or make image save reject that current binding.

### 9. Image-v6 cannot express CodeObject-owned block vtables

**Finding:** Image-v6 intentionally preserves v5 Heap bytes. A Block header's
manifest-index vtable locator is resolved before its payload is patched and
assumes payload word count equals captures. Merely adding CodeObject at word
zero would violate frozen preflight and loader rules.

**Resolution:** 60K requires image-v7, a CodeObject-aware locator, complete
portable-code inventory, `capture_count + 1` validation, and staged allocation
that admits CodeObjects/materializations before Block shells while still
allowing forward references and cyclic captures.

### 10. Source-less recovery still needed durable bindings and targets

**Finding:** Persisting CodeObjects alone would recover closure values but not
which target/selector currently names one as a method. Replaying source would
reintroduce the requirement being removed. An unnamed/unimageable target could
also make a nominally portable method impossible to reconstruct.

**Resolution:** Image-v7 carries current CodeBindings naming target, selector,
and implementation object. Source is optional provenance. Every target and
implementation must independently be imageable; impossible cases reject rather
than fabricate stable names.

### 11. A monotonically growing materialization registry would merely move the leak

**Finding:** Replacing `LIVE_UNITS` with one never-reused registry slot per
definition would still grow with process age. Reusing an unqualified integer
slot would let a stale thunk invoke an unrelated successor.

**Resolution:** Materialization repositories use generation-qualified reusable
capabilities, weak caches, explicit semantic roots, and diagnostics. Capacity
may track peak simultaneous live/quiescing code, never historical operation
count. K6 includes the same fixed-live-set churn that exposed the original leak.

### 12. The exported Block prototype is not a closure

**Finding:** The current `Block` singleton is a zero-payload family prototype.
Saying every Block has word zero would make bootstrap itself malformed or tempt
runtime code to guess layouts from payload size.

**Resolution:** 60K distinguishes the non-callable family prototype, legacy
private vtable schemas, and new portable-closure vtable schema explicitly.
Closure inspection and invocation validate the schema; no path probes word zero
speculatively.

## 4. Second-pass attacks

After incorporating the blockers, the revised design was attacked with these
scenarios.

### Self-replacement

A method replaces or removes itself while its old Block is running. The revised
ordering prevents new dictionary acquisitions, but the current dispatch-read
extent and active Frame retain the old materialization. The old native unit can
retire only at a later safe point. **Pass.**

### Retained implementation after method removal

The same Block is stored in a Vector and installed as a method. Removing the
method drops one CodeBinding but the Vector still reaches the Block, its
CodeObject, captures, vtable, and carrier. The Block remains callable. **Pass.**

### Source stripped before incompatible boot

An image contains CodeObjects and no source, then boots on a target where its
native Code record is incompatible. With an admitted compiler it rebuilds from
PortableProgram. A compiler-stripped image rejects rather than pretending its
portable data is executable in that deployment. **Pass, with an honest
deployment limitation.**

### Captured cycle

A closure capture graph cycles through another object back to the closure.
V7 preflights references, admits the CodeObject/materialization, allocates Block
and other shells, then patches edges. No source execution is needed during
allocation. **Pass.**

### Digest collision

Two different canonical byte sequences are forced to share a test digest.
Cache admission compares complete bytes, so they remain different CodeObjects
and materializations. **Pass.**

### Dynamic callable implementation

A proxy object implements `value:value:` and is installed as a method. Runtime
invocation remains an ordinary send. Image save succeeds only if that proxy and
its type have a durable contract; otherwise the binding is reported as
unimageable. **Pass without narrowing the language.**

### Repeated definition churn

One target/selector is defined and removed thousands of times with a fixed live
set. Current CodeBinding ownership replaces rather than accumulates; Store
history compacts under 60J; materialization slots are generation-qualified and
reused; weak CodeObject/Symbol/vtable indexes do not root predecessors. Retained
space should plateau, and K6 makes that executable proof mandatory. **Pass;
the K6 implementation proof closed 2026-08-27.**

## 5. Residual risks, not blockers

1. **Physical grammar size — resolved 2026-08-25.** 60L now freezes the same
   offset/tag/bounds/fixture rigor as 60I; its accepted K0 evidence closes this
   prerequisite.
2. **Compiler bootstrap availability.** Source-free portability does not mean a
   stripped executable can rebuild code without any compiler. Deployment must
   carry either compatible mapped code or an admitted compiler.
3. **Re-homing live Blocks.** The first implementation may retain a generation
   while any Block using its private vtable remains live. Cross-generation
   re-homing is an optimization and requires its own safe-point proof.
4. **Debug fidelity.** Normalized code without provenance cannot reproduce
   comments, whitespace, or original source spans. It remains executable and
   structurally inspectable; tools must report when original provenance is
   absent.
5. **Public protocol spelling — resolved 2026-08-25.**
   [Design 60M](60m-portable-code-object-protocol.md) checks the vocabulary
   against the standard library, distinguishes a whole CodeProgram from its
   expression CodeNodes, and removes redundant constructor authority. Its
   [60MA review](60ma-portable-code-object-protocol-adversarial-review.md)
   stabilizes the exact accepted K1 surface.

## 6. Final verdict

**Accepted 2026-08-25.** The revised proposal preserves ordinary sends,
Store-arbitrated MOP mutation, compiler replaceability, source-independent
recovery, arbitrary callable implementation objects, and 60J's bounded-space
equation. No remaining issue requires changing its central shape.

Semantic acceptance originally authorized K0 design work rather than runtime
implementation. [60L](60l-portable-program-and-image-v7-byte-grammar.md) was
subsequently accepted and independently validated. K1's physical prerequisite
and exact public surface are therefore closed, so K1 implementation may begin.
CodeObject is the portable
executable meaning, while compiled units and vtable dictionaries remain the
primary physical executable and live-dispatch layer.
