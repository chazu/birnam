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

def put {Key Value : Type} [DecidableEq Key] (fields : Key → Value) (key : Key) (value : Value) :=
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

/-! Default local Cells and deliberately requested fact handles. This is the new
    API contract, not equivalence with the old public asCell fact/history behavior.
    Id freshness, GC, image encoding and history retention are outside this model. -/
inductive CellHandle where
  | local (identity : Nat)
  | fact (key : Nat)

def defaultCell (identity : Nat) : CellHandle := .local identity

def readCell (s : State) : CellHandle → Nat
  | .local key => s.cells key
  | .fact key => s.world key

def writeCell (s : State) (handle : CellHandle) (value : Nat) : State :=
  match handle with
  | .local key => writePrivate s key value
  | .fact key => publish s key value

theorem default_cell_preserves_world (s : State) (key value : Nat) :
    (writeCell s (defaultCell key) value).world = s.world := by rfl

theorem cell_alias_reads_write (s : State) (handle : CellHandle) (value : Nat) :
    readCell (writeCell s handle value) handle = value := by
  cases handle <;> simp [readCell, writeCell, writePrivate, publish, put]

theorem explicit_fact_cell_publishes (s : State) (key value : Nat) :
    (writeCell s (.fact key) value).world key = value := by
  simp [writeCell, publish, put]

theorem local_and_fact_handles_are_distinct (s : State) (key value : Nat) :
    readCell (writeCell s (defaultCell key) value) (.fact key) = s.world key := by rfl

theorem distinct_default_cells_are_independent (s : State) (key other value : Nat)
    (different : other ≠ key) :
    readCell (writeCell s (defaultCell key) value) (defaultCell other) = s.cells other := by
  simp [readCell, writeCell, defaultCell, writePrivate, put, different]

/-- The local Cell representation is one ordinary reference slot. Values stand
    for opaque identities, so cloning the container does not clone its contents. -/
def slotStorage (s : State) : Nat → Nat → Nat :=
  fun identity slot => if slot = 0 then s.cells identity else 0

theorem ordinary_slot_reads_local_value (s : State) (identity : Nat) :
    slotStorage s identity 0 = s.cells identity := by simp [slotStorage]

theorem ordinary_slot_write_matches_local (s : State) (key value : Nat) :
    slotStorage (writePrivate s key value) =
      put (slotStorage s) key (put (slotStorage s key) 0 value) := by
  funext identity slot
  by_cases same : identity = key <;> by_cases zero : slot = 0 <;>
    simp_all [slotStorage, writePrivate, put]

theorem clone_container_preserves_value (s : State) (source target : Nat) :
    (put s.cells target (s.cells source)) target = s.cells source := by simp [put]

/-- Static inspection may use a published inventory or a newly parsed value.
    Only the result escapes; parsing does not authorize compiled admission.
    Parser callbacks and metadata validation are premises, not modeled effects. -/
def inspectSource (published : Option Nat) (parsed : Nat) : Nat × Option Nat :=
  match published with
  | some value => (value, published)
  | none => (parsed, none)

theorem source_inspection_preserves_publication (published : Option Nat) (parsed : Nat) :
    (inspectSource published parsed).2 = published := by
  cases published <;> rfl

theorem source_inspection_does_not_enable_admission (parsed : Nat) :
    (inspectSource none parsed).2 = none := by rfl

theorem source_inspection_reuses_published (value parsed : Nat) :
    (inspectSource (some value) parsed).1 = value := by rfl

-- A retained compilation is an immutable value with the original source ID.
structure Compilation where
  sourceIdentity : Nat
  metadataIdentity : Nat
  deriving DecidableEq

def compilation (source metadata : Nat) : Compilation := ⟨source, metadata⟩
theorem compilation_keeps_source_identity (source metadata : Nat) :
    (compilation source metadata).sourceIdentity = source := by rfl

structure CompilationOwner where
  selected : Nat
  revision : Nat

def replaceCompilation (s : CompilationOwner) (record : Nat) : CompilationOwner :=
  if record = s.selected then s else ⟨record, s.revision + 1⟩

theorem compilation_noop_keeps_stamp (s : CompilationOwner) :
    replaceCompilation s s.selected = s := by simp [replaceCompilation]

theorem compilation_replacement_stales_plan (s : CompilationOwner) (record : Nat)
    (changed : record ≠ s.selected) :
    (replaceCompilation s record).revision ≠ s.revision := by
  simp [replaceCompilation, changed]

theorem compilation_aba_changes_stamp (s : CompilationOwner) (record : Nat)
    (changed : record ≠ s.selected) :
    (replaceCompilation (replaceCompilation s record) s.selected).revision = s.revision + 2 := by
  have reverse : s.selected ≠ record := Ne.symm changed
  simp [replaceCompilation, changed, reverse, Nat.add_assoc]

theorem retained_compilation_is_snapshot (records : Nat → Compilation)
    (s : CompilationOwner) (replacement : Nat) :
    let captured := records s.selected
    let _ := replaceCompilation s replacement
    captured = records s.selected := by rfl

/-! Lifecycle retirement changes reachability, not explicit facts or retained
    records. This models owned fields and selected observation, not native GC,
    arbitrary callbacks, or the whole transaction implementation. -/
structure Lifecycle where
  records : Nat → Nat
  facts : Nat → Nat
  current : Nat

def selectLifecycle (s : Lifecycle) (next : Nat) : Lifecycle := { s with current := next }
def lifecycleFact (s : Lifecycle) (key value : Nat) : Lifecycle :=
  { s with facts := put s.facts key value }
def lifecycleRead (s : Lifecycle) (record : Nat) := (s.records record, s)

theorem lifecycle_selection_preserves_annotations (s : Lifecycle) (next : Nat) :
    (selectLifecycle s next).facts = s.facts := by rfl

theorem lifecycle_selection_preserves_retained_record (s : Lifecycle) (next old : Nat) :
    (selectLifecycle s next).records old = s.records old := by rfl

theorem lifecycle_status_read_has_no_effect (s : Lifecycle) (record : Nat) :
    (lifecycleRead s record).2 = s := by rfl

theorem unrelated_fact_keeps_prepared_observation (s : Lifecycle) (key value record : Nat) :
    (lifecycleFact s key value).records record = s.records record := by rfl

end Birnam.PrivateScratch
