# 67 — Query candidate allocation

Status: implemented and verified, 2026-09-05.
Beads: `birnam-query-allocation-plan-u32`.

## Investigation

The canonical relational adapter takes an owned Store snapshot, then evaluates
rule bodies. Each triple candidate currently creates a three-Value Vec (cloning
String bytes), and `extend_rule_solution` copies the existing bindings before
checking whether constants or repeated variables reject the row. Rule calls
already supply a borrowed row but still incur eager binding copies.

The selected slice changes only this pure matching seam. It does not index
Store, reorder clauses, cache query results, alter dispatch or expand IdApi.
Design 42's authority and Design 60J's retention rules remain unchanged.

## Design

Accept a borrowed slice whose elements borrow Values: stack arrays of references
for triples, existing slices of owned Values for rule rows. Use two passes:

1. Validate every term against existing bindings or the first earlier occurrence
   of that variable in the candidate. Constants use the existing `Value::matches`.
   Return on mismatch without allocating or cloning.
2. Copy the input bindings and append each new variable once, in encounter order.

This preserves repeated-variable consistency, first-binding order, identity vs
scalar equality, signed-zero behavior, and NaN behavior. A first unbound NaN
is permitted; a later occurrence fails because NaN does not match itself.
Inputs remain owned by the snapshot and outputs remain independently owned.
No new Rust borrow crosses a language callback.

Validation scans earlier terms for repeats, using O(k²) term work for arity k
and O(k*b) existing-binding lookup for b bindings. Triple arity is three. Wide
rule rows need coverage; a scratch hash map would allocate and is not justified
for this bounded change. Successful rows pay an extra validation pass; measure
all-match as well as selective workloads before claiming throughput gains.

## Implementation plan and acceptance

1. Keep the original eager matcher in tests as a differential oracle. Add a
   thread-local, test-only System allocator counter to reproduce candidate costs.
2. Replace eager triple ownership and defer result construction until validated.
3. Verify constants, repeated variables, prebound conflicts, strings, identity,
   integer/float distinctions, NaN, signed zero, empty/mismatched/wide rows,
   deterministic output and independent result ownership.
4. Assert zero candidate allocations for rejected rows, separating fixed planner
   allocations from matching. Exercise successful rows and existing joins,
   negation, disjunction, recursive rules and aggregates.
5. Run Rust workspace and fresh-prefix Birnam tests plus the production
   verification gate. Record measured allocation evidence separately from
   any timings; no release-performance claim follows from allocation counts.

## Other approved investigations

These remain planning-only, independently selectable in Beads:

- `birnam-store-snapshot-plan-1nj`: Store snapshot scaling.
- `birnam-sorting-plan-byd`: sorting and String comparison contracts.
- `birnam-map-deletion-plan-iis`: persistent deletion and key equality.
- `birnam-peg-matching-plan-ki3`: parse-local PEG representation.

Direct Machine access, operand canonicalization and the rejected inline
initial-slot experiment from Design 66 are outside this slice.

## Allocation evidence

The regression test was run against the original implementation first: 1,000
rejected triples with String values and a pre-existing String binding produced
5,003 allocations (5,000 candidate allocations plus three fixed planner
allocations), failing the zero-candidate-allocation assertion. After the change,
the same canonical evaluator test produces only the three planner allocations.
A successful String-valued triple drops from four allocations to two; output
bindings and their String data remain owned, not borrowed.

The test-only allocator forwards to System and counts only the current test
thread, including reallocations. It is absent from production builds. The eager
matcher remains test-only as an oracle for 98,304 combinations of three terms,
candidate values and initial bindings, checked through both owned-row and
borrowed-triple carriers. Additional cases cover scalar constants, 65-term rows,
length mismatch, empty rows and independent result ownership.

Reproduce with:

```sh
CARGO_TARGET_DIR=.cargo-target cargo test -p birnam-relational --locked --offline -- --nocapture
```

An ignored development timing test compares the old/new candidate paths with
alternating order for successful/rejected scalar/String triples. Its initial
run overlapped builds and was noisy; those timings are not accepted performance
evidence. No end-to-end speedup or release qualification is claimed here.
The reproducible allocation assertions, not wall-clock thresholds, are the
regression gate for this slice.

## Verification

- Focused relational tests: 21 passed, one manual timing test ignored.
- `mise run verify`: passed Rust workspace tests, native-prefix lifecycle gates,
  two independent cold-source builds and relocated-runtime parity.
- The first verify attempt rejected a source-manifest change when test coverage
  was expanded during its build. The source tree was frozen and the entire gate
  rerun successfully; the rejected attempt is not counted as acceptance.
- `mise run test`: fresh-prefix Birnam module suite, 611 passed, zero failed.

Local execution logs: `/tmp/birnam-query-allocation.KiNSRL/` (`focused.log`,
`verify-frozen.log`, `birnam-tests.log`). These temporary logs are supplemental;
the executable regression tests and this result summary are durable evidence.
