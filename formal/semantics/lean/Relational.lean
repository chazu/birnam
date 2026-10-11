import Std

/-! Positive matching with an arbitrary stateful, failing comparison operation.
    No reflexivity, symmetry, transitivity, purity or permanent binding premise.
    Object values are identities in this model, never reconstructed scalars.
    Runtime parsing, Boolean dispatch and memory safety need separate tests. -/
namespace Birnam.Relational

variable {S O E : Type}
abbrev Bindings (O : Type) := List (Nat × O)
abbrev Compare (S O E : Type) := S → O → O → S × Except E Bool
inductive Term (O : Type) where
  | constant : O → Term O
  | variable : Nat → Term O

def lookup (name : Nat) : Bindings O → Option O
  | [] => none
  | (key, value) :: rest => if key = name then some value else lookup name rest

def compared (equal : Compare S O E) (s : S) (bindings : Bindings O)
    (expected actual : O) : S × Except E (Option (Bindings O)) :=
  let (next, result) := equal s expected actual
  (next, result.map fun yes => if yes then some bindings else none)

def matchTerm (equal : Compare S O E) (s : S) (bindings : Bindings O)
    (term : Term O) (actual : O) : S × Except E (Option (Bindings O)) :=
  match term with
  | .constant expected => compared equal s bindings expected actual
  | .variable name => match lookup name bindings with
    | none => (s, .ok (some (bindings ++ [(name, actual)])))
    | some expected => compared equal s bindings expected actual

/-- Reference row evaluation stops at the first mismatch or failure. -/
def reference (equal : Compare S O E) (s : S) (bindings : Bindings O) :
    List (Term O × O) → S × Except E (Option (Bindings O))
  | [] => (s, .ok (some bindings))
  | (term, actual) :: rest =>
    let (next, result) := matchTerm equal s bindings term actual
    match result with
    | .ok (some extended) => reference equal next extended rest
    | stopped => (next, stopped)

/-- Authored fold threads the world even when a comparison changes methods. -/
def step (equal : Compare S O E) (state : S × Except E (Option (Bindings O)))
    (pair : Term O × O) : S × Except E (Option (Bindings O)) :=
  match state.2 with
  | .ok (some bindings) => matchTerm equal state.1 bindings pair.1 pair.2
  | _ => state

theorem stopped_error (equal : Compare S O E) (s : S) (e : E)
    (pairs : List (Term O × O)) : pairs.foldl (step equal) (s, .error e) = (s, .error e) := by
  induction pairs with
  | nil => rfl
  | cons pair rest ih => simpa [List.foldl, step] using ih

theorem stopped_mismatch (equal : Compare S O E) (s : S)
    (pairs : List (Term O × O)) : pairs.foldl (step equal) (s, .ok none) = (s, .ok none) := by
  induction pairs with
  | nil => rfl
  | cons pair rest ih => simpa [List.foldl, step] using ih

theorem row_fold_refines_reference (equal : Compare S O E) (pairs : List (Term O × O))
    (s : S) (bindings : Bindings O) :
    pairs.foldl (step equal) (s, .ok (some bindings)) = reference equal s bindings pairs := by
  induction pairs generalizing s bindings with
  | nil => rfl
  | cons pair rest ih =>
    rcases pair with ⟨term, actual⟩
    simp only [List.foldl, step, reference]
    cases h : matchTerm equal s bindings term actual with
    | mk next answer =>
      cases answer with
      | error e => exact stopped_error equal next e rest
      | ok possible =>
        cases possible with
        | none => exact stopped_mismatch equal next rest
        | some extended => exact ih next extended

theorem new_variable_preserves_identity (equal : Compare S O E) (s : S)
    (bindings : Bindings O) (name : Nat) (actual : O) (unbound : lookup name bindings = none) :
    matchTerm equal s bindings (.variable name) actual =
      (s, .ok (some (bindings ++ [(name, actual)]))) := by simp [matchTerm, unbound]

/-- Projection asks earlier representatives, in order, about the incoming value. -/
def seen (equal : Compare S O E) (s : S) (value : O) : List O → S × Except E Bool
  | [] => (s, .ok false)
  | prior :: rest =>
    let (next, answer) := equal s prior value
    match answer with
    | .ok false => seen equal next value rest
    | result => (next, result)

theorem seen_first_true_stops (equal : Compare S O E) (s next : S) (value prior : O)
    (rest : List O) (yes : equal s prior value = (next, .ok true)) :
    seen equal s value (prior :: rest) = (next, .ok true) := by simp [seen, yes]

def appendDistinct (equal : Compare S O E) (s : S) (prior : List O) (value : O) :
    S × Except E (List O) :=
  let (next, answer) := seen equal s value prior
  (next, answer.map fun duplicate => if duplicate then prior else prior ++ [value])

theorem projection_keeps_originals (equal : Compare S O E) (s next : S)
    (prior : List O) (value : O) (fresh : seen equal s value prior = (next, .ok false)) :
    appendDistinct equal s prior value = (next, .ok (prior ++ [value])) := by
  simp [appendDistinct, fresh, Except.map]

def changingComparison (s : Nat) (_left _right : Nat) : Nat × Except Nat Bool :=
  (s + 1, .ok (s == 0))

theorem replacement_between_comparisons_is_observed :
    reference changingComparison 0 [] [(.constant 7, 7), (.constant 7, 7)] =
      (2, .ok none) := by rfl

theorem nonreflexive_equality_is_not_repaired :
    appendDistinct (fun s _ _ => (s, Except.ok false) : Compare Nat Nat Nat) 0 [7] 7 =
      (0, .ok [7, 7]) := by rfl

end Birnam.Relational
