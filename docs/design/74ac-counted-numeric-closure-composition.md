# 74AC — Counted Frame indices with numeric closure bodies

Status: isolated composition in progress; both development switches remain
independently default-off. No integration or promotion.

The counted-index implementation is integrated on main after its full production
and installed acceptance. The numeric closure implementation at `6ecce1b0` has
passed its full production gate. Their separate proofs do not establish the
behavior of a closure constructor executing numeric callees while both passes
are active. Compose these exact implementations on main `5e667907` and close that
interaction gap before numeric closures join main.

## Composition and invariants

Carry the numeric compiler plans, exact callee provenance, checked warm entry
prediction and physical callee Frame execution from `6ecce1b0`. Keep main's
counted-index ledger, numeric-journal reverse lookup and quiet aggregate callers.
Their only shared source edit is the production inventory/validator script;
merge its input lists and checks without dropping either acceptance surface.
Check the exact union of both source inventories and keep ABI, canonical
artifacts, modules and the Error implementation unchanged.

Every logical allocation charge, collection threshold, collection/finalizer
observation, exposed identity and shared Cell must retain canonical meaning.
The numeric closure path must retain real callee entry leases, physical Frames,
original action publication and V2 return/recovery. Counted indices remain
already-dead, unexposed canonical slot operands; they cannot stand in for numeric
results or a closure identity. Baseline and observing/escaping paths remain
ordinary execution. Both controls must remain independently switchable.

## Execution and acceptance

1. Compose the verified sources and audit their union. Run debug/release floor
   and compiler suites on the actual combined source.
2. Add an explicit counted-index option to the numeric closure source, sequence,
   budget, input and capture/suspend checks. Their ordinary default inputs must
   remain byte-identical. Verify counting off/on with closure baseline, enabled
   and forced recovery, both GC modes and every supported call exit.
3. Keep physical savings attributable: closure probes currently report every
   deferred allocation as `physical_saved`. Counted indices need a separate
   diagnostic count, so original closure-savings assertions remain exact after
   subtracting only independently checked index savings. Do not weaken checks
   by merely allowing a larger total. Preserve original logical charge and
   collection comparisons, including budgets and stale/changed methods.
4. Make the joint numeric closure matrices required by this composition's full
   production gate. Then publish a fresh normal prefix and run counted installed
   acceptance and ordinary numeric-closure driver checks against that exact build.
5. Only complete correctness permits integration with defaults unchanged.
   Application, resource and exploratory profitability evidence are still needed
   before default promotion or completion of either optimization family.

The String cleanup and scoped control-read changes are absent. The shared numeric
prediction descendant remains on its independent acceptance/cost path; introducing
it into this composition requires the same joint checks. Existing frozen jobs
and installations remain intact. No qualified timing is claimed.


## Initial composition evidence

The composition preserves the exact union of both production inventories: 304
inputs. All 27 numeric-side source changes and all 19 counted-side paths retain
their accepted contents except the deliberately merged production inventory and
validator script, their only intersecting edit. `74ac-composition-source-audit.json`
records the source union and both original production proof hashes.

Fresh debug/release runs each pass 92 compiler tests and 220 floor tests, with
15 floor tests ignored. `74ac-composition-tests.json` retains the command output
and exact source bindings. This establishes that the two implementations build
and retain their existing individual tests together. Joint enabled source,
accounting, GC and V2-control checks remain the next required work.
