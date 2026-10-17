# Logical discard, explicit files and optional diagnostics

2026-10-17. Follows `bc319081`. Implemented and qualified.

`CompiledUnit discard` returns the unit after releasing its native candidate
handle, preparation tokens and inverse reservation. It does not change installed
Methods/Globals or the module's source registration. Active and prepared handles
can be discarded explicitly: their tokens become unusable and rollback capability
is released. Default status becomes discarded and rollbackAvailable becomes false.
There is no automatic file deletion, collection, checkpoint or physical-disposal
receipt. DiscardReceipt and its native representation/exported accessors are removed.

`CompiledUnit removeFiles` explicitly removes matching admitted artifact and
sidecar contents while the handle is still open. Call it before discard if desired.
It neither discards nor unmaps code. Each deletion is conditional; a later failure
can leave partial file cleanup, and no file rollback is promised. Logical discard
succeeds even if an author has replaced a file or file cleanup would fail.

For optional retirement diagnostics, call `Generation watchCandidateRetirement:`
after mapping and before discard. Existing retainedGenerationCount/Bytes queries
report still-mapped watched groups. Watching records only weak mapping identities;
it does not retain code. Mapping and discard do not invoke diagnostic callbacks,
and unwatched discards create no diagnostic history. Ordinary GC and Execution/TCC
checkpoints retain exclusive responsibility for physical reclamation.

Callback-capable inverse cleanup completes before native ownership is released.
A cleanup Error leaves the handle available for retry. Private-state reentry guards
remain. This does not promise transactional behavior for arbitrary replaced metadata
setters; their effects follow normal message passing. Captured and entered code
remain independently owned even after the candidate handle disappears.

## Evidence

- A Method replaces itself, discards the active handle/inverse, evicts materialization
  caches, invokes GC and completes its old body; later calls use the replacement.
- Discard cancels prepared tokens and leaves changed author-file bytes untouched.
  Mapping/discard work with a deliberately failing diagnostic hook; explicitly
  requesting the diagnostic reports that failure.
- A failed inverse-reservation release preserves the active handle for retry;
  successful release leaves code and projected Globals live but removes undo.
- Native retirement tests separately request file cleanup and weak monitoring.
  Captured Methods survive logical discard and mappings retire after the final
  real owner is released and an ordinary checkpoint runs.
- Native transactions, 62 CLI policy checks, all 826 language assertions,
  independent module changes, parser/compiler-disabled startup and Host
  mapped/rebuilt/resaved/divergent images pass.
- Lean checks 368 theorems with 110 rejected mutations; TLC obtains 241 expected
  outcomes. New models distinguish handle/token release from files, bindings and
  entered-code ownership. Abstract proofs supplement the native lifetime tests.

This completes the generation-lifecycle simplification recommendations. The
Generation library remains the owner of explicit artifact/source facilities;
source certification is optional and does not gate execution or candidate edits.
This is not a claim that all remaining Store-refactor or Rust-to-Birnam work is done.
No user installation, release or push is part of this slice.

The final delivery build is `/tmp/birnam-logical-discard/final-prefix`. Its full
language suite passes all 826 assertions with the updated 20-entry lifecycle help;
the entered-code/discard integration tests also pass against that exact build.
See the [receipt](../measurements/logical-candidate-discard-20261017.json).
