# 74AD — Compose recovery consumers with scoped controls and String cleanup

Status: isolated implementation; no main integration or default promotion.

The four optimization families need a coherent runtime, not separate experiments
that have only passed independently. Start from the 74AC controls composition
(`f86a61fd`, documentation checkpoint `f2fab37e`): counted Frame indices, numeric
closure bodies, and mandatory joint sequence/budget/V2/input matrices. Apply only
the three scoped-control Rust changes from 74AA (`dd1bf0b1`) and the four
String-cleanup Rust changes from 74Z (`aca122e8`). Both individual compositions
have passed their full production gates; their installed and cost work remains
independent and does not transfer to this combined runtime.

## Contract

Keep the exact Id implementations, current lookup and mutation checks, shared
Cells and object identities, logical allocation counts/bytes, automatic collection
boundaries, Frame V2 control and native-code ownership. Preserve public IdApi and
Method ABI, canonical descriptors and image formats. No language operation gains
an immutable special case.

Scope a counted-index control value only to an already admitted observation-free
fused invocation. The value is fresh at each invocation; ordinary or observing
publication retains per-PC reads. The test-only cumulative counted-event counter
from 74AC remains available in diagnostic floors. It must neither become a
production counter nor disappear when scoped control reads are composed.

Classify only the exact public-runtime String cleanup callback with the current
object's owned String entry. Cleanup runs once at its existing sweep position.
Unknown/wrapped/replaced finalizers, private runtimes, absent String owners and
dead dictionary retirement retain settlement before observation. Scoped reads
must not suppress these checks or extend across collection or recovery.

## Implementation and evidence

1. Record the exact seven-path input union against the common counted checkpoint.
   Resolve the shared `regions.rs` change by retaining both scoped control reads
   and the existing test/FFI counted-event diagnostics. Preserve every unrelated
   74AC source byte, compiler change, source fixture and required gate.
2. Run compiler and complete floor tests in both profiles. In particular retain
   off/on/off live-control tests, observing-publication fallback, String cleanup
   and replacement-finalizer cases, all recovery/GC/identity checks and original
   stackless lifetime tests. Test success is not production acceptance.
3. Exercise actual combined source consumers with both counting modes, forced
   exits, budget boundaries, V2 capture/suspend, current capture inputs and
   observing method replacement. Add explicit interaction coverage where existing
   tests do not establish that the relevant mechanisms ran together. Require
   exact logical observations; counters may distinguish removed physical work.
4. Run the complete production gate with all 74AC joint matrices, publish a fresh
   normal installation, and repeat installed numeric/Id/aggregate/V2/loop/closure
   and ordinary driver acceptance against that exact build. Existing proofs
   provide prerequisites and regression expectations, not acceptance by transfer.
5. Review predecessor/candidate cost evidence and combined application behavior
   before integration or promotion. No noisy or standalone allocation saving can
   justify a default change. Measurements remain unqualified under the current
   quiet-host waiver.

The shared numeric-prediction descendant is excluded from this checkpoint while
its independent acceptance and cost comparison run. Its eventual inclusion needs
its own explicit source composition and interaction checks. All four families
remain in progress until their complete production and semantic obligations are
met; creating this composition does not redefine those obligations.
