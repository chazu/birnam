# 74AK — Bounded qualification and default selection

The user authorized one hour on 2026-09-14, then authorized enabling optimizations
when the comparisons show no apparent slowdown. This replaces the earlier need
for host-qualified timing before considering these defaults. It does not relax
Id semantics, identity, GC/finalizer, V2, native lifetime or ABI requirements.

Start: 2026-09-15 00:34:17 UTC. Stop by 01:34:17 UTC. Reuse 74AJ's accepted source,
installed-prefix and compatibility checks. No exhaustive predecessor replay.

Finish the existing AI allocator checks (debug fields and four release families),
reusing its three passed debug cases and unchanged diagnostic source. Keep that
instrumentation separate from normal production code. Existing native teardown
and closure ownership evidence remains applicable. Peak RSS in the normal
application comparisons supplements this scoped allocation evidence.

Measure 21 fixed cases, three alternating off/on pairs each: hot/short/refused
numeric chains; counted indices on those same chains; invariant/changing/independent
loops; private/numeric/escaping/refused/single/literal closures; the combined
binding application; aggregate controls; and four ordinary application/cold cases.
Use existing source generators and exact answer/identity checks. The binding
application retains late getter mutation and clone identity checks. No quiet-host
waits, automatic retries or expansion to additional workloads.

A consistent slowdown means a median increase above 5% with at least two of the
three pairs above 5%, in steady execution, execution including explicit GC, or
whole-process time. This pragmatic noise margin is a default-selection rule,
not a statistical confidence claim. A regressing group stays opt-in. Confirm the
selected combination on the application/cold cases before changing defaults.

After selecting defaults, retain explicit environment opt-outs, rebuild the
normal release prefix and check the actual default behavior with focused
application/recovery/control tests. Recheck the ABI export surface. Record each
family as accepted, rejected or inconclusive; incomplete evidence is not a pass.

## Initial comparison

All 126 processes passed their source answer/identity assertions. The initial
three-pair screen leaves unboxed numeric execution, loop-invariant reuse and
counted indices opt-in: short numeric work, changing loops and counted-index
cases show slowdowns under the declared rule. Existing aggregate defaults pass
the binding-application comparison and remain enabled.

The closure/literal group improves hot execution but crosses the GC-inclusive
threshold in the refused and single-use controls. The one selected-combination
confirmation tests closure elimination alone, leaving literal callees and other
optional controls off, on the same fixed cases. This separates the two switches
without changing workloads or adding a new optimization experiment.

## Completed decision

All 252 normal-floor processes passed their correctness assertions: 126 initial
comparisons and 126 selected-combination confirmations. The eight isolated
debug/release allocator cases passed 24 complete Runtime teardown cycles; three
earlier debug cases were reused and the five missing cases completed. Production
source and the accepted installed release remained unchanged throughout.

| Setting | Decision | Representative median change (positive is slower) |
| --- | --- | --- |
| Unboxed numeric intermediates | Keep opt-in | Short chains: +10.5% execution, +20.9% including GC |
| Loop invariant reuse | Keep opt-in | Changing-input loop: +14.1% execution, +13.6% including GC |
| Closure elimination and literal callees | Keep opt-in | Closure-only confirmation: single use +10.3% including GC; ordinary collections +8.7% process time |
| Counted Frame indices | Keep opt-in | Hot chain: +8.2% execution, +6.0% process time |
| Discarded clones and private fields | Keep existing enabled defaults | Binding application: -7.7% execution, -9.5% including GC |

Hot repeated closures show substantial gains (roughly 18–38% in the initial
execution comparisons), but those gains do not erase the control/application
slowdowns. These are unqualified observations under the user's pragmatic rule,
not claims of statistically established whole-program effects.

[The final decisions](../measurements/74ak-default-decisions.json) bind both
comparison receipts, allocator completion and the existing landing acceptance.
The declared no-slowdown screen did not admit any new default, so production
settings were left unchanged. No rebuild or repetition of unchanged semantic
tests was necessary. The interrupted exhaustive replay is still not a pass.

This completes the user-authorized bounded qualification with explicit decisions,
not blanket performance approval. Further promotion requires a targeted admission
or implementation improvement. It must not be pursued by retrying this matrix
until a favorable sample appears.
