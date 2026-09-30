# 74I — Guard availability through the existing Id API

Status: completed; all eight cases agree in twelve fresh processes against
ordinary debug/release floors. 74H is committed as `5e242e70`.
The original stop decision below was superseded by the user's 2026-09-11
clarification: runtime internals may change while the API, ABI, malleability
and expressibility in Id semantics remain intact. The protocol below preceded
implementation and execution.

## Problem and proposed solution

74F–74H prove recovery using private Runtime guards and test-only entry
selection. Before further optimization, determine whether the existing
interfaces can supply their guard authority to an ordinary native artifact.
The user requires work to stop at a necessary Id API, ABI or language-floor
change. A private function-pointer initializer avoids new exported names,
but does not by itself provide an ordinary floor implementation of that guard.

Run a C client through the frozen `IdApi` header against fresh, ordinary
debug/release floor builds. Use no development feature, private Rust access,
test driver or optimizer callback. This is a boundary experiment, not a
performance comparison or an attempted production optimizer.

## Frozen checks

- Replace a method while preserving receiver vtable identity and selector
  inventory; prove the subsequent ordinary send changes its answer.
- Change inheritance and add a shadowing method while observing the existing
  overwrite hook; prove those changes affect sends without notifying the hook.
- Replace that single hook; prove an earlier observer loses notifications.
- Install a forwarding global lookup with an observable effect. Compare one
  ordinary send with a proposed reflective guard followed by that same send.
  Preserve the answer and count the extra effect, including a lookup sent to
  the metatable itself.
- Collect with no change in live-object count or object vtable, while a real
  root callback changes a native object's payload. Check that a separate
  public root provider can observe the collection: absence of a public epoch
  does not imply absence of all external GC notification mechanisms.
- Check that ordinary floor builds do not export the experimental guarded
  slot helpers or the reverted 74E helpers.
- Run C at `-O0` and `-O2` against both floor profiles, in three fresh
  processes per combination. Require identical observations and retain
  source, compatibility, executable and floor hashes.

These checks can reject particular substitutes for the existing guards.
They cannot prove that every future compiler-only optimization is impossible.
If they expose a required floor mechanism, document that boundary and stop
before adding or promoting it.

## Results

Run `mise run deopt-api-experiment`, or:

```sh
python3 scripts/deopt-public-api.py --output docs/measurements/74i-public-api-guard-boundary.json
```

The [receipt](../../measurements/74i-public-api-guard-boundary.json) records
96 observations: eight cases in each of twelve fresh processes. Each floor
is built with `cargo build -p birnam-id --lib --no-default-features --offline
--locked`, adding `--release` for that profile, in an isolated target directory.
The client compiles with C11, `-Wall -Wextra -Werror`, and either `-O0` or
`-O2`. Each combination runs three times. No Runtime source is changed, and
neither test selection nor the development slot-access feature is compiled
into these floors. All recorded observations agree.

| Proposed substitute | Observed behavior | Consequence |
| --- | --- | --- |
| Receiver vtable and selector inventory | Replacement changes the answer while both remain identical | These are not sufficient binding guards |
| Overwrite callback as a general mutation epoch | Parent changes and new shadowing entries change answers with zero callbacks | It is not a complete mutation signal |
| Install another overwrite observer | The former observer receives zero additional calls | The setter replaces the observer; it does not subscribe or compose |
| Reflective lookup before the original send | The same answer has two lookup effects instead of one | This guard changes ordinary semantics |
| Send lookup to the metatable itself | The replacement lookup runs once | The metacircular base case is not a public, unoverrideable dictionary query |
| Live-object count as a collection epoch | A real root callback changes payload with the same count and vtable | Count equality cannot authorize reuse |
| Public root provider as a GC notification | The observer sees the collection | External notification exists; it does not supply dispatch guard authority |
| Bind existing experimental or reverted slot exports | All six are absent | An ordinary artifact cannot rely on those helpers |

The GC counterexample uses a declared native object's payload, not a private
Frame representation. It runs before the dispatch cases so garbage from
their temporary objects cannot confound the equal-count comparison. The
Frame-specific mutation counterexample remains the independently verified
74H case. This is correctness evidence, with no timing or speedup claim.

## Source audit and implementation boundary

The frozen [IdApi](../../../rust/birnam-abi/src/lib.rs) provides vtable identity,
selector/parent reflection, ordinary sends and callback registration. It has
no field for a mutation epoch, raw dictionary method query, canonical Machine
method identities or optimizer admission. The observed ordinary floor export
inventory also provides no guarded slot access. Its existing registration and
generation-lease symbols establish publication and ownership, not those guards.

In [dispatch.rs](../../../rust/birnam-runtime/src/dispatch.rs), `bind_active`'s lookup
base case resolves the *current* lookup method. The send still invokes that
method. The experiment explicitly exercises this subtlety; recursively asking
the metatable does not escape custom lookup effects.

In [runtime.rs](../../../rust/birnam-runtime/src/runtime.rs),
`guarded_slot_access_eligible` verifies private dictionary entries against the
actual canonical Machine read/write functions and floor lookup function.
The dictionary epoch lives in [vtable.rs](../../../rust/birnam-id/src/id/vtable.rs).
74H's GC epoch and 74F/74G's entry selection are test-only Runtime state.
Changing their compilation gates would change the production floor, even
while leaving the 432-byte IdApi table untouched. Supplying these routines
through an artifact-local initializer would still need an ordinary producer
of those callbacks; the fixture's private Rust access is not that producer.

The overwrite hook is already installed by
[Store](../../../rust/birnam-store/src/lib.rs) for method-history mirroring.
Taking it over would lose that observer, and supported native callers can
replace it later. It cannot silently become optimizer-owned authority.

An artifact-local wrapper can keep baseline and candidate functions in the
same native generation and use the existing StepMethod signature. That is a
possible way to avoid adding driver tier selection. It does **not** solve the
guard-authority problem for the tested read/slot optimizations. Similarly,
root-provider notification is worth retaining as a possible external GC
mechanism; this experiment does not claim that a new floor GC counter is the
only possible implementation.

Capturing method addresses at an ordinary module initializer is insufficient:
native callers may already have changed lookup or Machine methods before
loading that module. This client saves the lookup address immediately after
its own fresh `id_init` solely to forward its test override. A future trusted
bootstrap capture design would need an explicit initialization and authority
contract; this fixture does not establish one for normal artifacts.

## Original decision and subsequent clarification

The following stop decision treated any production Runtime implementation
change as a floor change. The user clarified on 2026-09-11 that internal
implementation changes are allowed, provided the language remains fully
malleable and expressible in Id semantics and the API/ABI stay compatible.
74I's external-interface counterexamples remain valid, but do not block an
optimization inside ordinary dispatch. [74J](74j-internal-guarded-machine-sends.md)
starts that production work without new helper symbols or language primitives.

Stop before promoting the private guard implementation or adding its normal
admission/transport. That would cross the user's production-floor boundary;
exporting additional helpers would also reopen the declined ABI decision.
The needed next design decision is how generated code obtains trustworthy,
non-effectful guard authority while retaining ordinary native mutation and
lookup. No such production contract is introduced by 74I.

Deoptimization remains experimentally proven, not completed as a production
optimizer. 74H also retains its independent performance blocker: no measured
benefit cleared its frozen criterion. These counterexamples reject the
tested shortcuts, not every conceivable future compiler-only optimization.

Validation for 74I is the external client matrix, source/authority hash checks
and `git diff --check`. The prior full `mise run verify` result applies to the
unchanged 74H Rust tree; it is not presented as a new full-gate run for 74I.
