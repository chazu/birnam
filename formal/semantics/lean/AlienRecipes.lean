/-! Recipe metadata describes dictionary bindings; it is never a second method
    authority. Identity stamps are non-rooting observations supplied by Id, not
    native addresses. Image projection omits obsolete registrations and recovery
    stamps the actual destination Method. These are default-owner contracts, not
    restrictions on replacing the owner's methods. -/
namespace Birnam.AlienRecipes

def project (current : Option Nat) (stamp recipe : Nat) : Option Nat :=
  if current = some stamp then some recipe else none

theorem project_current (stamp recipe : Nat) :
    project (some stamp) stamp recipe = some recipe := by simp [project]

theorem project_obsolete (current : Option Nat) (stamp recipe : Nat)
    (changed : current ≠ some stamp) : project current stamp recipe = none := by
  simp [project, changed]

def record (current : Option Nat) (installed : Nat) (previous : Option Nat) : Option Nat :=
  if current = some installed then some installed else previous

theorem registration_current (installed : Nat) (previous : Option Nat) :
    record (some installed) installed previous = some installed := by simp [record]

theorem reentrant_registration_preserved (current : Option Nat) (installed : Nat)
    (previous : Option Nat) (changed : current ≠ some installed) :
    record current installed previous = previous := by simp [record, changed]

inductive Reference where
  | object : Nat → Reference
  | stamp : Nat → Reference

def roots : Reference → List Nat
  | .object value => [value]
  | .stamp _ => []

theorem stamp_not_root (stamp : Nat) : roots (.stamp stamp) = [] := by rfl

theorem stamp_preserves_held_roots (held : List Nat) (stamp : Nat) :
    held ++ roots (.stamp stamp) = held := by simp [roots]

def restoredStamp (_savedStamp destinationMethod : Nat) : Nat := destinationMethod

theorem restored_stamp_is_destination (saved destination : Nat) :
    restoredStamp saved destination = destination := by rfl

theorem restored_stamp_projects (saved destination recipe : Nat) :
    project (some destination) (restoredStamp saved destination) recipe = some recipe := by
  simp [project, restoredStamp]

def replay {α : Type} (cache _registered : α) (saved destination : Nat) : α × Nat :=
  (cache, restoredStamp saved destination)

theorem replay_preserves_cache {α : Type} (cache registered : α) (saved destination : Nat) :
    (replay cache registered saved destination).1 = cache := by rfl

theorem replay_renews_registration {α : Type} (cache registered : α) (saved destination : Nat) :
    (replay cache registered saved destination).2 = destination := by rfl

def fillMissing (existing : Option Nat) (pending : Nat) : Option Nat :=
  match existing with
  | some value => some value
  | none => some pending

theorem image_index_preserves_callback (existing pending : Nat) :
    fillMissing (some existing) pending = some existing := by rfl

theorem image_index_fills_missing (pending : Nat) :
    fillMissing none pending = some pending := by rfl

end Birnam.AlienRecipes
