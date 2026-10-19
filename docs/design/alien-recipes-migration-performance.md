# Alien recipe ownership: approved performance remediation

2026-10-19, baseline `7ec21979`. The ordinary-owner draft passes its current
functional and formal checks, but remains uncommitted after a significant
performance regression. The user approved indexed ordinary-Map remediation.

## Delivered versus draft

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
[receipt](../measurements/alien-recipes-regression-20261019.json).
