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

/-- Native owners report private temporary roots through their GC provider;
    the same live values need not also be registered in the public registry. -/
def ownerRoots (parent reported : Roots Object) : Roots Object :=
  fun o => parent o ∨ reported o

theorem owner_local_root_equivalence
    (parent locals reported : Roots Object)
    (reports : ∀ o, reported o ↔ locals o)
    (edges : Object → Object → Prop) :
    ∀ o, Reach edges (ownerRoots parent reported) o ↔
      Reach edges (fun o => parent o ∨ locals o) o := by
  apply reach_congr
  intro o
  simp only [ownerRoots]
  rw [reports o]

theorem owner_local_values_survive
    (parent locals reported heap : Roots Object)
    (reports : ∀ o, reported o ↔ locals o)
    (alive : ∀ o, locals o → heap o) (edges : Object → Object → Prop) :
    ∀ o, locals o → collect heap (ownerRoots parent reported) edges o := by
  intro o h
  exact ⟨alive o h, Reach.root (Or.inr ((reports o).mpr h))⟩

/-- Nested scopes discard their appended suffix, retaining the parent's exact
    ordered inventory, including duplicate references. -/
theorem nested_owner_exit_preserves_parent (parent child : List Object) :
    (parent ++ child).take parent.length = parent := by simp

def canReturnDirect (tokens : Nat) (selectorReady : Bool) : Bool :=
  (tokens == 0) && selectorReady

theorem direct_return_requires_empty_cleanup (tokens : Nat) (selectorReady : Bool) :
    canReturnDirect tokens selectorReady = true ↔ tokens = 0 ∧ selectorReady = true := by
  simp [canReturnDirect]

/-- A cleanup owner holds the result even after its original token is
    removed. This is an overlap scope, not an additional logical retain. -/
theorem cleanup_owner_covers_result
    (parent reported : Roots Object) (edges : Object → Object → Prop)
    (result : Object) (covered : reported result) :
    Reach edges (ownerRoots parent reported) result := Reach.root (Or.inr covered)

theorem cleanup_result_survives_callback
    (heap parent reported : Roots Object) (edges : Object → Object → Prop)
    (result : Object) (alive : heap result) (covered : reported result) :
    collect heap (ownerRoots parent reported) edges result :=
  ⟨alive, cleanup_owner_covers_result parent reported edges result covered⟩

end Birnam.Invocation
