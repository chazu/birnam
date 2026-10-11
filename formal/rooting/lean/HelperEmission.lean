import Rooting

/-! Removing private helper bodies requires the complete reference closure,
    including address-taken callbacks. State can include the current world,
    ordered send trace, failures and roots; helper bodies are unchanged.
    This is not a proof of the C compiler's reference discovery or generated C. -/
namespace Birnam.HelperEmission
open Birnam.Rooting
variable {Function State : Type}

theorem reachable_retained (roots : Function → Prop) (edges : Function → Function → Prop)
    (keep : Function → Bool)
    (entry : ∀ f, roots f → keep f = true)
    (dependency : ∀ f g, keep f = true → edges f g → keep g = true)
    {f : Function} (reachable : Reach edges roots f) : keep f = true := by
  induction reachable with
  | root h => exact entry _ h
  | edge _ edge ih => exact dependency _ _ ih edge

/-- Calls are observed in their original order; no helper body is substituted. -/
def execute (body : Function → State → State) (keep : Function → Bool) :
    List Function → State → Option State
  | [], state => some state
  | f :: rest, state => if keep f then execute body keep rest (body f state) else none

theorem retained_calls_preserve_state (body : Function → State → State)
    (keep : Function → Bool) (calls : List Function) (state : State)
    (retained : ∀ f ∈ calls, keep f = true) :
    execute body keep calls state = some (calls.foldl (fun state f => body f state) state) := by
  induction calls generalizing state with
  | nil => rfl
  | cons f rest ih =>
    simp only [execute, retained f (by simp), ↓reduceIte, List.foldl_cons]
    exact ih _ (fun g h => retained g (by simp [h]))

end Birnam.HelperEmission
