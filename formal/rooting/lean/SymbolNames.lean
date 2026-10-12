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

end Birnam.SymbolNames
