import Rooting
import Examples
import GC
import Memo
import Borrow

-- Kernel dependency audit: no sorryAx or project axioms are permitted.
#print axioms Birnam.Rooting.reach_congr
#print axioms Birnam.Rooting.positive_iff_of_bounds
#print axioms Birnam.Rooting.root_equivalence
#print axioms Birnam.Rooting.reachability_equivalence
#print axioms Birnam.Rooting.collector_equivalence
#print axioms Birnam.Rooting.needed_survives
#print axioms Birnam.Rooting.keep_bounds
#print axioms Birnam.Rooting.keepRaw_preserves
#print axioms Birnam.Rooting.deopt_preserves
#print axioms Birnam.Rooting.deopt_restores_exact
#print axioms Birnam.Rooting.deopt_preserves_reachability
#print axioms Birnam.Rooting.guardedKeep_preserves
#print axioms Birnam.Rooting.guard_failure_restores_exact
#print axioms Birnam.Rooting.changePermission_preserves
#print axioms Birnam.Rooting.revocation_restores_exact
#print axioms Birnam.Rooting.unpermitted_exact
#print axioms Birnam.Rooting.acquire_preserves
#print axioms Birnam.Rooting.clearScope_preserves
#print axioms Birnam.Rooting.materializeOne_preserves
#print axioms Birnam.Rooting.materialization_progress
#print axioms Birnam.Rooting.step_preserves
#print axioms Birnam.Rooting.run_preserves
#print axioms Birnam.Rooting.compiler_gc_equivalence
#print axioms Birnam.Rooting.nonessential_iff_coverage
#print axioms Birnam.Rooting.Examples.duplicate_valid
#print axioms Birnam.Rooting.Examples.duplicate_keep_reuses_token
#print axioms Birnam.Rooting.Examples.deoptimization_materializes_duplicate
#print axioms Birnam.Rooting.Examples.stale_guard_falls_back
#print axioms Birnam.Rooting.Examples.partial_materialization_safe
#print axioms Birnam.Rooting.Examples.parent_protection_survives_child_exit
#print axioms Birnam.Rooting.Examples.omitted_first_root_is_invalid
#print axioms Birnam.Rooting.Examples.omitted_first_root_changes_collection
#print axioms Birnam.Rooting.Examples.bare_revocation_changes_observable_counts
#print axioms Birnam.GC.releaseLoop_eq_foldl
#print axioms Birnam.GC.bulk_cleanup_equivalent
#print axioms Birnam.GC.emitPair_preserves
#print axioms Birnam.GC.emitAll_preserves
#print axioms Birnam.GC.dead_inventory_equivalent
#print axioms Birnam.Memo.lookup_sound
#print axioms Birnam.Memo.memoization_equivalent
#print axioms Birnam.Memo.insert_preserves
#print axioms Birnam.Memo.eviction_preserves
#print axioms Birnam.Borrow.copy_contents
#print axioms Birnam.Borrow.pure_inspection_equivalent
