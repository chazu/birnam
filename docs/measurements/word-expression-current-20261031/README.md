# Current Method and allocation checks for Word expressions

Baseline `85ffaac9`, 2026-10-31. This is a correctness repair and removal of unsound
bookkeeping. It does not make Integer arithmetic fast or qualify the inventory port.

## Reproduced failures

The previous Word expression implementation remembered admitted primitive meanings
in an epoch-indexed bitset. The [regression](../../../rust/birnam-siphon/tests/word_expression_mutation.rs)
uses messages throughout: Method reference reporting, ObjectModel slots, dictionary
installation and current Method comparison. No foreign payload is read or written.

Three cases matter:

- After warming an expression, changing a primitive Method's context without editing
  its dictionary makes a direct call fail. The old expression nevertheless computes
  its remembered arithmetic result. Both ordinary and explicitly stamped expression
  entrypoints must instead decline.
- An admission callback can change a previously checked primitive during the same
  expression. A negative-control build with a fresh per-call bitset still produces
  the wrong result. The final implementation checks again at the later operation.
- An admission callback can change the allocator's Method data while leaving the
  epoch unchanged. The initial allocation permission then becomes stale. A plan
  ending in a comparison previously returned a Boolean while omitting the changed
  constructor's effects. It now declines, and an ordinary fallback observes those
  effects.

The fixtures exercise currently admitted mutable Method storage; they do not turn
its present field layout or declaration ordinals into language requirements.

## Final implementation

Before:

```text
initial permission -> input reads -> reuse epoch-cached meanings -> epoch check -> result
```

After:

```text
initial permission -> input reads -> current check at each operation
  -> current allocation permission -> epoch check -> result or refusal
```

The cached epoch/meaning tuple and bitset bookkeeping are removed. The same
constructor, permission and primitive protocols remain replaceable. No registry,
new object family, native ABI operation or language immutability rule is added.
Epochs still detect dictionary changes, but are not proof of immutable Method data.

Id's Method comparison also avoids reading the same object's metadata twice within
one callback-free comparison. It validates the selected Method first, then matches
an identical candidate directly. Distinct Methods still receive structural comparison,
and malformed Methods do not become valid merely by being compared to themselves.
This retains no proof across calls or callbacks.

## Performance and disposition

The existing arithmetic probe was extended by this [small patch](expression-probe.patch)
with one-operation, four mixed-operation and four repeated-operation expression
plans. Setup, compilation, selector interning and correctness checks are outside
timing. Each sample averages 1000 ordinary sends and Host result protection.
Baseline/candidate/candidate/baseline ordering gives ten samples per variant and
GC mode; no builds or tests ran during the final timings.

| Automatic GC enabled | Unsafe baseline | Corrected implementation |
| --- | ---: | ---: |
| One expression operation | 2.54 µs | 3.90 µs |
| Four mixed expression operations | 2.55 µs | 4.33 µs |
| Four repeated expression operations | 2.58 µs | 4.31 µs |
| Ordinary Integer addition | 41.97 µs | 41.45 µs |

Expression allocations remain two per measured call, including the probe boundary;
Integer addition remains at 49. The small addition difference is not a substantial
speedup claim. These are specific native expression entrypoints, not an application
benchmark or a claim about all generated arithmetic.

**Disposition:** accept the cost of the current checks rather than retain a shortcut
that computes stale results or omits constructor effects. The old fast path is not
a valid correctness baseline. The local identity comparison recovers some matching
cost without adding persistent state, but does not remove the regression relative
to the unsound cache. Further performance work must preserve the demonstrated
mutation and constructor behavior.

## Validation scope

The final implementation passes 323 workspace library tests, the workspace check,
and 17 focused integration targets covering expressions, mutable Methods, numeric
policy, native conversion, roots, image slots/refusal and snapshot mutation. The
independent Id continuation suite passes for the unchanged final Id comparison.

Full image roundtrip and 20 resave epochs passed after removing the primitive cache
and changing Id comparison, **before** the final allocation-permission recheck.
The final focused image targets were rerun; the full resave loop was not repeated
for that expression-only check. This is not release qualification or installation.

[result.json](result.json) identifies sources, libraries, raw evidence and test scope.
Raw logs remain under `.cargo-target/measurements/word-expression-current-20261031`.
The old epoch cache and the per-call-bitset alternative fail the mutation
probes. Production callers, Integer policy, bootstrap requirements and image formats
are unchanged. No push or user-prefix installation is claimed.
