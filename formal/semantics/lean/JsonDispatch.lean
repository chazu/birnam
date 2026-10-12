import Std

/-! Canonical JSON node kinds are disjoint. This models returned values and
    type-test counts, not equality of internal sends under method replacement.
    Handler/equality selection remains ordinary authored late-bound code. -/
namespace Birnam.JsonDispatch

inductive Kind where
  | string | number | object | array | literal | unknown
  deriving DecidableEq

def prior {α : Type} (kind : Kind) (handle : Kind → α) (absent : α) : α :=
  let a := if kind = .string then handle .string else absent
  let b := if kind = .number then handle .number else a
  let c := if kind = .object then handle .object else b
  let d := if kind = .array then handle .array else c
  if kind = .literal then handle .literal else d

def selected {α : Type} (kind : Kind) (handle : Kind → α) (absent : α) : α :=
  if kind = .string then handle .string else
  if kind = .number then handle .number else
  if kind = .object then handle .object else
  if kind = .array then handle .array else
  if kind = .literal then handle .literal else absent

theorem same_value {α : Type} (kind : Kind) (handle : Kind → α) (absent : α) :
    selected kind handle absent = prior kind handle absent := by
  cases kind <;> simp [selected, prior]

def checks : Kind → Nat
  | .string => 1 | .number => 2 | .object => 3
  | .array => 4 | .literal => 5 | .unknown => 5

theorem no_more_checks (kind : Kind) : checks kind ≤ 5 := by
  cases kind <;> decide

theorem fixture_saves_checks :
    250 * 5 - (50 * checks .string + 150 * checks .number + 50 * checks .object) = 750 := by
  decide

end Birnam.JsonDispatch
