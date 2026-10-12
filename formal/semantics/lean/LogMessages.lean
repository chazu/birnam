import Std

/-! Intended bounded-log policy, not equivalence with the old unbounded bug.
    Canonical size/slice contracts only; actual Birnam sends remain replaceable.
    Elements are arbitrary values, so slicing preserves the original entries. -/
namespace Birnam.LogMessages

def trim {α : Type} (xs : List α) : List α :=
  if xs.length > 6 then xs.drop (xs.length - 6) else xs

theorem bounded {α : Type} (xs : List α) : (trim xs).length ≤ 6 := by
  unfold trim
  split
  · simp only [List.length_drop]
    omega
  · omega

theorem short_unchanged {α : Type} (xs : List α) (small : xs.length ≤ 6) :
    trim xs = xs := by simp [trim, show ¬ xs.length > 6 by omega]

theorem exact_suffix {α : Type} (xs : List α) :
    trim xs = xs.drop (xs.length - 6) := by
  unfold trim
  split
  · rfl
  · have zero : xs.length - 6 = 0 := by omega
    simp [zero]

theorem append_bounded {α : Type} (xs : List α) (entry : α) :
    (trim (xs ++ [entry])).length ≤ 6 := bounded _

theorem burst_keeps_newest : trim [1,2,3,4,5,6,7,8,9,10,11,12] = [7,8,9,10,11,12] := by
  decide

end Birnam.LogMessages
