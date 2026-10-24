import Std

/-! Specification of metacircular request construction, bootstrap publication,
and native activation boundaries. All identities are Oops; payload/state records
below belong to their owners. This is not an ABI or a production-code proof. -/
namespace Birnam.Boundary

structure Request (Oop : Type) where
  receiver : Oop
  selector : Oop
  arguments : List Oop
  destination : Oop
  deriving DecidableEq

-- The designated bootstrap send breaks recursion only for lookup on the root.
def atAnchor [DecidableEq Oop] (root lookup : Oop) (r : Request Oop) : Bool :=
  r.receiver == root && r.selector == lookup

def lookupRequest (vtable : Oop → Oop) (lookup fresh : Oop) (r : Request Oop) : Request Oop :=
  ⟨vtable r.receiver, lookup, [r.selector], fresh⟩

structure Pending (Oop : Type) where
  original : Request Oop
  deriving DecidableEq

-- Only the continuation owner reads Pending. An arbitrary delay, callback or
-- binding replacement does not change the stored original request.
def selected (pending : Pending Oop) (method : Oop) : Request Oop × Oop :=
  (pending.original, method)

theorem lookup_uses_vtable (vtable : Oop → Oop) (lookup fresh : Oop) (r : Request Oop) :
    (lookupRequest vtable lookup fresh r).receiver = vtable r.receiver := by rfl

theorem selected_preserves_complete_request (r : Request Oop) (m : Oop) :
    selected ⟨r⟩ m = (r, m) := by rfl

-- Primitive table search is the bootstrap owner's operation. Its current
-- binding is read, not a cached original lookup implementation.
def anchorSelect (table : Oop → Oop → Oop) (vtable : Oop → Oop) (r : Request Oop) : Oop :=
  table (vtable r.receiver) r.selector

theorem anchor_observes_replacement (old new : Oop → Oop → Oop)
    (vtable : Oop → Oop) (r : Request Oop)
    (changed : old (vtable r.receiver) r.selector ≠ new (vtable r.receiver) r.selector) :
    anchorSelect old vtable r ≠ anchorSelect new vtable r := changed

-- Well-founded bootstrap graph. A general malleable graph may have cycles;
-- no termination theorem is claimed for such a graph or arbitrary lookup code.
def metaPath : Nat → List Nat
  | 0 => [0]
  | n+1 => (n+1) :: metaPath n

theorem metaPath_length (height : Nat) : (metaPath height).length = height+1 := by
  induction height with
  | zero => rfl
  | succ n ih => simp [metaPath, ih, Nat.add_assoc]

theorem metaPath_anchor (height : Nat) : (metaPath height).getLast? = some 0 := by
  induction height with
  | zero => rfl
  | succ n ih => cases n <;> simp_all [metaPath]

structure Boot where
  graph : Bool := false
  seeded : Bool := false
  published : Bool := false
  deriving DecidableEq
inductive BootOp where
  | graph | seed | publish
  deriving DecidableEq

def bootStep (op : BootOp) (s : Boot) : Boot :=
  match op with
  | .graph => { s with graph := true }
  | .seed => if s.graph then { s with seeded := true } else s
  | .publish => if s.graph && s.seeded then { s with published := true } else s

def BootSafe (s : Boot) : Prop := s.published = true → s.graph = true ∧ s.seeded = true

theorem boot_step_safe (s : Boot) (op : BootOp) (safe : BootSafe s) :
    BootSafe (bootStep op s) := by
  cases op <;> simp_all [bootStep, BootSafe] <;> split <;> simp_all

def bootRun (ops : List BootOp) (s : Boot) : Boot := ops.foldl (fun s op => bootStep op s) s

theorem bootstrap_no_early_publication (ops : List BootOp) (s : Boot) (safe : BootSafe s) :
    BootSafe (bootRun ops s) := by
  induction ops generalizing s with
  | nil => exact safe
  | cons op ops ih => exact ih _ (boot_step_safe s op safe)

theorem cold_bootstrap_safe (ops : List BootOp) : BootSafe (bootRun ops {}) :=
  bootstrap_no_early_publication ops {} (by simp [BootSafe])

structure NativeState (Oop : Type) where
  -- Newest synchronous native activation first. These are owner identities,
  -- not borrowed native frame pointers and not Oop payload tags.
  frames : List Oop
  parked : Bool
  deriving DecidableEq
inductive NativeOp (Oop : Type) where
  | enter (owner : Oop)
  | leave (owner : Oop)
  | suspend
  | resume
  deriving DecidableEq

