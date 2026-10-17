/-! Code sharing is separate from per-Method binding and current message lookup.
    These equations model the forwarding obligation, not the compiler, heap or
    frame engine. Opaque Nat values stand for existing object identities. -/
namespace Birnam.SharedInvocation
structure Method where
  binding : Nat
  code : Nat
  implementation : Nat
structure World where
  lookup : Nat → Nat
  captures : Nat → List Nat
structure Observation where
  selected : Nat
  arguments : List Nat
  captures : List Nat
  deriving DecidableEq

def forwarding (w : World) (implementation receiver : Nat) (args : List Nat) : Observation :=
  ⟨w.lookup implementation, receiver :: args, w.captures implementation⟩
def shared (w : World) (method : Method) (receiver : Nat) (args : List Nat) :=
  forwarding w method.implementation receiver args

theorem same_current_send (w : World) (m : Method) (r : Nat) (args : List Nat) :
    shared w m r args = forwarding w m.implementation r args := by rfl

theorem code_address_irrelevant (w : World) (a b implementation r : Nat) (args : List Nat) :
    shared w ⟨0, a, implementation⟩ r args = shared w ⟨0, b, implementation⟩ r args := by rfl

theorem receiver_is_first (w : World) (m : Method) (r : Nat) (args : List Nat) :
    (shared w m r args).arguments = r :: args := by rfl

theorem captures_keep_identity (w : World) (m : Method) (r : Nat) (args : List Nat) :
    (shared w m r args).captures = w.captures m.implementation := by rfl

theorem current_lookup_is_used (w : World) (m : Method) (r : Nat) (args : List Nat) :
    (shared w m r args).selected = w.lookup m.implementation := by rfl

def sample : World := ⟨fun i => i + 10, fun i => [i, i]⟩

theorem one_mutable_implementation_is_wrong :
    shared sample ⟨1, 0, 1⟩ 9 [] ≠ shared sample ⟨2, 0, 2⟩ 9 [] := by decide

theorem freezing_lookup_is_wrong :
    (shared sample ⟨1, 0, 1⟩ 9 []).selected ≠
    (shared { sample with lookup := fun i => i + 20 } ⟨1, 0, 1⟩ 9 []).selected := by decide
theorem binding_identity_not_merged (a b code implementation : Nat) (h : a ≠ b) :
    (⟨a, code, implementation⟩ : Method).binding ≠
    (⟨b, code, implementation⟩ : Method).binding := h
-- Construction selects code once, but creates new logical binding state.
def bind (template : Method) (identity implementation : Nat) : Method :=
  ⟨identity, template.code, implementation⟩

def directRoots (m : Method) : List Nat := [m.code, m.implementation]

theorem binding_uses_selected_code (t : Method) (identity implementation : Nat) :
    (bind t identity implementation).code = t.code := by rfl

theorem binding_uses_new_implementation (t : Method) (identity implementation : Nat) :
    (bind t identity implementation).implementation = implementation := by rfl

theorem binding_has_fresh_identity (t : Method) (identity implementation : Nat)
    (fresh : identity ≠ t.binding) :
    (bind t identity implementation).binding ≠ t.binding := fresh

theorem binding_roots_code_and_new_body (t : Method) (identity implementation : Nat) :
    directRoots (bind t identity implementation) = [t.code, implementation] := by rfl

theorem rebinding_does_not_chain_old_implementations (t : Method) (a b x y : Nat) :
    directRoots (bind (bind t a x) b y) = directRoots (bind t b y) := by rfl

-- Factory cache contains selected code, never a user's implementation. A miss
-- compiles once; publication merges into the post-callback cache snapshot.
def publishTemplate (latest : Nat → Option Nat) (arity code : Nat) : Nat → Option Nat :=
  fun key => if key = arity then some code else latest key

theorem template_publication_keeps_reentrant_other_arity
    (latest : Nat → Option Nat) (arity code other : Nat) (distinct : other ≠ arity) :
    publishTemplate latest arity code other = latest other := by simp [publishTemplate, distinct]

def templateResult (latest : Nat → Option Nat) (arity : Nat) (result : Except Nat Nat) :=
  match result with
  | .error error => (latest, Except.error error)
  | .ok code => (publishTemplate latest arity code, Except.ok code)

theorem template_error_keeps_cache (latest : Nat → Option Nat) (arity error : Nat) :
    (templateResult latest arity (.error error)).1 = latest := by rfl

