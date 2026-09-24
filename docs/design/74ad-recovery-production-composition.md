# 74AD — Compose recovery consumers with scoped controls and String cleanup

> **Allocation policy superseded (2026-09-24):**
> [Design 90](90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history)
> retires baseline allocation/count/GC-timing equivalence, dead-payload replay and
> prepaid deferred identities. Preserve language effects, observable identity,
> recovery and actual resource lifetimes. Older algorithms, receipts and commands
> below record history; they do not require restoring that machinery. See the
> [Self implementation review](90-self-optimization-review.md).

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
   Preserve the numeric-closure `execute_prepared` entry when composing
   `fused_numeric.rs`; retain both scoped control reads and the existing test/FFI
   counted-event diagnostics in `regions.rs`. Preserve every unrelated
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


## Initial composed runtime

The source audit confirms 306 inputs and exactly the seven declared Rust changes.
Five files equal their incoming source versions byte-for-byte. `fused_numeric.rs`
is the clean three-way composition, retaining the numeric-closure prepared entry
and placing the scoped control value inside that admitted execution. `regions.rs`
retains the existing 74AC diagnostic counter's test/FFI lifetime and wrapping
increment while adding the scoped read. The initial audit expected six direct
copies and correctly rejected the fused entry as an additional overlap; the
corrected audit verifies both compositions explicitly.

`74ad-composition-tests.json` records 92 compiler tests and 222 runtime tests
passing in each profile, with 16 runtime tests ignored by the general runner.
The public-runtime String test is exercised through its parent subprocess test.
The seven-path composition does not add or skip tests, weaken the production
gate, change public ABI, or promote defaults. Joint source interactions and full
production/installed acceptance remain the next requirements.

The composed release source matrix completed all 1,148 processes in the
predecessor `da7ed262`: 40 V2-control, 20 changed-input, 448 GC-budget and 640
sequence cases. The independent `74ad-joint-release-audit.json` binds its 306
source inputs, release diagnostic floor, installed compiler artifacts, all eight
exact coordinate sets and counting off/on comparisons (commit `1731b757`).
It reuses the unchanged installed numeric compiler with an explicit new floor
overlay; this is release source acceptance, not a fresh combined normal
installation. The interaction descendant below adds explicit collection and
observer cases before proceeding to complete production verification.

## Owned String cleanup during source-produced deferred allocations

The interaction descendant adds an optional diagnostic collection to the existing
real compiled-closure sequence probe. After warm-up it creates one owned String,
runs the unchanged numeric closure source, retains only the actual returned value
for the new collection, and checks that the returned closure/capture still works.
The String remains outside the existing outer native-root interval. Ordinary
probe cases gain no root or collection. No production API or runtime behavior
changes; the existing settlement counter becomes available in diagnostic floors.

Three cases cover exact cleanup, an observing wrapper, and replacement by that
wrapper after optimized execution. Exact cleanup must release String storage
without realizing any deferred identities. Both observing cases must settle all
pending identities before the callback. Counting-enabled source cases must have
positive counted-index savings; closure allocation savings and all existing
result, alias, byte and recovery checks remain required.

The first fixture incorrectly required identical live counts inside finalizers
across independent processes. It failed even though pre/post collection counts
matched. Eight repeated observations establish that baseline physical allocation
already varies: its callback sees 2,718 through 3,783 live objects while every
run starts with 3,845 and ends with 2,716. The existing 74T contract leaves sweep
order and addresses unspecified. The original failed 12-row receipt and all eight
diagnostic runs remain retained; this is a fixture assumption failure, not proof
of a runtime accounting defect.

The corrected check records each callback's remaining dead objects and bytes,
then requires its live observations to equal those remaining amounts plus the
final retained heap. Thus every callback observation is checked in its own sweep.
Raw values remain in the receipt. Pre/post logical counts and bytes, reclaimed
counts and callback effects still compare exactly across counting modes. No
retries or additional roots are used to hide an observation mismatch.

All 120 initial corrected release cases pass. Both full matrices also pass:
240 cases per build profile, 480 overall, including 33-call sequences and the
final eligible forced exit. They span both GC modes, escaping/non-escaping
results, counting off/on and baseline/enabled/first/second/final forced recovery. The production
gate now requires both complete matrices and their raw-output/source/accounting
admission checks, adding 480 cases to the previous 6,656-case gate. Twelve
admission checks accept the full actual release receipt and reject missing,
duplicate, stale, mismatched, unexercised or incorrectly accounted evidence.
`74ad-cleanup-interactions-full.json` also passes the complete runtime suites:
222 tests in each profile, with 16 ignored. The independent
`74ad-cleanup-interactions-audit.json` revalidates every interaction coordinate,
source/install/floor binding and unit log against the current 307 inputs.

The source-bound acceptance queue (`74ad-interactions-acceptance-start.json`)
has verified that exact interaction/unit process finished successfully. It
rechecked all 307 inputs, both complete matrices, diagnostic floors, installed
compiler artifacts and unit logs, then started the 75-command, 7,136-case
production gate. A fresh normal release installation, 1,848 focused installed cases, 240
normal/test-floor driver cases and 20 ordinary changed-input cases follow only
successful prerequisites. This is a correctness dependency, not a quiet-host
wait. No production or installed acceptance is claimed until those jobs finish.
