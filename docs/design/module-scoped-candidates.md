# Module-scoped candidate changes

2026-10-17. Follows `8c1e3bc8`. Implemented and qualified.

Candidates no longer require or advance a DispatchGeneration or its certification
ledger. Compilation binds ABI-5 candidate envelopes to the registered revision
of that module only. Identity, dependencies and residual initializer compatibility
remain checked; arbitrary initializers are not silently replayed. Introduce a new
module normally before using the candidate replacement facility.

Preparation accepts `[action, unit]`. It captures the actual selected Methods,
projected Globals, lifecycle fields and that module's immutable source record.
Commit and conditional rollback replace only that source row, preserving unrelated
registration and callback effects. There is no whole-registry lease; source-record
A-to-B-to-A restoration between checks is permitted, just like restored bindings.
The per-pass observation stamps remain for detecting mixed callback observations.

The undo record holds selected before/after bindings, Globals, source records and
the prior candidate handle, rather than two whole-world ledgers. Different modules
can activate and roll back independently. Same-module source conflicts and shared
binding conflicts remain real dependencies. `CompiledUnit activeFor:` queries the
existing candidate ownership registry against the current module source; it is
not a second authority for dispatch. Image loading still discards process-local
capabilities while preserving live code and descriptions.

`DispatchGeneration adoptCurrent` is now independent, renewable source
certification. It projects the currently registered sources and validates them
against live code; it neither blocks candidate operations nor changes their undo
records. Certification can become stale after an edit and be refreshed explicitly.
It does not promise arbitrary state/initializer equivalence or native byte identity.
The former successor-projection send and mandatory managed-binding admission are
removed. Old candidate ABI-4 bundles must be recompiled.

Native candidate operations still reject reentry into partially updated private
state. Image cancellation/abandonment cannot clear that state during a candidate
operation. Code lifetimes, owner access, actual setter postconditions, selective
compensation and optimizer invalidation remain unchanged. Host-only activation and
discard's file/diagnostic coupling are the next slices, not part of this commit.

## Evidence

- Two module candidates compile/activate without any certificate; another module
  is introduced; the first two roll back out of global order. Certification can
  be established/refreshed without disrupting the remaining undo.
- Expanded native transaction checks cover unrelated source registration surviving
  commit/rollback, exact Methods/Globals, callbacks, returned Errors, native failure,
  absent/nil, no-op setters, GC and retained side-effect Methods in both execution modes.
- Candidate mapping, retirement, selected-source conflicts and optional ledger
  validation pass. Host mapped/rebuilt/resaved and divergent images pass.
- All 826 language assertions and 61 compiled-unit CLI checks pass. Candidate
  codec/codegen tests pass with the new ABI.
- Lean checks 361 theorems and 105 mutation controls; TLC obtains 232 expected
  outcomes. The new laws/model cover selected-row replacement, independent source
  changes and conditional undo. The model is not a proof of the native loader.

Migrating the fault hook from the removed generation setter to the unit setter
exposed unrooted test Symbols. Removing implicit ledger retention exposed raw
outside Method entries that the test had not rooted. Both fixtures now retain
what they own; the production code already roots these values through owner scopes.
Formal verification was rerun after proof edits on stable inputs. See the
[receipt](../measurements/module-scoped-candidates-20261017.json).
