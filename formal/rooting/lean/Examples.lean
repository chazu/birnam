import Rooting

namespace Birnam.Rooting.Examples
open Birnam.Rooting

def duplicate : State Unit Unit where
  logical := fun _ _ => 2
  physical := fun _ _ => 1
  optimized := fun _ => true
  witness := fun _ => 7
  permitted := true
  epoch := 7

theorem duplicate_valid : Invariant duplicate := by
  constructor
  · intro s o
    simp [duplicate, compact]
  · intro s h
    simp [duplicate]

theorem duplicate_keep_reuses_token :
    (guardedKeep duplicate () ()).physical () () = 1 ∧
    (guardedKeep duplicate () ()).logical () () = 3 := by
  simp [guardedKeep, duplicate, keepRaw, keepPhysical, update]

theorem deoptimization_materializes_duplicate :
    (changePermission duplicate false).physical () () = 2 ∧
    (changePermission duplicate false).permitted = false := by
  simp [changePermission, deopt, duplicate]

/-- Stale witnesses cannot justify the next optimized registration. -/
theorem stale_guard_falls_back :
    (guardedKeep {duplicate with epoch := 8} () ()).physical () () = 3 := by
  simp [guardedKeep, duplicate, keepRaw, keepPhysical, deopt, update]

/-- Incremental recovery is safe even if collection happens halfway through. -/
theorem partial_materialization_safe :
    Invariant (materializeOne (keepRaw duplicate () ()) () ()) :=
  materializeOne_preserves (keepRaw_preserves duplicate_valid () ()) () ()

/-- An escaped result is first rooted in the caller; then the child can exit. -/
def handedOff : State Bool Unit where
  logical := fun _ _ => 1
  physical := fun _ _ => 1
  optimized := fun _ => false
  witness := fun _ => 0
  permitted := false
  epoch := 0

theorem parent_protection_survives_child_exit :
    roots (clearScope handedOff true).physical () := by
  exact ⟨false, by simp [clearScope, handedOff]⟩

/-- Root coverage fails for a needed object if its only registration is omitted.
    This is a proved counterexample, not an intentionally unproved theorem. -/
def missing : State Unit Unit :=
  {duplicate with logical := fun _ _ => 1, physical := fun _ _ => 0}

theorem omitted_first_root_is_invalid : ¬ Invariant missing := by
  intro h
  have impossible := h.1 () ()
  simp [missing, duplicate, compact] at impossible

theorem omitted_first_root_changes_collection :
    collect (fun _ : Unit => True) (roots missing.logical) (fun _ _ => False) () ∧
    ¬ collect (fun _ : Unit => True) (roots missing.physical) (fun _ _ => False) () := by
  constructor
  · exact ⟨True.intro, Reach.root ⟨(), by simp [missing, duplicate]⟩⟩
  · rintro ⟨_, path⟩
    cases path with
    | root hr => simp [roots, missing, duplicate] at hr
    | edge _ edge => exact edge

/-- Lifetime remains safe, but observed multiplicity differs after an invalid
    revocation. Changing an epoch alone is insufficient recovery. -/
theorem bare_revocation_changes_observable_counts :
    let bad := {duplicate with permitted := false, epoch := 8}
    (∀ o, roots bad.physical o ↔ roots bad.logical o) ∧
    bad.physical () () ≠ bad.logical () () := by
  constructor
  · intro o
    simp [roots, duplicate]
  · decide

end Birnam.Rooting.Examples
