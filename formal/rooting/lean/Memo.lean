import Std

/-! Exact canonical-byte memoization of a fixed pure parser. The key is the
    complete current byte sequence, not an address, epoch, or hash digest.
    Owner sends and allocations happen before this pure interval and remain
    unchanged. No callback may invalidate an ABI borrow during this interval.
-/
namespace Birnam.Memo
variable {Key Value Error : Type} [DecidableEq Key]
abbrev Cache (Key Value : Type) := List (Key × Value)

def lookup (cache : Cache Key Value) (key : Key) : Option Value :=
  match cache with
  | [] => none
  | (k,v) :: rest => if key = k then some v else lookup rest key

def Sound (parse : Key → Except Error Value) (cache : Cache Key Value) : Prop :=
  ∀ k v, (k,v) ∈ cache → parse k = .ok v

def result (parse : Key → Except Error Value) (cache : Cache Key Value) (key : Key) : Except Error Value :=
  match lookup cache key with
  | some v => .ok v
  | none => parse key

theorem lookup_sound (parse : Key → Except Error Value) {cache : Cache Key Value}
    (h : Sound parse cache) (key : Key) (value : Value)
    (found : lookup cache key = some value) : parse key = .ok value := by
  induction cache with
  | nil => simp [lookup] at found
  | cons pair rest ih =>
    rcases pair with ⟨k,v⟩
    simp only [lookup] at found
    split at found
    · rename_i same
      have values : v = value := Option.some.inj found
      subst k
      subst v
      exact h key value (by simp)
    · apply ih (fun k v member => h k v (by simp [member])) found

theorem memoization_equivalent (parse : Key → Except Error Value) {cache : Cache Key Value}
    (h : Sound parse cache) (key : Key) : result parse cache key = parse key := by
  unfold result
  cases found : lookup cache key with
  | none => rfl
  | some value => exact (lookup_sound parse h key value found).symm

omit [DecidableEq Key] in
theorem insert_preserves (parse : Key → Except Error Value) {cache : Cache Key Value}
    (h : Sound parse cache) (key : Key) (value : Value) (parsed : parse key = .ok value) :
    Sound parse ((key,value)::cache) := by
  intro k v member
  simp only [List.mem_cons] at member
  rcases member with same | old
  · cases same
    exact parsed
  · exact h k v old

omit [DecidableEq Key] in
theorem eviction_preserves (parse : Key → Except Error Value) {cache remaining : Cache Key Value}
    (h : Sound parse cache) (subset : ∀ p, p ∈ remaining → p ∈ cache) : Sound parse remaining := by
  intro k v member
  exact h k v (subset (k,v) member)

end Birnam.Memo
