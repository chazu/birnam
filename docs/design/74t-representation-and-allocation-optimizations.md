# 74T — Numeric, loop, aggregate and closure optimization

Status: implementation in progress. This document tracks the user's request to
implement all four remaining families in [74Q](74q-optimization-roadmap.md).
Recovery infrastructure alone does not complete a family. The outstanding 74S
application timing gate remains separate.

## Contract

Preserve ordinary Id meaning, mutable method lookup, object identity, shared
Cells, observable Frames, stackless control and active native-code ownership.
No selector or prototype name grants intrinsic authority. The public IdApi,
Method ABI, canonical descriptors and image formats remain unchanged.

Compiler eligibility and recovery descriptions must be producible using
ordinary Id objects. Runtime admission validates storage, roots, authority and
lifetime; it does not turn a producer's semantic assertion into trusted proof.
An optimizer must independently establish that the operation being replaced is
the operation for which it has a guarded summary.

The existing code establishes two constraints that govern implementation:

- `birnam-prelude::numeric_binary` uses signed 128-bit intermediates for integer
  addition, subtraction and multiplication, then checks the signed 64-bit range.
  Division also checks zero and range; mixed arithmetic converts to floating
  point. Error operands and operator redefinition remain ordinary behavior.
- The floor exposes live allocation counts and triggers automatic collection
  using allocated bytes. Non-escape alone cannot authorize allocation removal.
  Removing an allocation must preserve observations at subsequent boundaries,
  including GC timing and finalizers; suppressing collection is not a proof.

## Implementation sequence and completion evidence

| Work | Required production behavior | Evidence required before completion |
| --- | --- | --- |
| Shared recovery | Validated acyclic identity graphs, existing Oop inputs, exact numeric bits, one materialization per virtual identity; roots survive collecting materialization and publication stays atomic | Invalid and stale recipes; repeated aliases versus equal distinct objects; GC between construction steps; Id-authored recipes; capture/restore and native retirement |
| Unboxed arithmetic | Derive numeric chains from current admitted implementations; carry raw intermediate values with exact overflow/error rules and materialize at observation boundaries | Integer extrema, zero division, mixed numeric/error operands, changed operators and lookup, forced exits at every supported point; separate arithmetic and allocation accounting |
| Loop optimization | Recognize actual Id control behavior; remove invariant work within bounded execution intervals, preserving ordinary mutation and stackless transition boundaries | Representative loop, effects once and in order, invariant invalidation, suspension/capture responsiveness, guard overhead and profitable execution |
| Temporary-object elimination | Derive escape, identity and allocation-observation proofs for acyclic aggregates; scalar-replace private fields and materialize before exposure | Actual allocation savings, shared graph identity, reflective/native escape, collection during materialization, finalizer/collection threshold counterexamples |
| Closure/capture elimination | Derive a directly invoked private closure and substitute its environment into the admitted body; retain shared mutable Cells | Actual closure allocation savings, multiple uses and escaping fallbacks, shared Cell updates, method replacement, native observation and continuation capture |

These are implementation obligations, not a claim that every source program is
optimizable. Unsupported or unstable cases execute canonical code. Broader shapes
must use the same analysis and recovery contracts; passing a synthetic kernel
does not stand in for an installed compiler consumer.

## Recovery graph

The compiler representation separates input identities from newly constructed
identities. A value reference selects an existing input or an earlier graph node.
Every node denotes one fresh identity even when two nodes have equal numeric
values. Multiple references to one node must resolve to the same Oop. Nodes are
ordered before their users, so cycles and forward references are rejected before
Birnam allocation. Graphs explicitly describe every destination Frame slot.

The first implemented node shapes are exact signed integers, exact floating-point bits and
all-Oop aggregates with an existing vtable input. Existing objects, including
captured Cells, are retained as inputs. Raw storage, finalizers and trace hooks
require additional contracts and are not inferred from a producer's description.
Closure-specific construction must also retain its registered implementation and
capture layout; an arbitrary aggregate is not automatically a valid closure.

Structural validation is pure compiler code with fixed node, input, slot and
field limits. Runtime validation checks the actual heap and layout before
materialization. All inputs and constructed nodes stay precisely rooted until
the complete graph has been installed or rejected. Allocating construction and
callback-free Frame publication are separate phases. Recovery authority is
revalidated after any phase that can run foreign code.

`Machine recoverAt: pc withGraph: recipe` uses the same scoped, one-shot
registered-call authority as `recoverAt:with:`. Outside such a request it returns
nil. The recipe is `[inputs, nodes, slots]`; these are ordinary zero-based
`size`/`at:` objects, not a frozen binary encoding or new IdApi entry. Nodes are
`[0, integer]`, `[1, float]`, or `[2, vtableInputIndex, fields]`. Nonnegative value
references select inputs. Negative references select node `-reference - 1`.
For example, two `-1` references share node zero; two separate integer nodes
remain distinct even when their values are equal.

The decoder snapshots and roots every returned carrier through ordinary sends,
then validates the whole graph before construction. Limits are 128 inputs,
64 nodes, 128 destination slots and 512 aggregate fields in total. Runtime
checks require actual heap inputs and declared Oop fields with no custom trace
hook. Even an empty aggregate cannot impersonate a raw numeric carrier. Layout
and scoped authority are rechecked after collection before each construction;
publication uses the existing acquired-call recovery check and pin transfer.

This operation prepares new identities explicitly requested by a producer. It
does not authorize eliding any source allocation, replacing an escaped identity,
or using an arbitrary aggregate in place of a registered closure.

