import PrivateScratch

/-! Ordinary-slot mirror laws. Names and values belong to the live owner; index
    membership and the selected names are premises supplied by owner protocols.
    No assertion of atomic enumeration, compiler correctness or GC is made.
    Removing the old libDir fact alias is an explicit API transition, not modeled
    as equivalence of arbitrary Store queries or old fact mutation effects. -/
namespace Birnam.SlotReflection
open Birnam.PrivateScratch

structure Owner where
  names : Nat → Option Nat
  slots : Nat → Nat
  facts : Nat → Nat

def edit (s : Owner) (index value : Nat) : Owner :=
  { s with slots := put s.slots index value }

def rename (s : Owner) (index name : Nat) : Owner :=
  { s with names := put s.names index (some name) }

theorem edit_is_actual_storage (s : Owner) (index value : Nat) :
    (edit s index value).slots index = value := by simp [edit, put]

theorem edit_preserves_other_slots (s : Owner) (index other value : Nat)
    (different : other ≠ index) :
    (edit s index value).slots other = s.slots other := by simp [edit, put, different]

theorem edit_does_not_publish_facts (s : Owner) (index value : Nat) :
    (edit s index value).facts = s.facts := by rfl

theorem rename_preserves_storage (s : Owner) (index name : Nat) :
    (rename s index name).slots = s.slots := by rfl

def nameAt (s : Owner) (index : Nat) := s.names index

theorem new_description_uses_current_name (s : Owner) (index name : Nat) :
    nameAt (rename s index name) index = some name := by simp [nameAt, rename, put]

/-- Inventory rows copy names, not closures reading future owner metadata. -/
theorem old_description_is_snapshot (s : Owner) (index newName : Nat) :
    let description := nameAt s index
    let _ := rename s index newName
    description = s.names index := by rfl

/-- Absent and stored nil retain Module's previous getter default. The new slot
    exposes stored nil directly; the getter alone supplies the default. -/
def directory (stored : Option Nat) (defaultPath : Nat) := stored.getD defaultPath

theorem nil_directory_uses_default (defaultPath : Nat) :
    directory none defaultPath = defaultPath := by rfl

theorem directory_rebase_uses_new_path (newPath defaultPath : Nat) :
    directory (some newPath) defaultPath = newPath := by rfl

end Birnam.SlotReflection
