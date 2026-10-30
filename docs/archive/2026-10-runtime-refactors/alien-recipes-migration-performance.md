# Alien recipe ownership: qualification and accepted startup cost

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-19, baseline `7ec21979`. Completed in `9aa4843d` after the user
accepted the remaining startup cost on 2026-10-20. The initial vector regression
and its approved remediation are retained below as measurement history.

## Initial delivered versus draft boundary

Direct Method publication is committed (`7ec21979`): Alien no longer calls Store
for method installation. The next draft removes `__aliens__` facts, synthetic
attribute keys and Image's fact-table recipe discovery. An authored AlienRecipes
owner separates function/signature object-code reuse from current registrations.

Registration metadata holds Id's non-reused identity stamps, preserving native
code reclamation. Current dictionaries decide which registrations belong in a
snapshot. Replaced/removed wrappers are omitted, and restored Methods receive
new stamps. Normal sends use current dictionaries; stamps are consulted during
publication bookkeeping and snapshot construction. Image invokes the restored
owner after ordinary method bindings have been applied.

The first build exposed a wrong native Namespace lookup for the authored owner.
A minimal probe established that the owner lived in Globals; correcting that
lookup allowed source wrapping and the complete installed build to pass.

## Measurements

Three interleaved normal-release observations, default GC:

| Measurement | Baseline | Vector draft | Change |
| --- | ---: | ---: | ---: |
| CPU for 20 repeated wraps | 697 ms | 1,211 ms | +74% |
| Fresh source-file startup CPU | 2.782 s | 3.404 s | +22% |
| Fresh source-file startup wall | 2.888 s | 3.674 s | +27% |

The draft performs linear scans for cache lookup and rewrites the entire cache
and registration vectors on successful wrapping. This work also repeats while
reconstructing the startup image's wrappers.

Diagnostic-only overrides on a cloned owner isolate the costs. Actual C compilation,
loading, publication and invocation remain in place; these are not valid production
implementations because they omit bookkeeping needed by later images.

| Candidate diagnostic, 20 wraps | CPU median |
| --- | ---: |
| Ordinary cloned owner | 1,170 ms |
| Suppress object-code cache rewriting | 1,017 ms |
| Suppress all registration bookkeeping | 839 ms |

The reductions implicate both cache rebuilding and registration maintenance.
Linear cache lookup remains in the final diagnostic; removing just one operation
is unlikely to recover all of the regression.

## Approved next work

- Use ordinary nested Maps for function/signature code lookup and target/selector
  registrations. Keep the existing public Id/Method owner protocols.
- Skip cache writes when the selected cached byte object is already present.
- Build process-local target keys from Id identities, with destination keys rebuilt
  during image restoration. Persist plain recipe rows, not native Method roots or
  process identity authority.
- Preserve cache data independently of restoring active Methods: replay should
  renew registration stamps without overwriting an independently edited code cache.
- Capture immutable field indices/dependency receiver values where useful, while
  every operation remains a current send.
- Repeat absence, callback, GC, alias, image, startup and timing checks before
  committing the runtime migration. No Store fallback, native registry or method
  guards on ordinary invocation are proposed.

## Current evidence and remaining checks

Real compiled owner initialization and actual Alien wrapping/code reuse pass with
Store, Relational, Bootstrap, Image and Isolate physically absent. Publication,
collecting reentry, nested-wrapper preservation and owner-policy errors pass.
Mapped/rebuilt/resaved images preserve current foreign and authored definitions,
omit removed wrappers and retain owner aliases. REPL source setup, borrowed-stream
lifecycle and mapped/rebuilt images pass. Native FFI/callback checks confirm exact
wrapper-Unit reclamation. Installed console startup performs no compilation.

The formal gate passes **413 Lean theorems, 132 rejected mutations and 282 expected
TLC outcomes**. New contracts cover stale registration omission, nested publication,
non-rooting stamps, independently held code and destination-stamp renewal. These
are abstract owner contracts, supplemented by runtime checks; they are not a
whole-compiler or foreign-C correctness proof.

