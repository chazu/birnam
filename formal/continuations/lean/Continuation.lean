import Std

/-! Proposed semantics only: no production runtime is imported or verified.
Oop is an arbitrary type. Frame belongs to Execution, never to Id. The generic
Id driver below can only invoke the owner interface and handle public events.
It cannot inspect an owner's state, or distinguish a continuation Oop by tag. -/
namespace Birnam.Continuation

structure Frame (Oop : Type) where
  method : Oop
  destination : Oop
  deriving DecidableEq

inductive Op (Oop : Type) where
  | call (method destination : Oop)
  | tail (method : Oop)
  | answer (value : Oop)
  deriving DecidableEq

inductive Event (Oop : Type) where
  | entered (method destination : Oop)
  | answered (destination value : Oop)
  | invalid
  deriving DecidableEq

-- Reference control semantics. The nearest unfinished call is first.
def execute (op : Op Oop) (frames : List (Frame Oop)) :
    List (Frame Oop) × Event Oop :=
  match op, frames with
  | .call m k, fs => (⟨m, k⟩ :: fs, .entered m k)
  | .tail m, f :: fs => (⟨m, f.destination⟩ :: fs, .entered m f.destination)
  | .answer v, f :: fs => (fs, .answered f.destination v)
  | _, [] => ([], .invalid)

-- The public contract contains Oop arguments and events, not frame pointers.
-- This is an abstraction of owner entry, not a proposed separate service table.
structure Owner (Oop State : Type) where
  step : State → Op Oop → State × Event Oop