## Verification and measurements

For each family retain a canonical source fixture, pass-off and pass-on modes,
forced recovery points and ineligible/mutation controls. Run debug and release
differential checks over values, identities, errors, ordered effects and resource
lifetime. Complete the source-build, installed-prefix, image, cold archive and
relocation gates before calling the production change verified.

Record hits, misses, recovery counts, allocations, retained memory, code/metadata
size and preparation cost, plus paired cold/warm/steady timings. Host qualification
uses the existing thresholds. A noisy host can delay qualified timing but does
not justify dropping implementation or correctness obligations.

## Numeric implementation preparation

Prelude now supplies an [optional native declaration](../../abi/native-numeric-semantics-v1.md)
for its exact add, subtract, multiply, divide and remainder implementations.
The declaration is an unsafe native-module contract, comparable to native Method
and trace-hook contracts. Arbitrary Id annotations cannot attest opaque native
code. The floor queries the Method's actual resident image, checks that the
declaration is in that same image, and retains a loader reference for its lifetime.
There is no IdApi field or Method calling-convention change, and modules without
declarations continue normally.

The declaration also checks the initialized floor's Integer identity and the
current API functions used for type checks, payload reads, ErrorVt imports and
boxing. Each use revalidates this context. An instrumented/copied API table can
withdraw optimization; restoring its functions can restore eligibility. This
prevents silently bypassing a native boxing or import wrapper.

The first production consumer specializes a successful boxed numeric call inside
the existing stackless synchronous boundary. It retains every ordinary lookup,
Frame transition and result allocation. Both actual operands must have canonical
Integer storage. The ErrorVt symbol must already exist so its first interning is
not omitted. Current ErrorVt rebinding preserves the original argument identity;
overflow, zero division and other operands invoke the original Method. Each
method becomes eligible for declaration discovery after 64 calls, with at most
256 cached method records. A selected summary retains its image through GC even
if its record is evicted.

`numeric_chain::Plan` separately compiles bounded raw integer dataflow: up to
64 inputs and 32 operations. The C kernel retains all intermediate raw results
and returns the first failing operation without allocating or invoking Id.
Checked range predicates avoid C signed-overflow and division undefined behavior,
including MIN remainder -1. Differential tests compile 96 real TinyCC kernels
and compare 49,152 executions with the Prelude's i128 arithmetic rule, checking
partial results and surrounding canaries.

These raw kernels are not yet connected to production multi-call execution.
The boxed-call consumer does not establish completion of unboxed arithmetic.

### Next: connect a whole numeric chain

Derive straight-line call chains from canonical lowering, preserving aliases and
physical input locations. Resolve each admitted numeric implementation by its
actual current target, then instantiate and compile the raw plan only for hot
code. Record every original PC, index-box allocation, numeric-result allocation
and observation boundary. Literal and alias identities remain explicit.

The initial production chain should retain existing Frames and result boxes.
Within a proven bounded interval without observation or collection, compute the
raw chain and reuse its intermediate raw results while the ordinary driver
publishes the original boxed results and transitions in order. This avoids making
Frame reconstruction or allocation elimination prerequisites for arithmetic
specialization. Admission must cover all Machine methods used in that interval,
lookup/implementation dependencies and the complete allocation budget, including
PC tokens and publication operands. No callback or collection may invalidate a
precomputed value without discarding the remaining raw results and executing the
canonical operations. All errors and unsupported paths retain baseline behavior.

Prove forced withdrawal at each operation, exact allocation/identity/effect parity,
current target replacement and continuation behavior before performance
acceptance. Do not promote a compiler-only kernel as this production consumer.

## Progress

- Implementation contracts checked against current lowering, numeric primitives,
  allocation accounting and production recovery.
- Implemented pure graph validation, exact numeric/aggregate construction with
  scoped roots, and the ordinary Id graph-recovery protocol. Tests exercise
  shared references, distinct equal integers, NaN payload and signed-zero bits,
  malformed references, storage/layout rejection and actual finalizer mutation.
- A compiled Id producer supplies all three node categories through ordinary
  sends. Real registered recovery tests cover one-shot admission, collecting
  recipe reads, Frame aliases, precise GC, capture/restore and code-retirement
  pins. The prior dense recovery protocol has regression coverage.
- `python3 scripts/recovery-graphs.py --verify` records debug/release test binary
  hashes, source hashes, unchanged compatibility authorities and the full
  production gate in `docs/measurements/74t-recovery-graphs.json`.
  The initial run passed: nine new focused tests plus the prior dense-recovery
  test in each profile, and 479 workspace tests with 15 diagnostic tests ignored.
  Installed-prefix, image, cold-archive and relocation checks also passed.
- Native implementation declarations, guarded boxed numeric calls and raw integer
  kernel generation are implemented. Their tests include current target changes,
  observing lookup, actual collection at result allocation, operand mutation
  during that collection, native image pinning and API-wrapper rejection.
  `python3 scripts/recovery-graphs.py --numeric --verify` records this checkpoint
  separately in `docs/measurements/74t-numeric-preparation.json`.
  Debug/release checks and full production verification passed: 487 workspace
  tests, 15 ignored diagnostics, installed Prelude arithmetic and API-context
  probes, image/lifetime, cold archives and relocation. No timings are claimed.
- No remaining optimization family is complete yet. Next: connect the raw chain
  kernels to production execution as above, then implement loop analysis and the
  two allocation-elimination consumers.