theorem template_error_keeps_identity (latest : Nat → Option Nat) (arity error : Nat) :
    (templateResult latest arity (.error error)).2 = Except.error error := by rfl

def imageTemplates (_live : Nat → Option Nat) : Nat → Option Nat := fun _ => none

theorem image_omits_process_local_templates (live : Nat → Option Nat) (arity : Nat) :
    imageTemplates live arity = none := by rfl

-- Cache publication compares owned snapshot identities, not service methods.
-- A conflict only declines caching; the already selected code is still usable.
def publishOwnedCache (expected current next : Nat) :=
  if current = expected then next else current

theorem cache_conflict_keeps_current (expected current next : Nat) (h : current ≠ expected) :
    publishOwnedCache expected current next = current := by simp [publishOwnedCache, h]

def updatePrivateCache (state : Nat × Nat) (next : Nat) : Nat × Nat := (state.1, next)

theorem private_cache_keeps_global_binding_basis (state : Nat × Nat) (next : Nat) :
    (updatePrivateCache state next).1 = state.1 := by rfl

-- A selected payload is authority; equal arity alone cannot replace it.
def reusablePayload (arity payload otherArity otherPayload : Nat) : Prop :=
  arity = otherArity ∧ payload = otherPayload

theorem shared_payload_preserves_selection (a p b q : Nat)
    (h : reusablePayload a p b q) : p = q := h.2

theorem equal_arity_does_not_authorize_other_code :
    ¬ reusablePayload 1 10 1 20 := by simp [reusablePayload]

-- Each synchronous Unit owns mutable implementation state. Only resumable
-- templates can use immutable shared code plus per-binding implementations.
def sharesSuppliedTemplate (synchronous : Bool) : Bool := !synchronous

theorem synchronous_unit_is_not_shared : sharesSuppliedTemplate true = false := by rfl

def publishActivation (current : Option Nat) (created : Nat) : Option Nat :=
  match current with
  | some winner => some winner
  | none => some created

theorem nested_activation_winner_survives (winner created : Nat) :
    publishActivation (some winner) created = some winner := by rfl

-- Ordinary mutation is separate from an explicitly prepared transaction.
structure EditState where
  binding : Nat
  epoch : Nat
  facts : Nat
  deriving DecidableEq

def directEdit (s : EditState) (method : Nat) : EditState :=
  { s with binding := method, epoch := s.epoch + 1 }

def applyPrepared (observed : Nat) (s : EditState) (method : Nat) : EditState :=
  if s.epoch = observed then directEdit s method else s

theorem ordinary_edit_does_not_assert_facts (s : EditState) (m : Nat) :
    (directEdit s m).facts = s.facts := by rfl

theorem ordinary_edit_changes_future_binding (s : EditState) (m : Nat) :
    (directEdit s m).binding = m := by rfl

theorem stale_preparation_cannot_overwrite (observed : Nat) (s : EditState) (m : Nat)
    (stale : s.epoch ≠ observed) :
    applyPrepared observed s m = s := by simp [applyPrepared, stale]

theorem edit_invalidates_old_preparation (s : EditState) (edit planned : Nat) :
    applyPrepared s.epoch (directEdit s edit) planned = directEdit s edit := by
  simp [applyPrepared, directEdit]

-- Generation tokens observe values; a global dispatch epoch is not a lease.
-- The older EditState model above remains useful for owners explicitly choosing
-- revision-conditioned mutation (such as the source registry).
structure AdmissionBasis where
  binding : Nat
  target : Nat
  projectedGlobal : Option Nat
  metadata : Nat
  sourceRecord : Nat
  deriving DecidableEq

structure AdmissionWorld extends AdmissionBasis where
  unrelated : Nat
  dictionaryEpoch : Nat
  deriving DecidableEq

def observeAdmission (s : AdmissionWorld) : AdmissionBasis :=
  ⟨s.binding, s.target, s.projectedGlobal, s.metadata, s.sourceRecord⟩

def admitObserved (basis : AdmissionBasis) (s : AdmissionWorld) : Prop :=
  observeAdmission s = basis

theorem unrelated_activity_keeps_admission (s : AdmissionWorld) (noise epoch : Nat) :
    admitObserved (observeAdmission s) { s with unrelated := noise, dictionaryEpoch := epoch } := by
  rfl

