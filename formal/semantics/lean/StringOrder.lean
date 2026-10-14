import Std

/-! Authored byte-order traversal. `step` reads the left then right byte and
    performs current equality/order sends, carrying arbitrary callback state.
    None means equal bytes; some false is a decisive result, not continuation.
    `finish` compares current sizes only after exhausting the common prefix.
    These proofs abstract canonical control/index operations and finite bounds.
    They do not equate internal Cell messages, allocation, or arbitrary replaced
    control methods, and do not prove compiler/GC implementation correctness. -/
namespace Birnam.StringOrder

def cellLoop {σ : Type} (step : σ → Nat → σ × Option Bool)
    (finish : σ → σ × Bool) : List Nat → σ → Option Bool → σ × Bool × List Nat
  | [], state, none => let (final, answer) := finish state; (final, answer, [])
  | _, state, some answer => (state, answer, [])
  | index :: rest, state, none =>
      let (next, verdict) := step state index
      let (final, answer, reads) := cellLoop step finish rest next verdict
      (final, answer, index :: reads)

def direct {σ : Type} (step : σ → Nat → σ × Option Bool)
    (finish : σ → σ × Bool) : List Nat → σ → σ × Bool × List Nat
  | [], state => let (final, answer) := finish state; (final, answer, [])
  | index :: rest, state =>
      let (next, verdict) := step state index
      match verdict with
      | some answer => (next, answer, [index])
      | none =>
          let (final, answer, reads) := direct step finish rest next
          (final, answer, index :: reads)

theorem same_result_state_reads {σ : Type} (step : σ → Nat → σ × Option Bool)
    (finish : σ → σ × Bool) (indices : List Nat) (state : σ) :
    cellLoop step finish indices state none = direct step finish indices state := by
  induction indices generalizing state with
  | nil => rfl
  | cons index rest ih =>
    simp only [cellLoop, direct]
    cases h : step state index with
    | mk next verdict =>
      cases verdict with
      | none => simp [ih]
      | some answer => cases rest <;> simp [cellLoop]

theorem mismatch_stops_even_when_false {σ : Type} (step : σ → Nat → σ × Option Bool)
    (finish : σ → σ × Bool) (index : Nat) (rest : List Nat) (state next : σ)
    (hit : step state index = (next, some false)) :
    direct step finish (index :: rest) state = (next, false, [index]) := by
  simp [direct, hit]

theorem equal_prefix_continues_in_current_state {σ : Type} (step : σ → Nat → σ × Option Bool)
    (finish : σ → σ × Bool) (index : Nat) (rest : List Nat) (state next : σ)
    (equal : step state index = (next, none)) :
    direct step finish (index :: rest) state =
      let (final, answer, reads) := direct step finish rest next
      (final, answer, index :: reads) := by
  simp [direct, equal]

end Birnam.StringOrder
