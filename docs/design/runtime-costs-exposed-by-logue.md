# Language-wide costs exposed by Logue — 2026-10-22

> **Historical scope:** This diagnosis records the earlier installed baseline. Its proposals were followed by retained optimizations and the later Oop-only migration. See the
> [complete work summary and current checkpoint](logue-runtime-work-summary.md).

The [first Id improvement](id-default-lookup-fact.md) reduced Logue CPU by 4–6%,
but addressed only a small repeated native dictionary read. The larger finding
is how much execution machinery a small Birnam operation invokes. Dispatch is
prominent in profiles because simple source constructs generate many messages.

This follow-up makes no further runtime or game changes. It measures the current
installed runtime, including that Id improvement, using generic language probes.
[Evidence, exact probe source and compressed dispatch traces](../measurements/logue-language-costs-20261022.json)
are retained for the next implementation.

## Measured work

Prepared blocks run through Id from a native measurement loop. They use normal
Codegen evaluation and installed stdlib/stackless-control owners. Compilation,
fixture preparation, answer checks and explicit between-sample GC are untimed.
Automatic GC remains enabled during three timed repetitions. Separate allocation
runs disable automatic GC for one invocation. Separate instrumented-Id runs
count messages; their execution times are not used.

These timings include entering and returning from the probe block. An empty
block costs about 8 microseconds and allocates 14 objects. Consequently the
allocation columns are complete probe costs, not solely the leaf operation.

| Prepared language probe | CPU per invocation | Objects allocated |
| --- | ---: | ---: |
| Public Integer addition | 62 µs | 24 |
| Multiply/add coordinate expression | 142 µs | 44 |
| Symbol `printString` | 51 µs | 29 |
| Compare two Symbol print strings | 102 µs | 48 |
| Map hit | 8 µs | 12 |
| Cell read | 15 µs | 14 |
| Range `size` | 209 µs | 66 |
| Range `at:` | 186 µs | 62 |
| 1,080 empty Range iterations | 543.5 ms | 152,431 |
| Empty nested 60×18 Range loops | 561.0 ms | 157,363 |
| 1,080 empty direct Integer iterations | 174.3 ms | 75,611 |

Repeated-process measurements agree: the original flat/nested empty Range loops
measured 531.1/558.4 ms. Their allocation counts are identical in both runs.
The flat loop charges 2.54 MB; nested loops charge 2.63 MB. These timed samples
start after collection, so this work does not require crossing the 16 MiB GC
budget. The half-second empty loop is execution cost, not an occasional GC pause.

A separate direct-owner probe measures the actual boxed ObjectModel slot read
at **0.374 µs** and write at **0.381 µs**, including current numeric conversion,
validation and Id dispatch. Native Integer unboxing measures about 0.07 µs.
These are different measurement boundaries from an authored Cell call; they
demonstrate that the underlying native read is much cheaper than the surrounding
language execution, not a promised accessor speedup ratio.

## 1. General Range traversal repeatedly runs expensive arithmetic policies

The selected [Range driver](../../modules/stackless-control.birnam) uses the same
indexed traversal shape as Vector: compare index with `size`, call `at:`, invoke
the callback, increment index, tail-send the next iteration.

