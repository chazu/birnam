import Std
namespace Birnam.Registration
variable {State Object Token : Type}
/-- State contains the complete retained list, positions, next id and providers.
    Acquisition returns each newly assigned id in input order. -/
def loop (retain : State → Object → State × Token) : State → List Object → State × List Token
  | st, [] => (st, [])
  | st, o :: rest =>
      let (st',token) := retain st o
      let (final,tokens) := loop retain st' rest
      (final,token :: tokens)

def batch (retain : State → Object → State × Token) (st : State) (objects : List Object) : State × List Token :=
  objects.foldl (fun (acc : State × List Token) o =>
      let (next,token) := retain acc.1 o
      (next,acc.2 ++ [token])) (st,[])

theorem loop_accumulator (retain : State → Object → State × Token)
    (st : State) (objects : List Object) (priorTokens : List Token) :
    objects.foldl (fun (acc : State × List Token) o =>
      let (next,token) := retain acc.1 o
      (next,acc.2 ++ [token])) (st,priorTokens) =
      ((loop retain st objects).1, priorTokens ++ (loop retain st objects).2) := by
  induction objects generalizing st priorTokens with
  | nil => simp [loop]
  | cons o rest ih =>
    simp only [List.foldl_cons, loop]
    rw [ih]
    simp [List.append_assoc]

theorem exact_batch_registration (retain : State → Object → State × Token)
    (st : State) (objects : List Object) : batch retain st objects = loop retain st objects := by
  simpa [batch] using loop_accumulator retain st objects []

/-- Capacity must be checked AFTER any potentially reentrant proof dispatch. -/
theorem successful_prefix_fits (next count bound offset : Nat)
    (capacity : next + count ≤ bound) (inside : offset < count) : next + offset + 1 ≤ bound := by omega
end Birnam.Registration
