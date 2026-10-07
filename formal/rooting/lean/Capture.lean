import Rooting

/-! A live initialized Block covers its immutable capture edges. Replacement
readers returning fresh values require ordinary result protection. These are
reachability rules with explicit premises, not a proof of the generated C ABI. -/
namespace Birnam.Capture
open Birnam.Rooting
variable {Object : Type}

def withResults (base results : Object → Prop) : Object → Prop :=
  fun o => base o ∨ results o

theorem reachable_results_do_not_need_separate_roots
    (edges : Object → Object → Prop) (base results : Object → Prop)
    (covered : ∀ o, results o → Reach edges base o) :
    ∀ o, Reach edges (withResults base results) o ↔ Reach edges base o := by
  intro o
  constructor
  · intro path
    induction path with
    | root h => exact h.elim Reach.root (covered _)
    | edge _ e ih => exact Reach.edge ih e
  · intro path
    induction path with
    | root h => exact Reach.root (Or.inl h)
    | edge _ e ih => exact Reach.edge ih e

theorem capture_edge_covers_result
    (edges : Object → Object → Prop) (base : Object → Prop)
    (block result : Object) (live : base block) (capture : edges block result) :
    Reach edges base result := Reach.edge (Reach.root live) capture

theorem borrowed_needed_survives
    (edges : Object → Object → Prop) (heap base results needed : Object → Prop)
    (covered : ∀ o, results o → Reach edges base o)
    (alive : ∀ o, needed o → heap o)
    (reachable : ∀ o, needed o → Reach edges (withResults base results) o) :
    ∀ o, needed o → collect heap base edges o := by
  intro o h
  exact ⟨alive o h, (reachable_results_do_not_need_separate_roots edges base results covered o).mp (reachable o h)⟩

-- A fresh replacement result is not protected by keeping only its Block alive.
theorem fresh_result_requires_coverage :
    ¬ collect (fun _ : Nat => True) (fun o => o = 0) (fun _ _ => False) 1 := by
  rintro ⟨_, path⟩
  cases path with
  | root h => omega
  | edge _ h => contradiction
end Birnam.Capture
