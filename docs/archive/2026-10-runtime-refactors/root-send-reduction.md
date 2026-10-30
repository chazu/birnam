# Further root-send reduction

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

The preceding work was committed as `41d2e007` on main. This follow-up reduces
individual Root sends while retaining the ordinary root-token representation,
duplicate visibility, cleanup ordering, current-owner messages and cold bootstrap.

## Changes

- Id `nativeBindingsMatch:` checks up to 32 current native meanings in one
  callback-free owner query. Inputs are native-endian Word quadruples containing
  receiver/selector addresses and opaque method/context addresses. Only Id reads
  its heap and dictionaries; contexts are compared, never inspected or invoked.
  Custom lookup, unknown addresses, mismatched meanings and saturated epochs
  decline. Malformed ABI sizes reject before access. No public ABI struct changed.
- Roots `tryRetainMany:` returns borrowed native token Words in the original input
  order. Null inputs, capacity failure or denied proof return Word 0 without any
  acquisition, allowing the original per-object prefix/failure path. The caller
  copies the returned Words immediately. Capacity is checked after proof dispatch,
  because dispatch itself may reenter before the callback-free query body runs.
- Roots `tryReleaseManyForward:` preserves Word's forward cleanup order. Missing
  or repeated tokens decline before mutation; fallback preserves the original
  failure position. Generated cleanup retains its reverse-order API and now
  offers batches at four tokens instead of eight, using the cheaper guard.
- Word batches input scopes of at least three values and forward cleanup through
  Roots messages. It stores no foreign payload view and introduces no GC provider.
- NativeBackend bit 16 groups validated incoming receiver/argument roots (at most
  nine objects). The default is mask 31. Invalid arity/type and guard rejection
  retain the original individual path. Other message results are still rooted
  immediately; this is not deferred rooting across an arbitrary callback.

The incoming-root contract requires caller protection throughout invocation,
which the execution owner supplies for compiled method bodies. Native callers
already must keep their protocol, objects, arguments, code and owners live for
that invocation. The new APIs do not infer that a C local is a GC root.

## Measurements and interpretation

Compared with the preceding committed sixteen-addition census:

| Individual sends per addition | Committed | Follow-up |
| --- | ---: | ---: |
| retain: | 738.50 | 650.25 |
| release: | 466.25 | 381.00 |
| combined | 1204.75 | 1031.25 |

That is about 14.4% fewer individual retain/release sends. Logical registrations
remain 738.5 per addition. The observed maximum live-token count and distinct
object-address count match the ordinary path, and zero observed tokens remain.
The new group and metadata messages are part of the cost: the full optimized
message census falls from 4023.75 to approximately 3911.06 messages per addition,
about 2.8%, not 14.4% overall. Counts include the fixture wrapper amortized over
sixteen additions and are not CPU-time percentages.

The four-second paired mask-15/31 experiment reports no meaningful CPU difference
for incoming-root batching alone (ratio 0.996; interval 0.992–1.004). The extra
batching's demonstrated benefit here is fewer dispatch boundaries, not a large
measured arithmetic speedup. A separate compatible-prefix comparison retains
committed Root/Word dylibs as a reference; those independent process timings are
descriptive and must not be represented as a paired confidence estimate.

The evidence is `docs/measurements/root-send-reduction-20261007.json` and raw
logs under `/tmp/birnam-retain-slice/`. Diagnostic prefixes are separate from the
installed prefix; startup images were not rebuilt for measurements.

## Validation

The formal checks add exact ordered registration and capacity-prefix proofs to
existing ordered release proofs. TLA+ negative cases catch unprotected inputs
and declined batches that mutate state. Rust executes the actual C prelude,
checks capacity after reentrant proof dispatch, compares snapshots/token ids,
and verifies that Id declines a custom metabehavior without calling its lookup.
The existing Word, helper, ABI, Integer and GC tests retain independent semantic,
replacement and collection checks. These are abstract proofs plus implementation
checks, not a complete machine-code or runtime verification.

This work does not remove the native Rust needed for bootstrap, replace authored
Integer policy with native arithmetic, elide Frames, inspect another owner's
payload or dispatch using a cached foreign executable. Logical duplicate-root
coalescing still needs its observer/revocation contract and is not enabled here.
