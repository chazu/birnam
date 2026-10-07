import Rooting

/-! Value equivalence for a deliberate Boolean source/API simplification. This
    does not require preserving allocations or intercepted calls to the removed
    comparison/boxing/selection sequence. Id still dispatches the new operation. -/
namespace Birnam.Selection
open Birnam.Rooting
variable {Object : Type} [DecidableEq Object]

def comparison (left right : Object) : Nat := if left = right then 1 else 0

def selectWord (bits : Nat) (zero nonzero : Object) : Object :=
  if bits = 0 then zero else nonzero

def selectIdentity (left right equal different : Object) : Object :=
  if left = right then equal else different

theorem removes_comparison_box (left right equal different : Object) :
    selectWord (comparison left right) different equal =
      selectIdentity left right equal different := by
  by_cases h : left = right <;> simp [comparison, selectWord, selectIdentity, h]

theorem result_is_supplied (left right equal different : Object) :
    selectIdentity left right equal different = equal ∨
      selectIdentity left right equal different = different := by
  by_cases h : left = right <;> simp [selectIdentity, h]

theorem live_arguments_cover_result (base : Object → Prop)
    (left right equal different : Object) (yes : base equal) (no : base different) :
    base (selectIdentity left right equal different) := by
  by_cases h : left = right <;> simp [selectIdentity, h, yes, no]

theorem selection_survives_collection (heap base : Object → Prop)
    (edges : Object → Object → Prop) (left right equal different : Object)
    (yesLive : base equal) (noLive : base different)
    (yesHeap : heap equal) (noHeap : heap different) :
    collect heap base edges (selectIdentity left right equal different) := by
  constructor
  · by_cases h : left = right <;> simp [selectIdentity, h, yesHeap, noHeap]
  · exact Reach.root (live_arguments_cover_result base left right equal different yesLive noLive)
end Birnam.Selection
