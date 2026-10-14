import PrivateScratch

/-! Value/ownership model for authored registry records. Values are opaque Nat
    identities; None models an absent/nil getter result. Map protocols, scheduling,
    GC, public fact aliases and image codecs are not proved by these equations. -/
namespace Birnam.ModuleRegistry
open Birnam.PrivateScratch
abbrev Record := Nat → Option Nat
abbrev Registry := Nat → Record
structure State where
  records : Registry
  facts : Registry

def write (s : State) (name field value : Nat) : State :=
  { s with records := put s.records name (put (s.records name) field (some value)) }

def flatWrite (facts : Nat × Nat → Option Nat) (name field value : Nat) :=
  put facts (name, field) (some value)
def flatten (records : Registry) : Nat × Nat → Option Nat := fun key => records key.1 key.2

theorem field_write_matches_flat_getters (s : State) (name field value : Nat) :
    flatten (write s name field value).records = flatWrite (flatten s.records) name field value := by
  funext key
  rcases key with ⟨n, f⟩
  by_cases hn : n = name <;> by_cases hf : f = field <;>
    simp_all [flatten, write, flatWrite, put]

theorem field_read_after_write (s : State) (name field value : Nat) :
    (write s name field value).records name field = some value := by simp [write, put]

theorem unrelated_field_preserved (s : State) (name field other value : Nat) (h : other ≠ field) :
    (write s name field value).records name other = s.records name other := by simp [write, put, h]

theorem unrelated_record_preserved (s : State) (name other field value : Nat) (h : other ≠ name) :
    (write s name field value).records other = s.records other := by simp [write, put, h]

theorem explicit_facts_unchanged (s : State) (name field value : Nat) :
    (write s name field value).facts = s.facts := by rfl

/-- Field codes 0/1/2/3 stand for registered/kind/requires/loaded. -/
def declare (s : State) (name dependencies : Nat) :=
  write (write (write s name 0 1) name 1 1) name 2 dependencies

theorem declaration_preserves_loaded (s : State) (name dependencies : Nat) :
    (declare s name dependencies).records name 3 = s.records name 3 := by simp [declare, write, put]

def kernel (s : State) (name : Nat) :=
  write (write (write s name 0 1) name 1 2) name 3 1

theorem kernel_preserves_requirements (s : State) (name : Nat) :
    (kernel s name).records name 2 = s.records name 2 := by simp [kernel, write, put]

theorem loaded_does_not_register (s : State) (name : Nat) :
    (write s name 3 1).records name 0 = s.records name 0 := by simp [write, put]

theorem snapshot_is_not_replaced (s : State) (name field value : Nat) :
    let snapshot := s.records
    let _ := write s name field value
    snapshot = s.records := by rfl

end Birnam.ModuleRegistry
