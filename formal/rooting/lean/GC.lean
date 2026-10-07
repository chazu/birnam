import Std

/-! Production optimizations: ordered cleanup batching and inventory-index marks.
    Pure release means no foreign callback occurs during the authorized batch.
    Indexed flags require an injective, immutable inventory index for the walk.
-/
namespace Birnam.GC

variable {State Token Object Index : Type}

/-- Ordinary cleanup consumes the caller's token array in reverse order. -/
def releaseLoop (release : State → Token → State) : State → List Token → State
  | st, [] => st
  | st, token :: rest => releaseLoop release (release st token) rest

theorem releaseLoop_eq_foldl (release : State → Token → State)
    (st : State) (tokens : List Token) :
    releaseLoop release st tokens = tokens.foldl release st := by
  induction tokens generalizing st with
  | nil => rfl
  | cons token rest ih => exact ih (release st token)

/-- Exact whole-state equality: next token id, providers, ordered retained
    entries and positions can all be included in State. No set-only abstraction.
    Same current default pure release is an explicit owner-guard obligation. -/
theorem bulk_cleanup_equivalent (release : State → Token → State)
    (st : State) (callerTokens : List Token) :
    releaseLoop release st callerTokens.reverse = callerTokens.reverse.foldl release st :=
  releaseLoop_eq_foldl release st callerTokens.reverse

structure Marks (Object Index : Type) where
  reference : Object → Bool
  indexed : Index → Bool
  pending : List Object

variable [DecidableEq Object] [DecidableEq Index]

def Compatible (index : Object → Index) (st : Marks Object Index) : Prop :=
  ∀ o, st.reference o = st.indexed (index o)

def emitReference (st : Marks Object Index) (o : Object) : Marks Object Index :=
  if st.reference o then st else
    {st with reference := fun x => if x = o then true else st.reference x,
             pending := o :: st.pending}

def emitIndexed (index : Object → Index) (st : Marks Object Index) (o : Object) : Marks Object Index :=
  if st.indexed (index o) then st else
    {st with indexed := fun i => if i = index o then true else st.indexed i,
             pending := o :: st.pending}

/-- The two projections evolve together without comparing their internal
    representations: reference state and indexed state have identical admission
    and identical LIFO work stacks at every step. -/
structure Pair (Object Index : Type) where
  reference : Object → Bool
  indexed : Index → Bool
  referencePending : List Object
  indexedPending : List Object

def PairInvariant (index : Object → Index) (st : Pair Object Index) : Prop :=
  (∀ o, st.reference o = st.indexed (index o)) ∧
  st.referencePending = st.indexedPending

def emitPair (index : Object → Index) (st : Pair Object Index) (o : Object) : Pair Object Index :=
  {reference := if st.reference o then st.reference else fun x => if x = o then true else st.reference x,
   indexed := if st.indexed (index o) then st.indexed else fun i => if i = index o then true else st.indexed i,
   referencePending := if st.reference o then st.referencePending else o :: st.referencePending,
   indexedPending := if st.indexed (index o) then st.indexedPending else o :: st.indexedPending}

theorem emitPair_preserves (index : Object → Index)
    (injective : ∀ a b, index a = index b → a = b)
    {st : Pair Object Index} (h : PairInvariant index st) (o : Object) :
    PairInvariant index (emitPair index st o) := by
  constructor
  · intro x
    unfold emitPair
    simp only [h.1 o]
    cases marked : st.indexed (index o) <;> simp only [Bool.false_eq_true, ↓reduceIte]
    · have equal : x = o ↔ index x = index o := ⟨congrArg index, injective x o⟩
      simp only [← equal]
      split
      · rfl
      · exact h.1 x
    · exact h.1 x
  · simp [emitPair, h.1 o, h.2]

def emitAll (index : Object → Index) (st : Pair Object Index) : List Object → Pair Object Index
  | [] => st
  | o :: rest => emitAll index (emitPair index st o) rest

theorem emitAll_preserves (index : Object → Index)
    (injective : ∀ a b, index a = index b → a = b)
    {st : Pair Object Index} (h : PairInvariant index st) (objects : List Object) :
    PairInvariant index (emitAll index st objects) := by
  induction objects generalizing st with
  | nil => exact h
  | cons o rest ih => exact ih (emitPair_preserves index injective h o)

omit [DecidableEq Object] [DecidableEq Index] in
/-- Therefore the same objects are swept in inventory creation order, and
    the same object-owner trace callbacks are scheduled in LIFO order. -/
theorem dead_inventory_equivalent (index : Object → Index)
    {st : Pair Object Index} (h : PairInvariant index st) (inventory : List Object) :
    inventory.filter (fun o => !st.reference o) =
    inventory.filter (fun o => !st.indexed (index o)) := by
  have functions : (fun o => !st.reference o) = (fun o => !st.indexed (index o)) := by
    funext o
    rw [h.1 o]
  rw [functions]

end Birnam.GC
