import Std

/-! Id caches only an ordered dictionary position. Validation reads the current
    key, and a successful lookup reads the current value. No native entry,
    parent or absence result is cached. Dictionary keys are unique; this is an
    invariant of Id's mutators, not a constraint on language value equality. -/
namespace Birnam.SlotHints
variable {K V : Type} [DecidableEq K]

def lookup (key : K) : List (K × V) → Option V
  | [] => none
  | (name,value) :: rest => if name = key then some value else lookup key rest

def UniqueFor (key : K) (rows : List (K × V)) : Prop :=
  ∀ a b, (key,a) ∈ rows → (key,b) ∈ rows → a = b

theorem lookup_of_member (key : K) (rows : List (K × V)) (value : V)
    (unique : UniqueFor key rows) (member : (key,value) ∈ rows) :
    lookup key rows = some value := by
  induction rows with
  | nil => simp at member
  | cons row rest ih =>
    rcases row with ⟨name,head⟩
    by_cases same : name = key
    · subst name
      have values : head = value := unique head value (by simp) member
      simp [lookup, values]
    · have tail : (key,value) ∈ rest := by simpa [same, Ne.symm same] using member
      have tailUnique : UniqueFor key rest := by
        intro a b ha hb
        exact unique a b (by simp [ha]) (by simp [hb])
      simpa [lookup, same] using ih tailUnique tail

/-- The runtime's bounds/key check produces this evidence from the live record.
    There is no assumption that the dictionary equals an earlier snapshot. -/
def validated (key : K) (rows : List (K × V)) (hint : Option (K × V)) : Option V :=
  match hint with
  | some (name,value) => if name = key then some value else lookup key rows
  | none => lookup key rows

theorem validated_hint_equals_current_lookup (key : K) (rows : List (K × V))
    (unique : UniqueFor key rows) (hint : Option (K × V))
    (current : ∀ row, hint = some row → row ∈ rows) :
    validated key rows hint = lookup key rows := by
  cases hint with
  | none => rfl
  | some row =>
    rcases row with ⟨name,value⟩
    by_cases same : name = key
    · subst name
      simp only [validated, ↓reduceIte]
      exact (lookup_of_member key rows value unique (current _ rfl)).symm
    · simp [validated, same]

theorem replacement_value_is_read :
    validated (7 : Nat) [(7,99)] (some (7,99)) = some 99 := by rfl

theorem shifted_key_falls_back :
    validated (7 : Nat) [(7,99),(8,11)] (some (8,11)) = some 99 := by rfl

end Birnam.SlotHints
