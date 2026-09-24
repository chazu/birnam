# Design 90 — Self optimization review and allocation-policy correction

Decision: 2026-09-24. The governing Birnam requirements are now in
[Design 90's optimization contract](90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history).
The objective is faster execution with late-bound language behavior. Reproducing
an unoptimized allocation history is not part of that objective.

## Sources and findings

Reviewed Hölzle, Chambers and Ungar's 1992 paper,
[Debugging Optimized Code with Dynamic Deoptimization](https://bibliography.selflanguage.org/_static/dynamic-deoptimization.pdf),
and the VM repository linked from the [Self project site](https://selflanguage.org/).
Implementation references below are pinned to commit
`ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e`. This was a source review of the relevant
compiler, block-allocation and conversion paths, not an execution benchmark or an
audit of every Self optimization.

The paper's sections 3–4 describe scope descriptors for reconstructing source
activations, arguments, locals and expression state at interrupt points. Method
changes invalidate dependent code; deoptimization makes subsequent calls observe
the new definition while preserving already active method semantics. Section 2.1
also preserves inspectable variables even after their last computational use.
Section 3.4 coordinates GC with interrupt points. These are obligations about
recoverable state and valid execution, not a requirement to match the allocation
schedule of unoptimized code.

The implementation makes the allocation distinction concrete:

| Reviewed path | What the code does | Relevance to Birnam |
| --- | --- | --- |
| [`PReg::canEliminateAndStillDebug`, `BlockPReg::canEliminateAndStillDebug`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/any/sic/preg.cpp#L420-L539) | Permits block elimination with reconstructible descriptors, subject to use/escape and nested-block constraints. | Retain enough information for required observations; a physical object is not always necessary. |
| [`BlockCloneNode::gen`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/any/sic/node.cpp#L2059-L2083) and [`BlockCreateNode::gen`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/i386/sic/node_i386.cpp#L281-L312) | A memoized block initially gets a sentinel. The creation path checks whether it already exists and clones only if needed. | Delaying allocation is real; it is not an allocation charged at the original source point. |
| [`really_clone_block`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/any/objects/blockOop.cpp#L40-L75) | Increments `NumberOfBlockClones` and allocates storage when a clone is actually made. | This path counts real cloning work. The sentinel path does not simulate that work. |
| [`Conversion::create_previously_optimized_blocks`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/any/runtime/conversion.cpp#L84-L107) | Walks the virtual frames being converted and asks them to create described blocks. | Deoptimization reconstructs the activation state being recovered. It does not replay an allocation log for completed computations. |
| [`compiled_vframe::createBlock`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/any/runtime/vframe.cpp#L257-L275), [`copyValueFrom`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/any/runtime/vframe.cpp#L407-L439), and [`valuesDo`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/any/runtime/vframe.cpp#L484-L525) | Materializes described blocks into a conversion table and reuses that mapping while copying values. Traverses frame names and expression state. | Recovery bookkeeping preserves required identities and frame contents; it is distinct from accounting for dead allocations. |
| [`nmethod::invalidate`](https://github.com/russellallen/self/blob/ac3faa60e82c41416e9d6ee7ec9f44cbd930fa0e/vm/src/any/zone/nmethod.cpp#L568-L584) | Marks compiled code invalid, requests process invalidation and unlinks it. | Optimizations depend on assumptions that can cease to hold. Birnam must enforce its own replacement behavior through its existing guards and recovery. |

I found no compelling reason in these paths to retain synthetic allocation charges,
dead-payload journals, baseline collection schedules or prepaid identities. That
conclusion is an inference from the reviewed mechanisms, not a claim that Self's
entire implementation has been proved free of unrelated allocation bookkeeping.
Self does reconstruct eliminated blocks when frames need them, including for
inspection. That is not evidence for recreating objects solely so a collector can
count and immediately discard them.

## Birnam decisions

| Keep | Drop |
| --- | --- |
| Current lookup and replacement behavior; selected-call ownership; effects and errors | Baseline allocation/count/byte parity and automatic GC/finalizer schedule parity |
| Observable identity, aliasing and shared Cells | Global prepaid identity issuance solely for allocation equivalence |
| Recovery of required values and logical Frames, including inspectable state | Journaling and replaying already-dead payloads and omitted index boxes |
| Roots, real resource cleanup and explicit collection | Fictitious allocation charges and uncharged realization of prepaid objects |
| Evidence that an optimization saves actual work at acceptable cost | Treating an old receipt or an existing helper as a requirement to preserve |

This does not make arbitrary constructors or finalizers pure. A replaced
constructor that increments a counter has a language effect, even if its result
is unused. An object with an actual resource/finalization duty cannot simply be
omitted. Such a candidate must preserve that effect or decline. Conversely,
reading heap statistics does not entitle the caller to a simulation of allocations
that optimization removed. Explicit collection operates on the actual heap;
automatic collection and finalization may occur at different times.

The current production prefix already has a suitable starting point:

- [Execution recovery graphs](../../rust/birnam-execution/src/recovery_graph.rs)
  distinguish existing inputs, distinct virtual nodes and repeated aliases.
  [Materialization](../../rust/birnam-execution/src/recovery.rs) constructs required
  values through owner messages with roots and validation. No global prepaid
  identity service is necessary for this representation.
- [GC accounting](../../rust/birnam-collect/src/accounting.rs) and its construction
  path account for actual allocations. Keep checked arithmetic and ordinary
  threshold/reset behavior; do not add charges for elided objects. The existing
  `canAllocate:` preview is neither a reservation nor an optimizer prerequisite.
- Runtime's [dead-allocation journal](../../rust/birnam-runtime/src/heap/dead_allocations.rs),
  counted-index/numeric ledgers and Storage's deferred-identity helper are retained
  legacy code outside the converted production path. Do not reconnect them.
  Their old integration tests/receipts are historical evidence, not migration gates.
- The generated-candidate test's replaced Integer factory has a visible call-count
  effect. Keep that dispatch/effect check; it does not demand equal physical heaps
  when the selected implementation actually permits elision.

No new runtime protocol is introduced by this decision. It removes obligations
from the remaining work; it does not claim allocation-elision is already ported.
Reuse useful analysis and recovery as concrete optimizations are migrated, removing
obsolete accounting plumbing from those consumers. Do not copy Self's VM-private
interfaces across Birnam's module boundaries or infer that every Self optimization
is valid for Birnam's object identity and control semantics.