But [Range's `size` and `at:`](../../modules/range.birnam) are authored policies.
Each `size` reads two slots, validates/compares Integer values, subtracts, checks
errors/overflow and adds one. Each indexed access validates the index, reads the
start, adds the index and checks the resulting value against the current end.
The loop repeats these policies even for an empty callback.

The direct Integer iteration probe performs the same 1,080 callback invocations
in about one third of the CPU time and half the allocations. This is concrete
evidence for a general traversal investigation. It is not a drop-in equivalence
proof: changing a Range driver must specify behavior under replaced `size`,
`at:`, range accessors and control methods, mutation during callbacks, errors,
collection and suspension. Keep the current path where required, or document an
intentional library implementation-contract change; do not silently bypass live
methods in a compiler optimization.

## 2. Call preparation is repeated at a very fine granularity

The separately instrumented message trace gives these complete probe counts:

| Probe | Id messages | Integer namespace reads | Call-preparation attempts |
| --- | ---: | ---: | ---: |
| Integer addition | 665 | 50 | 7 |
| Symbol print-string comparison | 1,519 | 128 | 19 |
| Range `size` | 2,579 | 230 | 31 |
| Range `at:` | 2,437 | 231 | 31 |
| Construct/traverse a one-element Range | 11,863 | 1,127 | 148 |
| Construct/traverse a two-element Range | 18,026 | 1,701 | 224 |

In the one-element case, 4,292 messages (36.2%) occur beneath
`tryPrepareCall:...`. For addition the corresponding fraction is 30.5%; for
Symbol string comparison it is 36.3%. Each message is counted once within that
category even if nested. **These are message shares, not CPU shares.**

The [generated prelude](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-native-backend/src/native_prelude.c)
and [Execution preparation](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-execution/src/machine/frame_call.rs)
explain the expansion. The prepared path resolves its optional protocol, checks
allocation/lookup permissions and namespace behavior, obtains the Integer
factory, checks index-box omission permission and machine controls, rechecks
namespace/factory/epoch state, then copies logical slots and publishes the call.
The fallback constructs Integer objects for frame-slot indexes and sends
`oopAt:`/`oopAt:put:`. Even generated native code performs this protocol work.

The next compiler/runtime investigation should target the amount of preparation
and frame-index machinery per source operation: reusable owner-certified facts,
prevalidated immutable descriptors, and bounded guarded call-chain inlining or
fusion. Namespace rebinding and replaceable permission/lookup callbacks require
more than a global method epoch. Preserve actual logical frames, selected-call
ownership and recovery at callbacks, suspension and debugging boundaries.

An attempted existing compiler-mask comparison failed during setup at
`micro_support/control.rs:115` on a missing binding. It supplied no performance
evidence. In particular, this investigation does **not** establish that simply
turning off call preparation would help; the fallback has its own boxing costs.

## 3. Boxing and adaptation amplify ordinary numeric and accessor work

The [Integer policy](../../modules/integer.birnam) validates operands, adapts
them to Word values, executes the checked operation and constructs the result.
Branch reuse and earlier input-root reductions are already present. The remaining
work still makes ordinary addition and a two-operation coordinate expression
expensive. Map hits and primitive reads are much smaller than these composed
paths; another HAMT or byte-copy micro-optimization is not the leading target.

The compiler opportunity is general numeric regions and cheap small method calls:
keep internal numeric values unboxed where proven, avoid materializing frame
index/control temporaries, and materialize required values at observation points.
Retain signed/overflow/Float/error semantics in their owners and honor changed
constructors/getters. The existing [numeric-predicate optimization](90-numeric-predicate.md)
only covers a narrow arithmetic-then-comparison body returning an existing
Boolean. It does not cover general arithmetic-returning chains or loops.

This is larger compiler/recovery work, with broader potential than the preceding
dictionary-read optimization. The current evidence does not promise a numerical
speedup for that unimplemented work.

## 4. Allocation churn turns full-heap collection into visible stalls

The [real-game diagnosis](logue-latency.md) reproduced 0.55–0.81 second
collection-associated pauses. Empty control flow alone can allocate more than
150,000 objects, so collection pressure is a language-wide concern.

The [collector](../../rust/birnam-collect/src/collection.rs) constructs a complete
allocation inventory, orders it, builds an address index and mark storage, then
traverses roots and owner-supplied references. Automatic collection is synchronous
at allocation admission. The preceding headless Logue run retained roughly
55,000 objects after GC, in addition to the transient objects created between
collections; those counts include the runtime and harness.

First reduce the avoidable temporary allocation above. Then profile inventory,
marking/owner dispatch and retirement separately to choose collector changes.
Incremental/generational collection would require explicit owner/write-barrier
contracts; it is a design candidate, not an already justified patch. Raising a
threshold or deferring collection does not eliminate its pause cost.

## Recommended order

1. Establish a cheaper general Range/control traversal contract and implement a
   bounded slice. Gate it on empty traversal, callback/mutation/resumption tests,
   allocations, and unchanged Logue rendering.
2. Reduce repeated call preparation and tiny-method/frame overhead using the
   current namespace and replacement contracts. Measure common source operations
   as well as the native leaves.
3. Extend guarded numeric lowering beyond the existing narrow predicate case.
4. Profile and improve GC pause phases, using the reduced allocation workload.

These are shared runtime/compiler/library opportunities. Logue remains the
acceptance workload. Independent probe times are not additive percentages of a
Logue frame, and the 3.1× empty-loop comparison is not a measured game speedup.
