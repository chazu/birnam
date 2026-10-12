import Std

/-! Canonical signed-integer Range values. This models the public results,
    including overflow and selected-result identity, not equivalence of internal
    send traces. Replaced operations execute as ordinary current sends. -/
namespace Birnam.RangeHandlers

def compare (a b : Int) : Int := if a < b then -1 else if a = b then 0 else 1

def checked (n : Int) : Option Int :=
  if -9223372036854775808 ≤ n ∧ n ≤ 9223372036854775807 then some n else none

def count (difference : Option Int) : Option Int :=
  match difference with
  | none => none
  | some d => if d = 9223372036854775807 then none else checked (d + 1)

def priorSize (a b : Int) : Option Int :=
  if compare a b > 0 then some 0 else count (checked (b - a))
def reusedSize (a b : Int) : Option Int :=
  let handler := if compare a b > 0 then fun (_ : Int) (_ : Int) => some 0
    else fun start finish => count (checked (finish - start))
  handler a b

theorem same_size (a b : Int) : reusedSize a b = priorSize a b := by
  simp only [reusedSize, priorSize]
  split <;> rfl

-- Pair a numeric payload with an arbitrary allocated identity. The handler
-- must return that same object, rather than a newly boxed equal integer.
def priorEnd {α : Type} (value : Int × α) (last : Int) : Option (Int × α) :=
  if compare value.1 last > 0 then none else some value

def reusedEnd {α : Type} (value : Int × α) (last : Int) : Option (Int × α) :=
  let handler := if compare value.1 last > 0 then fun (_ : Int × α) => none else some
  handler value

theorem same_element_and_identity {α : Type} (value : Int × α) (last : Int) :
    reusedEnd value last = priorEnd value last := by
  simp only [reusedEnd, priorEnd]
  split <;> rfl

theorem overflow_stays_nil :
    reusedSize (-9223372036854775808) 9223372036854775807 = none ∧
    reusedSize 0 9223372036854775807 = none := by decide

end Birnam.RangeHandlers