def drive (owner : Owner Oop State) : List (Op Oop) → State → State × List (Event Oop)
  | [], s => (s, [])
  | op :: rest, s =>
    let (s', event) := owner.step s op
    let (s'', events) := drive owner rest s'
    (s'', event :: events)

-- A representation implementation must discharge this law. It is a premise,
-- not a claim that arbitrary native owners are correct.
def Implements (owner : Owner Oop State) (view : State → List (Frame Oop)) : Prop :=
  ∀ s op, (view (owner.step s op).1, (owner.step s op).2) = execute op (view s)

def listOwner : Owner Oop (List (Frame Oop)) := ⟨fun s op => execute op s⟩

theorem listOwner_implements : Implements (listOwner : Owner Oop _) id := by
  intro s op
  simp [listOwner]

-- Independently written representation: oldest frame first, nearest at end.
-- Id uses exactly the same drive definition with either representation.
def endStep (frames : List (Frame Oop)) (op : Op Oop) :
    List (Frame Oop) × Event Oop :=
  match op with
  | .call m k => (frames ++ [⟨m, k⟩], .entered m k)
  | .tail m => match frames.getLast? with
    | none => ([], .invalid)
    | some f => (frames.dropLast ++ [⟨m, f.destination⟩], .entered m f.destination)
  | .answer v => match frames.getLast? with
    | none => ([], .invalid)
    | some f => (frames.dropLast, .answered f.destination v)

def endOwner : Owner Oop (List (Frame Oop)) := ⟨endStep⟩

theorem endOwner_implements :
    Implements (endOwner : Owner Oop _) List.reverse := by
  intro s op
  obtain ⟨xs, rfl⟩ : ∃ xs : List (Frame Oop), s = xs.reverse :=
    ⟨s.reverse, (List.reverse_reverse s).symm⟩
  cases xs with
  | nil => cases op <;> simp [endOwner, endStep, execute]
  | cons f fs =>
    cases op <;> simp [endOwner, endStep, execute, List.reverse_append]

-- Unbounded finite-trace refinement, including invalid empty-stack operations.
theorem drive_refines (owner : Owner Oop State) (view : State → List (Frame Oop))
    (implementation : Implements owner view) (ops : List (Op Oop)) (s : State) :
    (view (drive owner ops s).1, (drive owner ops s).2) =
      drive listOwner ops (view s) := by
  induction ops generalizing s with
  | nil => rfl
  | cons op ops ih =>
    have h := implementation s op
    have hs := congrArg Prod.fst h
    have he := congrArg Prod.snd h
    have hi := ih (owner.step s op).1
    simp only [drive]
    change (view (drive owner ops (owner.step s op).1).1,
      (owner.step s op).2 :: (drive owner ops (owner.step s op).1).2) =
      ((drive listOwner ops (execute op (view s)).1).1,
       (execute op (view s)).2 :: (drive listOwner ops (execute op (view s)).1).2)
    rw [← hs, ← he, ← hi]

theorem drive_trace_length (owner : Owner Oop State) (ops : List (Op Oop)) (s : State) :
    (drive owner ops s).2.length = ops.length := by
  induction ops generalizing s with
  | nil => rfl
  | cons op ops ih => simp [drive, ih]

-- A concrete representation replacement theorem, not just a conditional claim.
theorem representation_independence (ops : List (Op Oop)) (s : List (Frame Oop)) :
    ((drive endOwner ops s.reverse).1.reverse, (drive endOwner ops s.reverse).2) =
      drive listOwner ops s := by
  simpa using drive_refines endOwner List.reverse endOwner_implements ops s.reverse

-- Any number of tail transfers preserves both caller depth and destination.
def tailChain (methods : List Oop) (frames : List (Frame Oop)) :=
  (drive listOwner (methods.map Op.tail) frames).1

theorem tailChain_shape (methods : List Oop) (f : Frame Oop) (rest : List (Frame Oop)) :
    ∃ final, tailChain methods (f :: rest) = final :: rest ∧
      final.destination = f.destination := by
  induction methods generalizing f with
  | nil => exact ⟨f, rfl, rfl⟩
  | cons m ms ih =>
    obtain ⟨last, hs, hk⟩ := ih (⟨m, f.destination⟩ : Frame Oop)
    exact ⟨last, hs, hk⟩

theorem tailChain_bounded (methods : List Oop) (f : Frame Oop) (rest : List (Frame Oop)) :
    (tailChain methods (f :: rest)).length = rest.length + 1 := by
  obtain ⟨last, hs, _⟩ := tailChain_shape methods f rest
  simp [hs]

theorem tailChain_answer (methods : List Oop) (f : Frame Oop)
    (rest : List (Frame Oop)) (value : Oop) :
    execute (.answer value) (tailChain methods (f :: rest)) =
      (rest, .answered f.destination value) := by
  obtain ⟨last, hs, hk⟩ := tailChain_shape methods f rest
  simp [hs, execute, hk]

-- One-shot wakeup is an Execution policy, not an intrinsic kind of Oop.
-- Tokens need equality only; a stale delivery never regains ownership.
def consume [DecidableEq Oop] (token : Oop) (used : List Oop) : Option (List Oop) :=
  if token ∈ used then none else some (token :: used)

theorem consumed_cannot_resume_again [DecidableEq Oop] (token : Oop) (before after : List Oop)
    (success : consume token before = some after) : consume token after = none := by
  unfold consume at success
  split at success
  · contradiction
  · cases success
    simp [consume]

theorem consumption_monotone [DecidableEq Oop] (token : Oop) (before after : List Oop)
    (success : consume token before = some after) : ∀ k ∈ before, k ∈ after := by
  unfold consume at success
  split at success
  · contradiction
  · cases success
    intro k h
    exact List.mem_cons_of_mem token h

def remember [DecidableEq Oop] (token : Oop) (used : List Oop) : List Oop :=
  (consume token used).getD used

def consumeMany [DecidableEq Oop] (tokens : List Oop) (used : List Oop) : List Oop :=
  tokens.foldl (fun acc token => remember token acc) used

theorem remember_keeps [DecidableEq Oop] (token k : Oop) (used : List Oop)
    (member : k ∈ used) : k ∈ remember token used := by
  simp only [remember, consume]
  split <;> simp_all

theorem consumeMany_keeps [DecidableEq Oop] (tokens : List Oop) (used : List Oop)
    (k : Oop) (member : k ∈ used) : k ∈ consumeMany tokens used := by
  induction tokens generalizing used with
  | nil => exact member
  | cons token rest ih => exact ih _ (remember_keeps token k used member)

theorem stale_wakeup_rejected_after_any_history [DecidableEq Oop]
    (token : Oop) (before after history : List Oop)
    (success : consume token before = some after) :
    consume token (consumeMany history after) = none := by
  have member : token ∈ after := by
    unfold consume at success
    split at success
    · contradiction
    · cases success; simp
  have retained := consumeMany_keeps history after token member
  simp [consume, retained]

-- The transfer protocol is explicit. An Oop used as an answer stays an answer,
-- even if the same Oop is also used elsewhere as a continuation destination.
theorem continuation_can_be_returned_as_data (f : Frame Oop) (rest : List (Frame Oop)) :
    execute (.answer f.destination) (f :: rest) =
      (rest, .answered f.destination f.destination) := by rfl

-- Non-tail calls keep an actual return frame; tail replacement does not.
theorem call_answer_restores (frames : List (Frame Oop)) (method destination value : Oop) :
    execute (.answer value) (execute (.call method destination) frames).1 =
      (frames, .answered destination value) := by cases frames <;> rfl

-- Computable witnesses ensure rejected mutations change behavior, rather than
-- merely requiring a different proof script for an equivalent definition.
theorem tail_example :
    (tailChain [2, 3] [⟨1, 9⟩]).length = 1 := by decide

theorem representation_example :
    ((endStep [⟨1, 9⟩] (.tail 2)).1.reverse,
      (endStep [⟨1, 9⟩] (.tail 2)).2) = execute (.tail 2) [⟨1, 9⟩] := by decide

theorem wake_example : consume (9 : Nat) [9] = none := by decide

theorem trace_example :
    (drive listOwner [Op.call (1 : Nat) 9] []).2.length = 1 := by decide

end Birnam.Continuation