theorem changed_binding_rejects_admission (s : AdmissionWorld) (next : Nat)
    (changed : next ≠ s.binding) :
    ¬ admitObserved (observeAdmission s) { s with binding := next } := by
  intro same
  exact changed (congrArg AdmissionBasis.binding same)

theorem changed_target_rejects_admission (s : AdmissionWorld) (next : Nat)
    (changed : next ≠ s.target) :
    ¬ admitObserved (observeAdmission s) { s with target := next } := by
  intro same
  exact changed (congrArg AdmissionBasis.target same)

theorem present_nil_is_not_absence (s : AdmissionWorld)
    (absent : s.projectedGlobal = none) :
    ¬ admitObserved (observeAdmission s) { s with projectedGlobal := some 0 } := by
  intro same
  have h := congrArg AdmissionBasis.projectedGlobal same
  change some 0 = s.projectedGlobal at h
  simp [absent] at h

theorem changed_metadata_rejects_admission (s : AdmissionWorld) (next : Nat)
    (changed : next ≠ s.metadata) :
    ¬ admitObserved (observeAdmission s) { s with metadata := next } := by
  intro same
  exact changed (congrArg AdmissionBasis.metadata same)

theorem changed_source_record_rejects_admission (s : AdmissionWorld) (next : Nat)
    (changed : next ≠ s.sourceRecord) :
    ¬ admitObserved (observeAdmission s) { s with sourceRecord := next } := by
  intro same
  exact changed (congrArg AdmissionBasis.sourceRecord same)

def setterAccepted (expected actual : Nat) (reportedError : Bool) : Bool :=
  actual == expected && !reportedError

theorem setter_noop_is_rejected (old expected : Nat) (different : old ≠ expected) :
    setterAccepted expected old false = false := by simp [setterAccepted, different]

theorem setter_error_is_preserved (expected actual : Nat) :
    setterAccepted expected actual true = false := by simp [setterAccepted]

-- An explicit batch observes its selected bindings. Whole-ledger certification
-- is a separate predicate; even a tracked outside binding is outside this scope.
structure ScopedWorld where
  selected : Nat
  outside : Nat
  deriving DecidableEq

def scopedEdit (s : ScopedWorld) (expected replacement : Nat) : ScopedWorld :=
  if s.selected = expected then { s with selected := replacement } else s

theorem scoped_edit_preserves_outside (s : ScopedWorld) (expected replacement : Nat) :
    (scopedEdit s expected replacement).outside = s.outside := by
  simp only [scopedEdit]; split <;> rfl

theorem scoped_edit_accepts_outside_drift (old outside replacement : Nat) :
    (scopedEdit ⟨old, outside⟩ old replacement).selected = replacement := by
  simp [scopedEdit]

theorem scoped_edit_rejects_selected_conflict (s : ScopedWorld) (expected replacement : Nat)
    (changed : s.selected ≠ expected) :
    scopedEdit s expected replacement = s := by simp [scopedEdit, changed]

theorem scoped_undo_preserves_later_outside (old installed outside : Nat) :
    scopedEdit ⟨installed, outside⟩ installed old = ⟨old, outside⟩ := by simp [scopedEdit]

theorem scoped_edit_does_not_certify_outside (old outside replacement recorded : Nat)
    (drift : outside ≠ recorded) :
    (scopedEdit ⟨old, outside⟩ old replacement).outside ≠ recorded := by
  simpa [scopedEdit] using drift

-- Certification observes the world; it does not rewrite lifecycle state.
structure CertificateWorld where
  binding : Nat
  recorded : Nat
  sourceRevision : Nat
  recordedRevision : Nat
  adopted : Bool
  status : Nat
  undo : Bool
  deriving DecidableEq

def inspectCertificate (s : CertificateWorld) : CertificateWorld × Bool :=
  (s, s.adopted && s.binding == s.recorded && s.sourceRevision == s.recordedRevision)

def ordinaryStart (s : CertificateWorld) : CertificateWorld := s

def registerSource (s : CertificateWorld) : CertificateWorld :=
  { s with sourceRevision := s.sourceRevision + 1 }

theorem certification_preserves_world (s : CertificateWorld) :
    (inspectCertificate s).1 = s := by rfl

theorem ordinary_start_preserves_adoption_choice (s : CertificateWorld) :
    (ordinaryStart s).adopted = s.adopted := by rfl

theorem source_registration_not_blocked_by_adoption (s : CertificateWorld) :
    (registerSource s).sourceRevision = s.sourceRevision + 1 := by rfl

