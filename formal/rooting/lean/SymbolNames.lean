import Rooting

/-! Fixed helper names denote retained Symbol identities. This intentionally
    does not preserve repeated internal interning hooks. It caches no selected
    entry; every operation uses the current dispatch world. -/
namespace Birnam.SymbolNames
open Birnam.Rooting
variable {Name Symbol World Result : Type}

theorem retained_name_uses_current_dispatch
    (canonical : Name → Symbol) (cache : Name → Symbol)
    (agrees : ∀ n, cache n = canonical n)
    (dispatch : World → Symbol → Result) (world : World) (name : Name) :
    dispatch world (cache name) = dispatch world (canonical name) := by
  rw [agrees name]

theorem owner_edge_preserves_symbol
    (edges : Symbol → Symbol → Prop) (heap roots : Symbol → Prop)
    (owner name : Symbol) (live : Reach edges roots owner)
    (edge : edges owner name) (alive : heap name) :
    collect heap roots edges name := ⟨alive, Reach.edge live edge⟩

/-- Retaining the selector does not justify remembering its old method. -/
theorem stale_entry_changes_behavior :
    (fun version : Nat => version) 1 ≠ (fun _ : Nat => 0) 1 := by decide
/-- Capacity affects hit rate, not the canonical answer on a miss or full table.
    As above, internal interning frequency is outside this observation contract. -/
def resolveName (canonical : Name → Symbol) (cache : Name → Option Symbol) (name : Name) : Option Symbol :=
  match cache name with
  | some symbol => some symbol
  | none => some (canonical name)

theorem cache_or_fallback_is_canonical (canonical : Name → Symbol)
    (cache : Name → Option Symbol)
    (agrees : ∀ n s, cache n = some s → s = canonical n) (name : Name) :
    resolveName canonical cache name = some (canonical name) := by
  cases h : cache name with
  | none => simp [resolveName, h]
  | some symbol => simp [resolveName, h, agrees name symbol h]

theorem resized_cache_uses_current_dispatch (canonical : Name → Symbol)
    (cache : Name → Option Symbol)
    (agrees : ∀ n s, cache n = some s → s = canonical n)
    (dispatch : World → Symbol → Result) (world : World) (name : Name) :
    (resolveName canonical cache name).map (dispatch world) =
      some (dispatch world (canonical name)) := by
  rw [cache_or_fallback_is_canonical canonical cache agrees name]
  rfl

-- The private digest index narrows candidates, but names still compare in full.
-- Digest collisions may change cost; they must not change the canonical answer.
def lookupName (name : Nat) : List (Nat × Nat) → Option Nat
  | [] => none
  | (key, symbol) :: rest => if key = name then some symbol else lookupName name rest

theorem digest_bucket_preserves_lookup (digest : Nat → Nat) (name : Nat)
    (rows : List (Nat × Nat)) :
    lookupName name (rows.filter (fun row => digest row.1 == digest name)) =
      lookupName name rows := by
  induction rows with
  | nil => rfl
  | cons row rest ih =>
    rcases row with ⟨key, symbol⟩
    by_cases same : key = name
    · simp [lookupName, same]
    · by_cases bucket : digest key = digest name
      · simpa [lookupName, same, bucket] using ih
      · simpa [lookupName, same, bucket] using ih

theorem equal_digest_does_not_alias_names :
    lookupName 2 ([(1, 10), (2, 20)].filter (fun _ => true)) = some 20 := by decide

-- A cached digest holds neither an old Symbol nor a lookup result. The weak
-- inventory remains an explicit, current input on every query.
def cachedDigestLookup (digest cache : Nat → Nat) (name : Nat)
    (current : List (Nat × Nat)) : Option Nat :=
  lookupName name (current.filter (fun row => digest row.1 == cache name))

theorem cached_digest_uses_current_inventory (digest cache : Nat → Nat)
    (name : Nat) (agrees : cache name = digest name) (current : List (Nat × Nat)) :
    cachedDigestLookup digest cache name current = lookupName name current := by
  simp only [cachedDigestLookup, agrees]
  exact digest_bucket_preserves_lookup digest name current

theorem cached_digest_observes_retirement_and_reinterning :
    cachedDigestLookup id id 1 [(1, 10)] = some 10 ∧
    cachedDigestLookup id id 1 [] = none ∧
    cachedDigestLookup id id 1 [(1, 11)] = some 11 := by decide

end Birnam.SymbolNames
