# Design 90 — First discarded-clone consumer

Implemented 2026-09-24, following the
[optimizer reassessment](../archive/design/90-optimizer-reassessment.md) and the
[Self-derived contract](90-self-optimization-review.md). This is one narrow
allocation-saving producer and consumer, not completion of batch 2.

## Exact case

The source callee is equivalent to:

```birnam
[:p :answer | [p clone]; answer]
```

It receives a zero-slot ordinary object and an existing answer. At an eligible
unbounded, non-tail, unprotected call, it returns that exact answer without
constructing the discarded clone or the callee's temporary Integer index boxes.
An alias of the actual clone implementation works too. A replacement clone or
allocator runs normally through baseline fallback.

The compiler proof admits exactly two PCs, no captures, no lexical bindings or
other operations, a zero-argument send to an entry argument, and a return of an
entry argument. Named adapters are excluded. Nonempty source payloads, used or
escaping clones and all other shapes remain ordinary code. The source and answer
may alias: the returned value is always the original argument.

A discarded result is still inspectable in an ordinary Frame slot. Consequently,
this implementation does not erase that slot or put nil there. It reuses the
existing inline-call acquisition and return commit, without exposing a callee
Frame on the successful path. Bounded execution declines, preserving its actual
steps, paused Frames and visible clone. Failed admission executes the baseline
once. There is no later clone to reconstruct and no new deoptimization engine.

## Owner messages

All queries are current Id messages. Each owner recognizes its own implementation
and storage; no owner exports a table or examines another module's payload.

| Receiver and message | Arguments | Meaning |
| --- | --- | --- |
| Method `withDiscardedClone:answer:selector:return:in:` | Word source argument, Word answer argument, Symbol selector, Word return slot, Namespace Object | Capture the baseline with this producer-proven inline plan. Like native `withInline:`, the producer owns the body proof. |
| ObjectModel `canDiscardClone:of:` | selected NativeEntry, source Object | Word 1 only for its actual clone function/context, zero-slot ordinary storage and current allocation path; otherwise 0. |
| ObjectModel `canOmitAllocationFor:` | behavior Object | Word 1 only with its current ordinary allocation wrapper and GC admission; otherwise 0. |
| Integer `canOmitIndexing` | none | Word 1 only with its ordinary `fromWord:`/`wordOf:` implementations and default allocation path; otherwise 0. |
| GC `canOmitAllocationFor:` | behavior Object | Word 1 only while idle with its actual behavior allocator and `prepareAllocation`, default lookup and Id allocation facts; otherwise 0. |
| Id `usesDefaultAllocation` | none | Word 1 only when Id's storage/geometry/identity/behavior queries, bind/epoch/lookup machinery and `usesDefaultLookup:` still have their default implementations; otherwise 0. Id reads only its own dictionaries. |

Execution checks the actual selected clone entry, current generated Namespace
Integer binding and existing inline control operations. Inputs remain rooted
across callbacks. Epoch checks reject mutations during admission; Execution
rechecks controls after the owner queries. The answer already has an argument
root, so no extra retain message intervenes between the final verdict and commit.
The plan traces its original Method, selector and Namespace, while existing
acquisition and TCC Unit ownership cover execution and fallback.

These queries are immediate semantic facts, not reservations or promises for
future calls. Their native authors own their contracts, like other owner methods.
No allocation budget, synthetic charge, journal, deferred identity, dead payload
snapshot or baseline collection schedule is reproduced. Ordinary real-allocation
accounting remains intact. Internal bookkeeping call counts are not a new
language equivalence requirement.

## Evidence and local cost

The [production source test](../../rust/birnam-siphon/tests/compiler/discarded_clone.rs)
uses actual Bootstrap owners and TCC code. It checks:

- Physical Id inventories contain no new source-family clone after an admitted
  call; ordinary refusal and bounded calls create one. Results retain exact
  existing-input identity.
- Replaced lookup, clone, Id allocation, ObjectModel allocation, GC preparation
  and Integer construction/conversion execute with their real effects.
- A guard that collects preserves the inputs. A guard that replaces clone after
  answering positively causes fallback and executes the new clone exactly once.
- An implementation alias is admitted, and source/answer aliasing preserves
  identity. Nonempty payloads, a lexical binding, an escaping clone and a changed
  Namespace Integer binding use baseline allocation.
- One-step bounded execution exposes the actual discarded clone through FrameView;
  collection while paused keeps it alive.
- Final release and collection return actual TCC Unit and relocated-byte counters
  to zero. No optimizer hit counter stands in for allocation or lifetime evidence.

For cost comparison, the same source is compiled once with metadata and once
with the attachment method replaced by an identity method, producing a genuinely
unannotated baseline. Both use the same compiled caller and inputs. Six pairs
alternate order, with collection before each 30-invocation group, outside the
measurement. Compilation and binding changes are outside the timer. The automatic
GC budget is disabled for these short measured groups.

On arm64 macOS, the local unoptimized Rust/debug-owner run recorded:

| 180 invocations | Physical allocations | Elapsed |
| --- | ---: | ---: |
| Unannotated baseline | 6,660 (37/invocation) | 333.6 ms |
| Discarded-clone consumer | 5,220 (29/invocation) | 261.1 ms |

This removes eight allocations per invocation and was about 22% faster in that
local comparison. The correctness test asserts the allocation reduction, never a
wall-clock threshold. This is evidence to enable this exact case, not release
qualification, a broad benchmark or a promise for larger source programs.

Reproduce the production build, 13 integration binaries and native boundary audit:

```sh
mise run native-messages
```

The 123 focused Codegen, Execution and Id unit tests also pass:

```sh
mise exec -- cargo test --offline -p birnam-codegen -p birnam-execution -p birnam-id --lib
```

Print the local cost comparison after that build:

```sh
BIRNAM_TEST_MODULE_DIRECTORY="$PWD/.cargo-target/native-message-tests/debug" \
CARGO_TARGET_DIR="$PWD/.cargo-target/native-message-tests" \
CARGO_PROFILE_DEV_PANIC=unwind \
mise exec -- cargo test --offline -p birnam-siphon --test compiler -- \
  --include-ignored --nocapture
```

The full workspace/CLI, wider elision families, live aggregate recovery and other
batch 2 integration remain outside this result.
