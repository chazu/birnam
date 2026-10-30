# Design 90 — Arithmetic predicate with an unboxed intermediate

Implemented after the [numeric-chain reassessment](../archive/2026-10-runtime-refactors/90-numeric-chain-review.md).
This combines a short numeric chain and removal of its intermediate Integer.
It does not restore the old fused driver, loop cache or deferred-identity system.

## Supported source and behavior

The first producer recognizes an uncaptured body of this form:

```birnam
[:a :b :c | let answer := [[a + b] < c]; answer]
```

The first operation can be an owned Integer addition, subtraction, multiplication,
division or remainder; the second an owned equality or ordered comparison. Actual
selected native entries and contexts establish meaning. Aliases work, while a
replacement operation takes the baseline. The ordinary Integer and installed
mixed-number implementations are recognized for exact Integer inputs only.

Codegen admits exactly three PCs: entry arguments and the first call; a second
call using that result and another entry argument; then return of the comparison
result, optionally through its final local binding. Captures, additional operations,
bindings of the intermediate, general loops, named adapters and bare tail-call
forms remain outside this producer. The runtime also declines bounded, tail and
protected invocation boundaries. Mixed inputs, arithmetic failure, changed owners
and unsupported second operations execute the ordinary body.

A successful call computes one checked raw integer, compares it, and returns the
Integer owner's existing Boolean. It allocates neither the intermediate Integer
nor the callee's generated PC/index boxes. Ordinary and optimized Integer paths
share the owner's arithmetic and comparison helpers, including existing f64
rounding in ordered Integer comparisons. Equality remains exact. Overflow and
zero division decline before consuming the body; baseline error behavior runs.

## The observation boundary

The earlier proposed `(a + b) * c` example needs a new final Integer. Its allocation
can expose an active optimized callee through callbacks or collection. This first
slice instead returns an existing Boolean. Chains returning newly allocated
numeric results still require a concrete observation/recovery design.

Execution checks its current Machine controls and compilation Namespace Integer
binding. Integer roots the inputs, validates the actual operations, current index
and allocation semantics, live ErrorVt, default Id facts and the same behavior
epoch. It reads the final inputs only after callback-capable checks. The remaining
Id fact reads and result receiver lookup have been established as default.
Arithmetic/comparison allocate nothing; delivery uses a fresh receiver send.
Custom result-receiver lookup selects baseline execution.

Integer sends `commitNumericPredicate:` to the supplied Object with its existing
Boolean. Execution's method validates pending-call authority, performs the existing atomic
inline-return commit and releases the acquired entry. It marks that request
consumed. Only then does Integer release temporary roots. Any cleanup callback
therefore observes the committed caller, not a missing intermediate Frame or an
unpublished answer. A declined owner sends no result and the ordinary driver
continues. Integer roots the receiver through the synchronous send and cleanup;
it knows no Execution layout. The existing pending-call record holds the commit
target; inactive/repeated commits fail.

Bounded execution always creates the baseline Frame and its actual intermediate
Integer. There is no virtual value to reconstruct in the successful unbounded
case. No allocation budget is reserved, no dead object is recreated, and no
synthetic charge or prepaid identity is recorded.

## Concrete protocols and ownership

- A resumable Method accepts
  `withNumericPredicate:then:left:right:limit:return:in:`: two Symbol selectors,
  three native Word entry-argument indexes, Word return-slot index and Namespace
  Object. It returns a captured Method with private inline metadata. As with
  `withInline:`, the compiler owns the body proof; Execution validates invocation
  shape and runtime authority. The original Method, selectors and Namespace are
  traced with the captured Method and existing TCC Unit retention.
- Integer accepts `evaluatePredicate:then:left:right:limit:in:epoch:with:`:
  two Symbol selectors, three operand Objects, compilation Namespace Object,
  expected native Word behavior epoch and result receiver Object. It returns
  Unit on refusal. On success it sends `commitNumericPredicate:` once with one
  Boolean Object and returns the send's outcome. Execution answers Unit after
  publication. Both numeric protocols now use message receivers; NativeEntry
  visitor operands are rejected.

Existing `usesDefaultAllocation`, `usesDefaultLookup:`, `canOmitIndexing`, allocator
owner queries and current `bind:to:` sends supply admission. Integer owns numeric
representations and recognizes its own functions. Execution owns the call commit.
Codegen emits metadata alongside the ordinary baseline, with no new C kernel,
hotness registry, pooled result buffers, per-PC candidate or cross-call cache.

## Validation and cost

The [production source test](../../rust/birnam-siphon/tests/compiler/numeric_predicate.rs)
compares the annotated body with the same source compiled while metadata
attachment is replaced by an identity method. This is an unannotated baseline,
not a baseline paying admission costs before refusal. Actual Id inventories count
allocations, including Integer-family objects.

Checks cover all five arithmetic operations, comparison semantics at signed and
rounding boundaries, overflow/zero-division fallback, current method/lookup/factory/
allocator effects, implementation aliases, live ErrorVt, mixed inputs, a bound
intermediate and numeric-result fallback. Paused Frame inspection sees the real
intermediate and retains it through collection. A collecting admission wrapper
keeps inputs alive; a replacement during admission executes the changed arithmetic
once. A replacement commit method collects and inspects the caller *before the Integer
query returns*, proving that its Boolean is already published. Repeated and
inactive commits fail, and customized result-receiver lookup selects baseline. Final release
reclaims every actual TCC Unit and relocated byte. Two compiler tests check the
producer's dataflow and rejection boundaries.

The local arm64 macOS debug comparison uses six alternating pairs of 30 runs,
with collection before each group and the automatic budget raised for measurement.
Compilation, binding changes and collection are outside the timed groups. For
180 invocations, the original native-visitor measurement was (historical timing;
not a performance claim for the receiver-message conversion):

| Execution | Total physical allocations | Integer allocations per invocation | Elapsed |
| --- | ---: | ---: | ---: |
| Unannotated baseline | 9,180 (51 per invocation) | 40 | 533.4 ms |
| Numeric predicate | 5,760 (32 per invocation) | 21 | 317.4 ms |

This removes 19 allocations per invocation, including one arithmetic intermediate
and 18 generated index/PC boxes. The local debug run was about 40% faster. Timing
is diagnostic; tests assert actual allocation savings and behavior, never an
elapsed-time threshold. It is not release qualification or evidence for all
numeric programs. Refused candidates incur admission work, and long chains and
newly allocated results are not covered by these numbers.

Reproduce the production suite and native boundary audit, then print the timing:

```sh
mise run native-messages
BIRNAM_TEST_MODULE_DIRECTORY="$PWD/.cargo-target/native-message-tests/debug" \
CARGO_TARGET_DIR="$PWD/.cargo-target/native-message-tests" \
CARGO_PROFILE_DEV_PANIC=unwind \
mise exec -- cargo test --offline -p birnam-siphon --test compiler -- \
  --include-ignored --nocapture
mise exec -- cargo test --offline -p birnam-codegen -p birnam-execution --lib
```

The 13 production integration binaries, native export/install-name/dependency
audit, and 120 Codegen/Execution unit tests pass. Broader numeric result recovery,
loop caching, remaining optimizer families and full workspace/CLI restoration
remain separate work.
