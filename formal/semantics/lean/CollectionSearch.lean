import Std

/-! Authored search, under the canonical traversal/conditional/nil contracts.
    `step` includes predicate effects and the conditional's result: none means
    false or a matching nil; some preserves any non-nil identity, including Error.
    State can change future predicate behavior. This proves return/state/callback
    equivalence for a fixed element sequence, NOT equivalence of internal sends,
    allocation hooks, arbitrary accessor overrides or mutable sequence contents.
    Those sends remain live in Birnam and have separate executable tests. -/
namespace Birnam.CollectionSearch

def foldSearch {α σ : Type} (step : σ → α → σ × Option α) :
    List α → σ → Option α → σ × Option α × List α
  | [], state, found => (state, found, [])
  | _ :: rest, state, some found => foldSearch step rest state (some found)
  | each :: rest, state, none =>
      let (next, found) := step state each
      let (final, answer, calls) := foldSearch step rest next found
      (final, answer, each :: calls)

def search {α σ : Type} (step : σ → α → σ × Option α) :
    List α → σ → σ × Option α × List α
  | [], state => (state, none, [])
  | each :: rest, state =>
      let (next, found) := step state each
      match found with
      | some answer => (next, some answer, [each])
      | none =>
          let (final, answer, calls) := search step rest next
          (final, answer, each :: calls)

theorem found_suffix {α σ : Type} (step : σ → α → σ × Option α)
    (xs : List α) (state : σ) (found : α) :
    foldSearch step xs state (some found) = (state, some found, []) := by
  induction xs with
  | nil => rfl
  | cons each rest ih => exact ih

theorem same_result_state_callbacks {α σ : Type} (step : σ → α → σ × Option α)
    (xs : List α) (state : σ) :
    foldSearch step xs state none = search step xs state := by
  induction xs generalizing state with
  | nil => rfl
  | cons each rest ih =>
    simp only [foldSearch, search]
    cases h : step state each with
    | mk next found =>
      cases found with
      | none => simp [ih]
      | some answer => simp [found_suffix]

theorem hit_stops {α σ : Type} (step : σ → α → σ × Option α)
    (each : α) (rest : List α) (state next : σ) (answer : α)
    (hit : step state each = (next, some answer)) :
    search step (each :: rest) state = (next, some answer, [each]) := by
  simp [search, hit]

theorem miss_continues {α σ : Type} (step : σ → α → σ × Option α)
    (each : α) (rest : List α) (state next : σ)
    (miss : step state each = (next, none)) :
    search step (each :: rest) state =
      let (final, answer, calls) := search step rest next
      (final, answer, each :: calls) := by
  simp [search, miss]

-- Model nil by 0; even when the predicate accepts it, search must continue.
theorem matching_nil_continues :
    search (fun (n : Nat) (x : Nat) => (n + 1, if x = 0 then none else some x))
      [0, 7, 9] 0 = (2, some 7, [0, 7]) := by decide

theorem predicate_state_is_live :
    search (fun (n : Nat) (x : Nat) => (n + 1, if n = 0 then none else some x))
      [1, 2, 3] 0 = (2, some 2, [1, 2]) := by decide


-- The false branch can continue directly: no intermediate nil/notNil round trip.
-- A conditional-send Error is a non-nil result, not a reason to run the suffix.
inductive Decision (α : Type) where
  | accept
  | reject
  | error (value : α)

def conditional {α : Type} (decision : Decision α) (each : Option α) : Option α :=
  match decision with
  | .accept => each
  | .reject => none
  | .error value => some value

def directBranch {α : Type} (decision : Decision α) (each rest : Option α) : Option α :=
  match decision with
  | .accept => each.orElse (fun _ => rest)
  | .reject => rest
  | .error value => some value

theorem direct_branch_equivalent {α : Type} (decision : Decision α)
    (each rest : Option α) :
    (conditional decision each).orElse (fun _ => rest) = directBranch decision each rest := by
  cases decision <;> rfl

end Birnam.CollectionSearch
