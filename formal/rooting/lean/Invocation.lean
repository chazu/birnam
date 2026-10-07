import Rooting

/-! Invocation input protection. Token histories are not the observation contract;
    actual program Root operations are outside this compiler-private rule.
    Coverage and scope duration are premises, not inferred from C locals.
-/
namespace Birnam.Invocation
open Birnam.Rooting
variable {Object : Type}

abbrev Roots (Object : Type) := Object → Prop

def ordinary (parent inputs locals : Roots Object) : Roots Object :=
  fun o => parent o ∨ inputs o ∨ locals o

def borrowed (parent locals : Roots Object) : Roots Object :=
  fun o => parent o ∨ locals o

def Covered (parent inputs : Roots Object) : Prop := ∀ o, inputs o → parent o

theorem input_root_equivalence (parent inputs locals : Roots Object)
    (covered : Covered parent inputs) :
    ∀ o, ordinary parent inputs locals o ↔ borrowed parent locals o := by
  intro o
  constructor
  · intro h
    rcases h with p | i | l
    · exact Or.inl p
    · exact Or.inl (covered o i)
    · exact Or.inr l
  · intro h
    rcases h with p | l
    · exact Or.inl p
    · exact Or.inr (Or.inr l)

theorem reachable_equivalence (parent inputs locals : Roots Object)
    (covered : Covered parent inputs) (edges : Object → Object → Prop) :
    ∀ o, Reach edges (ordinary parent inputs locals) o ↔
      Reach edges (borrowed parent locals) o :=
  reach_congr (input_root_equivalence parent inputs locals covered)

theorem needed_survives (parent inputs locals heap needed : Roots Object)
    (covered : Covered parent inputs) (edges : Object → Object → Prop)
    (alive : ∀ o, needed o → heap o)
    (live : ∀ o, needed o → Reach edges (ordinary parent inputs locals) o) :
    ∀ o, needed o → collect heap (borrowed parent locals) edges o := by
  intro o h
  exact ⟨alive o h, (reachable_equivalence parent inputs locals covered edges o).mp (live o h)⟩

/-- Removing the child's registrations leaves the covering parent's roots.
    A different scope lifetime requires a new coverage proof. -/
theorem input_survives_child_exit (parent inputs heap : Roots Object)
    (covered : Covered parent inputs) (alive : ∀ o, inputs o → heap o)
    (edges : Object → Object → Prop) :
    ∀ o, inputs o → collect heap parent edges o := by
  intro o h
  exact ⟨alive o h, Reach.root (covered o h)⟩

/-- A new result is not covered merely because the invocation inputs are. -/
theorem unprotected_result_can_be_collected :
    ¬ collect (fun _ : Nat => True) (borrowed (fun _ => False) (fun _ => False))
      (fun _ _ => False) 1 := by
  rintro ⟨_, path⟩
  cases path with
  | root h => exact h.elim id id
  | edge _ impossible => exact impossible

/-- Reject deleting a first real root; duplicate removal needs coverage. -/
theorem uncovered_input_is_not_equivalent :
    ¬ (∀ o : Nat, ordinary (fun _ => False) (fun v => v = 1) (fun _ => False) o ↔
      borrowed (fun _ => False) (fun _ => False) o) := by
  intro h
  have missing := (h 1).mp (Or.inr (Or.inl rfl))
  exact missing.elim id id

end Birnam.Invocation
