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

end Birnam.SharedInvocation