def nativeStep [DecidableEq Oop] (op : NativeOp Oop) (s : NativeState Oop) : NativeState Oop :=
  match op with
  | .enter owner => if s.parked then s else { s with frames := owner :: s.frames }
  | .leave owner => match s.frames with
    | [] => s
    | top :: rest => if owner = top then { s with frames := rest } else s
  | .suspend => if s.frames.isEmpty then { s with parked := true } else s
  | .resume => { s with parked := false }

def NativeSafe (s : NativeState Oop) : Prop := s.parked = true → s.frames = []

theorem native_step_safe [DecidableEq Oop] (s : NativeState Oop) (op : NativeOp Oop)
    (safe : NativeSafe s) : NativeSafe (nativeStep op s) := by
  cases op with
  | enter owner => simp only [nativeStep]; split <;> simp_all [NativeSafe]
  | leave owner =>
    cases hs : s.frames with
    | nil => simpa [nativeStep, hs] using safe
    | cons top rest =>
      simp only [nativeStep, hs]
      split <;> simp_all [NativeSafe]
  | suspend =>
    simp only [nativeStep]
    split <;> simp_all [NativeSafe, List.isEmpty_iff]
  | resume => simp [nativeStep, NativeSafe]

def nativeRun [DecidableEq Oop] (ops : List (NativeOp Oop)) (s : NativeState Oop) :=
  ops.foldl (fun s op => nativeStep op s) s

theorem native_history_safe [DecidableEq Oop] (ops : List (NativeOp Oop))
    (s : NativeState Oop) (safe : NativeSafe s) : NativeSafe (nativeRun ops s) := by
  induction ops generalizing s with
  | nil => exact safe
  | cons op rest ih => exact ih _ (native_step_safe s op safe)

theorem inner_return_keeps_ancestors [DecidableEq Oop] (inner : Oop) (ancestors : List Oop) :
    (nativeStep (.leave inner) ⟨inner :: ancestors, false⟩).frames = ancestors := by
  simp [nativeStep]

theorem foreign_return_rejected [DecidableEq Oop] (foreign top : Oop) (rest : List Oop)
    (different : foreign ≠ top) :
    nativeStep (.leave foreign) ⟨top :: rest, false⟩ = ⟨top :: rest, false⟩ := by
  simp [nativeStep, different]

theorem enclosing_native_blocks_suspend [DecidableEq Oop] (inner outer : Oop) (rest : List Oop) :
    (nativeStep .suspend (nativeStep (.leave inner) ⟨inner :: outer :: rest, false⟩)).parked = false := by
  simp [nativeStep]

-- An asynchronous completion is owned language state after the native frame
-- returns. Completion is scoped to its execution owner and consumed once.
structure Ticket (Oop : Type) where
  owner : Oop
  destination : Oop
  used : Bool
  deriving DecidableEq

def complete [DecidableEq Oop] (caller : Oop) (t : Ticket Oop) : Option (Ticket Oop × Oop) :=
  if caller = t.owner ∧ t.used = false then some ({ t with used := true }, t.destination) else none

theorem wrong_owner_cannot_complete [DecidableEq Oop] (caller : Oop) (t : Ticket Oop)
    (different : caller ≠ t.owner) : complete caller t = none := by
  simp [complete, different]

theorem completion_preserves_destination [DecidableEq Oop] (caller : Oop)
    (before after : Ticket Oop) (answer : Oop) (success : complete caller before = some (after, answer)) :
    answer = before.destination ∧ after.used = true := by
  unfold complete at success
  split at success
  · cases success; exact ⟨rfl, rfl⟩
  · contradiction

theorem completion_cannot_repeat [DecidableEq Oop] (caller other : Oop)
    (before after : Ticket Oop) (answer : Oop) (success : complete caller before = some (after, answer)) :
    complete other after = none := by
  have used := (completion_preserves_destination caller before after answer success).2
  simp [complete, used]

-- Concrete witnesses for semantic mutation checks.
theorem bootstrap_example : (bootRun [.publish] {}).published = false := by decide

theorem anchor_example : atAnchor (0 : Nat) 7 ⟨1, 7, [], 9⟩ = false := by decide

theorem native_ancestor_example :
    (nativeRun [.enter 1, .enter 2, .leave 2, .suspend] (⟨[], false⟩ : NativeState Nat)).parked = false := by decide

theorem native_foreign_example :
    (nativeStep (.leave 2) (⟨[1], false⟩ : NativeState Nat)).frames = [1] := by decide

theorem completion_owner_example : complete (2 : Nat) ⟨1, 9, false⟩ = none := by decide

theorem completion_used_example : complete (1 : Nat) ⟨1, 9, true⟩ = none := by decide

end Birnam.Boundary