theorem source_registration_keeps_inverse (s : CertificateWorld) :
    (registerSource s).undo = s.undo := by rfl

theorem restored_binding_can_validate_again (s : CertificateWorld)
    (adopted : s.adopted = true) (source : s.sourceRevision = s.recordedRevision) :
    (inspectCertificate { s with binding := s.recorded }).2 = true := by
  simp [inspectCertificate, adopted, source]

-- Candidate source exchange changes one module, not a saved whole-world table.
def replaceModule (world : Nat → Nat) (key expected replacement : Nat) : Nat → Nat :=
  if world key = expected then fun k => if k = key then replacement else world k else world

theorem module_update_preserves_other (world : Nat → Nat) (key other expected replacement : Nat)
    (different : other ≠ key) :
    replaceModule world key expected replacement other = world other := by
  by_cases h : world key = expected
  · simp [replaceModule, h, different]
  · simp [replaceModule, h]

theorem module_update_rejects_changed_source (world : Nat → Nat) (key expected replacement : Nat)
    (changed : world key ≠ expected) :
    replaceModule world key expected replacement = world := by simp [replaceModule, changed]

theorem module_update_installs_selected (world : Nat → Nat) (key replacement : Nat) :
    replaceModule world key (world key) replacement key = replacement := by simp [replaceModule]

-- An image records current executable state. Revision metadata is descriptive
-- after drift and must not reactivate old transaction authority on restoration.
structure WorldSnapshot where
  code : Nat
  recordedRevision : Nat
  managedCurrent : Bool
  deriving DecidableEq

def snapshotLive (eligible : Bool) (code revision : Nat) : WorldSnapshot :=
  ⟨code, revision, eligible && code == revision⟩

def restoreLive (image : WorldSnapshot) : Nat × Bool :=
  (image.code, false)

-- Loading does not establish transaction authority, even for a matching revision.
theorem matching_image_has_no_managed_authority (code : Nat) :
    (restoreLive (snapshotLive true code code)).2 = false := by rfl

def certifyBaseline (requested : Bool) (code source : Nat) : Bool :=
  requested && code == source

theorem baseline_requires_explicit_request (code source : Nat) :
    certifyBaseline false code source = false := by simp [certifyBaseline]

theorem explicit_baseline_rejects_different_code (code source : Nat) (h : code ≠ source) :
    certifyBaseline true code source = false := by simp [certifyBaseline, h]

theorem explicit_matching_baseline_can_be_adopted (code : Nat) :
    certifyBaseline true code code = true := by simp [certifyBaseline]

theorem snapshot_uses_live_code (eligible : Bool) (code revision : Nat) :
    (restoreLive (snapshotLive eligible code revision)).1 = code := by rfl

theorem restored_image_has_no_managed_authority (eligible : Bool) (code revision : Nat) :
    (restoreLive (snapshotLive eligible code revision)).2 = false := by rfl

theorem descriptive_image_cannot_reactivate (code revision : Nat) :
    (restoreLive (snapshotLive false code revision)).2 = false := by simp [restoreLive, snapshotLive]

def finishSnapshot (before after oldFile proposedFile : Nat) : Nat :=
  if before = after then proposedFile else oldFile

theorem changed_snapshot_keeps_old_file (before after oldFile proposedFile : Nat)
    (changed : before ≠ after) :
    finishSnapshot before after oldFile proposedFile = oldFile := by simp [finishSnapshot, changed]

-- Explicit inverse admission observes owner data, never dependency Method identity.
def imageInverse (current revision observed before expected : Nat) : Nat :=
  if revision = observed ∧ current = expected then before else current

theorem image_inverse_keeps_later_revision (current revision observed before expected : Nat)
    (changed : revision ≠ observed) :
    imageInverse current revision observed before expected = current := by
  simp [imageInverse, changed]

theorem image_inverse_keeps_foreign_value (current revision observed before expected : Nat)
    (changed : current ≠ expected) :
    imageInverse current revision observed before expected = current := by
  simp [imageInverse, changed]

theorem image_inverse_restores_owned_write (current revision before : Nat) :
    imageInverse current revision revision before current = before := by
  simp [imageInverse]

theorem image_inverse_rejects_aba (value revision before : Nat) :
    imageInverse value (revision + 2) revision before value = value := by
  simp [imageInverse]

end Birnam.SharedInvocation
