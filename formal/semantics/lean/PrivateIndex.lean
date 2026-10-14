import Std

/-! An index is a bucket selector, never the authority for identity or order.
    Hash is arbitrary (even constant). Full key equality resolves collisions.
    This abstracts the standard HashMap contract; it is not a verification of
    Rust's hash table, the integer mixer, allocation or unsafe payload access. -/
namespace Birnam.PrivateIndex

abbrev Rows := List (Nat × Nat)

def lookup (key : Nat) : Rows → Option Nat
  | [] => none
  | (name, value) :: rest => if name = key then some value else lookup key rest

def bucket (hash : Nat → Nat) (key : Nat) (rows : Rows) : Rows :=
  rows.filter fun row => decide (hash row.1 = hash key)

theorem collision_safe (hash : Nat → Nat) (key : Nat) (rows : Rows) :
    lookup key (bucket hash key rows) = lookup key rows := by
  induction rows with
  | nil => rfl
  | cons row rest ih =>
    rcases row with ⟨name, value⟩
    by_cases same : name = key
    · subst name; simp [lookup, bucket]
    · by_cases sameBucket : hash name = hash key
      · simpa [bucket, lookup, same, sameBucket] using ih
      · simpa [bucket, lookup, same, sameBucket] using ih

theorem changing_hash_preserves_lookup (before after : Nat → Nat) (key : Nat) (rows : Rows) :
    lookup key (bucket before key rows) = lookup key (bucket after key rows) := by
  rw [collision_safe, collision_safe]

theorem colliding_keys_remain_distinct :
    lookup 1 (bucket (fun _ => 0) 1 [(0, 7), (1, 9)]) = some 9 := by decide

theorem unknown_key_cannot_release_another_token :
    lookup 2 (bucket (fun _ => 0) 2 [(0, 7), (1, 9)]) = none := by decide

/-- Enumeration is taken from the authoritative vector, not hash-table order. -/
structure IndexedRoots where
  roots : Rows
  hash : Nat → Nat

def reindex (s : IndexedRoots) (hash : Nat → Nat) : IndexedRoots := ⟨s.roots, hash⟩

theorem reindex_preserves_order_and_duplicates (s : IndexedRoots) (hash : Nat → Nat) :
    (reindex s hash).roots = s.roots := by rfl

end Birnam.PrivateIndex
