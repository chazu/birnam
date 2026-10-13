import Std

/-! Value-level simulation for moving private scratch fields out of a fact store.
    Public keys and private Cell identities are disjoint. Reads of private facts
    through the public database API are intentionally outside this abstraction.
    This proves neither compiler correctness nor GC/rooting, allocation history,
    Store basis equality, replaced constructor effects, or image behavior. -/
namespace Birnam.PrivateScratch

abbrev Store := Sum Nat Nat → Nat
structure State where
  world : Nat → Nat
  cells : Nat → Nat

def put {Key : Type} [DecidableEq Key] (fields : Key → Nat) (key : Key) (value : Nat) :=
  fun queried => if queried = key then value else fields queried

def separate (store : Store) : State :=
  ⟨fun key => store (.inl key), fun key => store (.inr key)⟩

def writePrivate (s : State) (key value : Nat) : State :=
  ⟨s.world, put s.cells key value⟩

def publish (s : State) (key value : Nat) : State :=
  ⟨put s.world key value, s.cells⟩

theorem private_write_related (store : Store) (key value : Nat) :
    separate (put store (.inr key) value) = writePrivate (separate store) key value := by
  simp [separate, put, writePrivate]
  rfl

theorem publication_related (store : Store) (key value : Nat) :
    separate (put store (.inl key) value) = publish (separate store) key value := by
  simp [separate, put, publish]
  rfl

theorem private_write_preserves_world (s : State) (key value : Nat) :
    (writePrivate s key value).world = s.world := by rfl

/-- Aliases are reads of the same Cell identity. -/
theorem alias_sees_update (s : State) (key value : Nat) :
    (writePrivate s key value).cells key = value := by simp [writePrivate, put]

theorem other_cell_preserved (s : State) (key other value : Nat) (different : other ≠ key) :
    (writePrivate s key value).cells other = s.cells other := by
  simp [writePrivate, put, different]

inductive Action where
  | read (cell : Nat)
  | write (cell value : Nat)
  | emit (key value : Nat)

def oldRun : List Action → Store → List Nat × Store
  | [], store => ([], store)
  | .read key :: rest, store =>
    let (answers, after) := oldRun rest store
    (store (.inr key) :: answers, after)
  | .write key value :: rest, store => oldRun rest (put store (.inr key) value)
  | .emit key value :: rest, store => oldRun rest (put store (.inl key) value)

def newRun : List Action → State → List Nat × State
  | [], s => ([], s)
  | .read key :: rest, s =>
    let (answers, after) := newRun rest s
    (s.cells key :: answers, after)
  | .write key value :: rest, s => newRun rest (writePrivate s key value)
  | .emit key value :: rest, s => newRun rest (publish s key value)

/-- Every finite prefix preserves scratch answers and published field values.
    NativeDoc's intended catalog writes remain writes, rather than being erased
    along with its private scanner. No equality of internal message traces is claimed. -/
theorem simulation (actions : List Action) (store : Store) :
    ((oldRun actions store).1, separate (oldRun actions store).2) =
      newRun actions (separate store) := by
  induction actions generalizing store with
  | nil => rfl
  | cons action rest ih =>
    cases action with
    | read key =>
      simpa [oldRun, newRun, separate] using
        congrArg (fun result : List Nat × State => (store (.inr key) :: result.1, result.2)) (ih store)
    | write key value =>
      simpa [oldRun, newRun, private_write_related] using ih (put store (.inr key) value)
    | emit key value =>
      simpa [oldRun, newRun, publication_related] using ih (put store (.inl key) value)

end Birnam.PrivateScratch
