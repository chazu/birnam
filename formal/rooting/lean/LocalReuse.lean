import Capture

/-! Compiler-private root reuse. Token/callback multiplicity is not this
    observation relation; explicit program Root operations are unchanged.
    Hints refer only to protection that lasts through the entire current call. -/
namespace Birnam.LocalReuse
open Birnam.Rooting
variable {Object : Type}

def Covered (edges : Object → Object → Prop) (roots hints : Object → Prop) : Prop :=
  ∀ o, hints o → Reach edges roots o

theorem alias_root_is_redundant
    (edges : Object → Object → Prop) (roots hints : Object → Prop)
    (safe : Covered edges roots hints) :
    ∀ o, Reach edges (Birnam.Capture.withResults roots hints) o ↔ Reach edges roots o :=
  Birnam.Capture.reachable_results_do_not_need_separate_roots edges roots hints safe

/-- Successful acquisition establishes coverage before a hint is inserted. -/
theorem successful_retain_covers_hint
    (edges : Object → Object → Prop) (roots hints : Object → Prop)
    (safe : Covered edges roots hints) (value : Object) :
    Covered edges (fun o => roots o ∨ o = value) (fun o => hints o ∨ o = value) := by
  intro o h
  rcases h with old | fresh
  · have lift : ∀ v, Reach edges roots v → Reach edges (fun o => roots o ∨ o = value) v := by
      intro v path
      induction path with
      | root r => exact Reach.root (Or.inl r)
      | edge _ e ih => exact Reach.edge ih e
    exact lift o (safe o old)
  · exact Reach.root (Or.inr fresh)

/-- Bounded-cache eviction changes hints, never the covering registrations. -/
theorem hint_eviction_preserves_coverage
    (edges : Object → Object → Prop) (roots hints remaining : Object → Prop)
    (safe : Covered edges roots hints) (subset : ∀ o, remaining o → hints o) :
    Covered edges roots remaining := by
  intro o h
  exact safe o (subset o h)

theorem covered_aliases_survive
    (edges : Object → Object → Prop) (heap roots hints : Object → Prop)
    (safe : Covered edges roots hints) (alive : ∀ o, hints o → heap o) :
    ∀ o, hints o → collect heap roots edges o := by
  intro o h
  exact ⟨alive o h, safe o h⟩

/-- A pointer remembered after its producing scope exits is not a root. -/
theorem stale_hint_is_not_coverage :
    ¬ Covered (fun _ _ : Nat => False) (fun _ => False) (fun o => o = 3) := by
  intro h
  have path := h 3 rfl
  cases path with
  | root r => contradiction
  | edge _ e => contradiction
end Birnam.LocalReuse
