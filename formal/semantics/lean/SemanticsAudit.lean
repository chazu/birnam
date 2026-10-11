import SlotHints
import Relational
import IndexedSnapshot
import Semantics
import SemanticsExamples
import Admission
#print axioms Birnam.Semantics.capture_valid
#print axioms Birnam.Semantics.unchanged_dependencies_preserve
#print axioms Birnam.Semantics.permission_revocation_invalidates
#print axioms Birnam.Semantics.replacement_invalidates
#print axioms Birnam.Semantics.certified_dispatch
#print axioms Birnam.Semantics.mutation_does_not_rewrite_entered
#print axioms Birnam.Semantics.protected_inputs_survive
#print axioms Birnam.Semantics.visible_effect_cannot_be_erased
#print axioms Birnam.Semantics.compose_refinement
#print axioms Birnam.Semantics.sequence_refinement
#print axioms Birnam.Semantics.recovery_preserves_prefix
#print axioms Birnam.Semantics.cons_run_bound
#print axioms Birnam.Semantics.repeated_owner_collapse
#print axioms Birnam.Semantics.owner_boundary_lower_bound
#print axioms Birnam.Semantics.bounded_schedule_forget
#print axioms Birnam.Semantics.bounded_owner_lower_bound
#print axioms Birnam.SemanticsExamples.addition_owner_floor
#print axioms Birnam.SemanticsExamples.string_owner_floor
#print axioms Birnam.SemanticsExamples.collection_owner_floor
#print axioms Birnam.SemanticsExamples.addition_abstract_schedule
#print axioms Birnam.SemanticsExamples.string_abstract_schedule
#print axioms Birnam.SemanticsExamples.collection_abstract_schedule
#print axioms Birnam.SemanticsExamples.valid_initial
#print axioms Birnam.SemanticsExamples.epoch_change_alone_is_not_invalidation
#print axioms Birnam.SemanticsExamples.same_epoch_revoked_permission_is_invalid
#print axioms Birnam.SemanticsExamples.changed_dependency_is_invalid
#print axioms Birnam.SemanticsExamples.first_refines
#print axioms Birnam.SemanticsExamples.second_refines
#print axioms Birnam.SemanticsExamples.composed_example_refines
#print axioms Birnam.SemanticsExamples.dropping_visible_effect_is_invalid
#print axioms Birnam.SemanticsExamples.unary_integer_access_schedule
#print axioms Birnam.Admission.packed_sound
#print axioms Birnam.Admission.selected_equivalent
#print axioms Birnam.Admission.changed_service_uses_fallback
#print axioms Birnam.Admission.changed_context_declines
#print axioms Birnam.Admission.custom_lookup_declines
#print axioms Birnam.Admission.guard_behavior_equivalent
#print axioms Birnam.Admission.successful_guard_saves_one
#print axioms Birnam.Admission.declined_guard_cost_is_explicit
#print axioms Birnam.Admission.admitted_guard_refines

#print axioms Birnam.Admission.metadata_query_requires_current
#print axioms Birnam.Admission.nonzero_metadata_identity_requires_shape
#print axioms Birnam.Admission.unknown_metadata_identity_declines
#print axioms Birnam.Admission.changed_metadata_preserves_legacy
#print axioms Birnam.Admission.selected_shape_preserves_result

#print axioms Birnam.Admission.packed_pair_sound
#print axioms Birnam.Admission.selected_pair_equivalent
#print axioms Birnam.Admission.revoked_permission_blocks_commit
#print axioms Birnam.Admission.changed_epoch_blocks_commit
#print axioms Birnam.Admission.pair_admission_saves_three

#print axioms Birnam.SemanticsExamples.unguarded_box_elision_is_not_a_refinement
#print axioms Birnam.SemanticsExamples.changed_conversion_binding_invalidates

#print axioms Birnam.Admission.metadata_batch_requires_every_shape
#print axioms Birnam.Admission.unknown_batch_identity_declines
#print axioms Birnam.Admission.metadata_batch_saves_inputs_minus_one
#print axioms Birnam.IndexedSnapshot.roots_roundtrip
#print axioms Birnam.IndexedSnapshot.edges_roundtrip
#print axioms Birnam.IndexedSnapshot.graph_policy_preserved
#print axioms Birnam.IndexedSnapshot.inverse_implies_injective
#print axioms Birnam.IndexedSnapshot.resume_keeps_prefix
#print axioms Birnam.SemanticsExamples.reusable_fold_without_guard_is_not_refinement
#print axioms Birnam.Relational.stopped_error
#print axioms Birnam.Relational.stopped_mismatch
#print axioms Birnam.Relational.row_fold_refines_reference
#print axioms Birnam.Relational.new_variable_preserves_identity
#print axioms Birnam.Relational.seen_first_true_stops
#print axioms Birnam.Relational.projection_keeps_originals
#print axioms Birnam.Relational.replacement_between_comparisons_is_observed
#print axioms Birnam.Relational.nonreflexive_equality_is_not_repaired
#print axioms Birnam.SlotHints.lookup_of_member
#print axioms Birnam.SlotHints.validated_hint_equals_current_lookup
#print axioms Birnam.SlotHints.replacement_value_is_read
#print axioms Birnam.SlotHints.shifted_key_falls_back
