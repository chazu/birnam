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

-- Fixed record fields are owned by object identity. A clone copies references;
-- explicit facts are a separate owner, not an automatic alias of these fields.
structure Records where
  fields : Nat → Nat → Nat
  facts : Nat → Nat

def writeRecord (s : Records) (record field value : Nat) : Records :=
  { s with fields := put s.fields record (put (s.fields record) field value) }

def cloneRecord (s : Records) (original fresh : Nat) : Records :=
  { s with fields := put s.fields fresh (s.fields original) }

theorem record_field_roundtrip (s : Records) (record field value : Nat) :
    (writeRecord s record field value).fields record field = value := by
  simp [writeRecord, put]

theorem clone_retains_field_identity (s : Records) (original fresh field : Nat) :
    (cloneRecord s original fresh).fields fresh field = s.fields original field := by
  simp [cloneRecord, put]

theorem clone_edit_keeps_original (s : Records) (original fresh field value : Nat)
    (distinct : original ≠ fresh) :
    (writeRecord (cloneRecord s original fresh) fresh field value).fields original = s.fields original := by
  simp [writeRecord, cloneRecord, put, distinct]

theorem record_edit_keeps_facts (s : Records) (record field value : Nat) :
    (writeRecord s record field value).facts = s.facts := by rfl

theorem reentrant_other_field_survives (s : Records) (record field other value nested : Nat)
    (distinct : other ≠ field) :
    (writeRecord (writeRecord s record other nested) record field value).fields record other = nested := by
  simp [writeRecord, put, distinct]

-- Test discovery owns an ordered identity registry. These laws assume the
-- ordinary identity predicate; replacing sends remains observable at runtime.
def registerSuite (identity : Nat) (suites : List Nat) : List Nat :=
  if identity ∈ suites then suites else suites ++ [identity]

def unregisterSuite (identity : Nat) (suites : List Nat) : List Nat :=
  suites.filter (fun entry => entry != identity)

theorem suite_registration_idempotent (identity : Nat) (suites : List Nat) :
    registerSuite identity (registerSuite identity suites) = registerSuite identity suites := by
  by_cases h : identity ∈ suites <;> simp [registerSuite, h]

theorem unregistered_suite_absent (identity : Nat) (suites : List Nat) :
    identity ∉ unregisterSuite identity suites := by
  simp [unregisterSuite]

theorem registration_preserves_order (identity : Nat) (suites : List Nat)
    (fresh : identity ∉ suites) :
    registerSuite identity suites = suites ++ [identity] := by
  simp [registerSuite, fresh]

end Birnam.SlotReflection
