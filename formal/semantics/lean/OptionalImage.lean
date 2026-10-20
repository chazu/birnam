/-! Optional data participation, independent of heap/code transport. Owner and
    revision are observations, not dispatch authority. This models admission and
    selected writes, not arbitrary user callbacks or the production codec. -/
namespace Birnam.OptionalImage

def readOwners (owner : Option Nat) : List Nat := owner.toList

theorem core_reads_no_owner : readOwners none = [] := by rfl

theorem selected_owner_only (owner : Nat) : readOwners (some owner) = [owner] := by rfl

def applyFacts (world : Nat → Nat) (owner : Option Nat) (saved : Nat) : Nat → Nat :=
  match owner with
  | none => world
  | some selected => fun key => if key = selected then saved else world key

theorem core_preserves_optional_data (world : Nat → Nat) (saved : Nat) :
    applyFacts world none saved = world := by rfl

theorem selected_data_restored (world : Nat → Nat) (owner saved : Nat) :
    applyFacts world (some owner) saved owner = saved := by simp [applyFacts]

theorem other_owner_preserved (world : Nat → Nat) (owner other saved : Nat)
    (different : other ≠ owner) :
    applyFacts world (some owner) saved other = world other := by simp [applyFacts, different]

def admitted (owner : Option Nat) (available : Nat → Bool) (factSections : Nat) : Bool :=
  match owner with
  | none => factSections == 0
  | some selected => available selected && factSections == 1

theorem missing_provider_refused (owner sections : Nat) (available : Nat → Bool)
    (missing : available owner = false) : admitted (some owner) available sections = false := by
  simp [admitted, missing]

theorem core_cannot_hide_facts (available : Nat → Bool) : admitted none available 1 = false := by rfl

end Birnam.OptionalImage
