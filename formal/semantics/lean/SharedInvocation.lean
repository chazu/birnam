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

end Birnam.SharedInvocation