Image-owner discovery before Map rehashing remains a qualification question. An
initial probe gave an individual Symbol private behavior; the existing image codec
rejected that shape before loading, so it neither establishes nor rules out an
ordering defect for supported hash changes. Test a supported case before final
image qualification; do not silently infer owner absence from a not-yet-rehashed
lookup or break foreign methods used by key hashing.

The complete Store-independent Host/worker/image composition and release remain
unfinished. Exact measurements, source hashes and evidence are in the
[receipt](../../measurements/alien-recipes-regression-20261019.json).

## Indexed owner: functional qualification and remaining startup decision

The approved Map remediation is implemented. Native Alien has no Store calls.
Code reuse and registration descriptions are ordinary, inspectable/mutable Maps;
image projection produces plain active recipe rows and strips process-local keys.
Registration stamps do not retain native Methods. Snapshot selection remains the
point where current dictionaries decide which descriptions are active.

The image checks exposed a real initialization-order defect: a changed Symbol hash
could make a restored Globals lookup miss the recipe owner, dropping wrappers.
The source key/value iterator also performed hidden hashed lookups. The final
implementation removes those workarounds and uses an explicit owner envelope:
`#[#imageOwnersV1 #[#sourceRegistryV2 sourceOwner] foreignOwnerOrNil]` in the existing
Sources payload. It restores foreign bindings before repairing hashes, then builds
ordinary registration indexes. Both identity-dependent and foreign-function-backed
hashes now pass mapped and rebuilt recovery. The prior direct source-owner payload
is rejected before application; rebuilding images is required.

Foundational Error predicates capture their boolean values, so default checks can
run without resolving a half-restored Globals Map. Error messages remain current
sends, including on instances with private behavior. Temporary image-operation
roots retain newly constructed Methods until indexing; the persistent owner retains
only identity stamps. Indexing fills missing entries without overwriting registrations
made by callbacks. Independent edits/aliases in cached byte objects survive recovery.

All 847 language checks pass. Real Store-absent wrapping, publication and private
Errors, reentrant registration, image reentry/resave, code-cache aliases, both hashing
cases, REPL lifecycle/images, Globals absence, task/worker fields and no-compilation
startup pass. Native FFI/callback/reclamation checks, image round trips, 20 chained
mapped/rebuilt resaves, partial-load conflicts and snapshot mutation pass. The formal
gate passes **417 theorems, 134 mutation rejections and 287 expected TLC outcomes**.
These remain abstract-contract proofs plus separate implementation evidence.

Final three-pair normal-release observations:

| Measurement | Committed baseline | Final draft | Change |
| --- | ---: | ---: | ---: |
| 20 repeated wraps, CPU | 653.90 ms | 648.19 ms | -0.9% |
| Fresh source-file startup, CPU | 2.470 s | 2.711 s | +9.8% / 241 ms |
| Fresh source-file startup, wall | 2.478 s | 2.714 s | +9.5% / 236 ms |

The repeated-wrap regression is resolved. The remaining startup increase is real
enough to report separately, rather than declare the migration performance-qualified.
Native-only startup is unchanged in the diagnostic, locating the extra work in
image reconstruction. A phase probe found a current-method report allocation crossing
the unchanged 16 MiB GC threshold. Image now queries resident binding keys directly
through Id/Execution instead of constructing unused implementation/provenance rows.
That removes temporary allocation, but did not remove the end-to-end startup delta;
the collection cost and additional image work still matter. GC thresholds and
scaling were not changed.

### Decision accepted (2026-10-20)

Recommend accepting this **temporary 0.24 s startup cost**, committing the now
functionally qualified owner migration, and proceeding to the already planned
removal of mandatory Store/Image staging and optional-tool bootstrap dependencies.
Those changes should be measured independently; this report does not promise they
will recover the cost. The alternative is to hold this runtime slice uncommitted
and continue profiling image reconstruction before advancing.

The user approved continuing even if this remaining time cannot be recovered.
The qualified runtime migration is accepted with the measured cost recorded;
future slices retain independent correctness and performance qualification. Exact measurements and check receipts are in the
[indexed-owner receipt](../../measurements/alien-recipes-indexed-20261019.json).
