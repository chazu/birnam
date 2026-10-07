import Std

/-!
Duplicate rooting only. No axiom assumes GC safety or compiler correctness.
The owner lease and compiler liveness obligations are explicit premises.
Counts abstract token identities and order ONLY while the owner authorizes it.
-/
namespace Birnam.Rooting

variable {Scope Object : Type}

abbrev Counts (Scope Object : Type) := Scope → Object → Nat

def compact (n : Nat) : Nat := if n = 0 then 0 else 1

structure State (Scope Object : Type) where
  logical : Counts Scope Object
  physical : Counts Scope Object
  optimized : Scope → Bool
  witness : Scope → Nat
  permitted : Bool
  epoch : Nat

/-- Supports partial materialization: a coalesced positive count may grow back
    toward its logical multiplicity without ever losing its covering root. -/
def Invariant (st : State Scope Object) : Prop :=
  (∀ s o, if st.optimized s then
    compact (st.logical s o) ≤ st.physical s o ∧ st.physical s o ≤ st.logical s o
    else st.physical s o = st.logical s o) ∧
  (∀ s, st.optimized s = true → st.permitted = true ∧ st.witness s = st.epoch)

def roots (counts : Counts Scope Object) (o : Object) : Prop := ∃ s, 0 < counts s o

/-- Reachability is inductive and therefore unbounded in heap/path size. -/
inductive Reach (edges : Object → Object → Prop) (root : Object → Prop) : Object → Prop
  | root {o} : root o → Reach edges root o
  | edge {a b} : Reach edges root a → edges a b → Reach edges root b

theorem reach_congr {edges : Object → Object → Prop} {a b : Object → Prop}
    (h : ∀ o, a o ↔ b o) : ∀ o, Reach edges a o ↔ Reach edges b o := by
  intro o
  constructor
  · intro path
    induction path with
    | root r => exact Reach.root ((h _).mp r)
    | edge _ e ih => exact Reach.edge ih e
  · intro path
    induction path with
    | root r => exact Reach.root ((h _).mpr r)
    | edge _ e ih => exact Reach.edge ih e

theorem positive_iff_of_bounds {n p : Nat}
    (h : compact n ≤ p ∧ p ≤ n) : 0 < p ↔ 0 < n := by
  unfold compact at h
  split at h <;> omega

theorem root_equivalence {st : State Scope Object} (h : Invariant st) :
    ∀ o, roots st.physical o ↔ roots st.logical o := by
  intro o
  have each : ∀ s, 0 < st.physical s o ↔ 0 < st.logical s o := by
    intro s
    have hs := h.1 s o
    cases hm : st.optimized s <;> simp [hm] at hs
    · rw [hs]
    · exact positive_iff_of_bounds hs
  constructor
  · rintro ⟨s, hp⟩
    exact ⟨s, (each s).mp hp⟩
  · rintro ⟨s, hn⟩
    exact ⟨s, (each s).mpr hn⟩

theorem reachability_equivalence {st : State Scope Object} (h : Invariant st)
    (edges : Object → Object → Prop) :
    ∀ o, Reach edges (roots st.physical) o ↔ Reach edges (roots st.logical) o :=
  reach_congr (root_equivalence h)

def collect (heap root : Object → Prop) (edges : Object → Object → Prop)
    (o : Object) : Prop := heap o ∧ Reach edges root o

/-- Equal explicit-root reachability yields identical precise-GC survivors,
    not merely survival of this benchmark's result. -/
theorem collector_equivalence {st : State Scope Object} (h : Invariant st)
    (heap : Object → Prop) (edges : Object → Object → Prop) :
    ∀ o, collect heap (roots st.physical) edges o ↔
         collect heap (roots st.logical) edges o := by
  intro o
  exact and_congr Iff.rfl (reachability_equivalence h edges o)

/-- Compiler supplies coverage of all future-used values at each safepoint.
    The theorem transfers that coverage to the optimized collector. -/
theorem needed_survives {st : State Scope Object} (h : Invariant st)
    (heap needed : Object → Prop) (edges : Object → Object → Prop)
    (alreadyAlive : ∀ o, needed o → heap o)
    (compilerCoverage : ∀ o, needed o → Reach edges (roots st.logical) o) :
    ∀ o, needed o → collect heap (roots st.physical) edges o := by
  intro o hn
  exact ⟨alreadyAlive o hn,
    (reachability_equivalence h edges o).mpr (compilerCoverage o hn)⟩

variable [DecidableEq Scope] [DecidableEq Object]

def update (counts : Counts Scope Object) (s : Scope) (o : Object) (n : Nat) :
    Counts Scope Object := fun s' o' => if s' = s ∧ o' = o then n else counts s' o'

/-- Coalescing is local to the scope: an ancestor's root is never borrowed. -/
def keepPhysical (optimized : Bool) (p : Nat) : Nat :=
  if optimized then (if p = 0 then 1 else p) else p + 1

def keepRaw (st : State Scope Object) (s : Scope) (o : Object) : State Scope Object :=
  {st with
    logical := update st.logical s o (st.logical s o + 1)
    physical := update st.physical s o (keepPhysical (st.optimized s) (st.physical s o))}

theorem keep_bounds {n p : Nat} (h : compact n ≤ p ∧ p ≤ n) :
    compact (n+1) ≤ (if p = 0 then 1 else p) ∧
    (if p = 0 then 1 else p) ≤ n+1 := by
  simp only [compact, Nat.add_eq_zero_iff, Nat.one_ne_zero, and_false, ↓reduceIte]
  split <;> omega

theorem keepRaw_preserves {st : State Scope Object} (h : Invariant st)
    (s : Scope) (o : Object) : Invariant (keepRaw st s o) := by
  constructor
  · intro s' o'
    simp only [keepRaw, update]
    by_cases same : s' = s ∧ o' = o
    · rcases same with ⟨sameScope, sameObject⟩
      subst s'
      subst o'
      simp only [and_self, ↓reduceIte]
      have hs := h.1 s o
      cases hm : st.optimized s <;> simp [hm, keepPhysical] at hs ⊢
      · rw [hs]
      · exact keep_bounds hs
    · simp only [same, ↓reduceIte]
      exact h.1 s' o'
  · exact h.2

/-- Materialization preserves logical registrations; no user retain callback
    is replayed. This operation belongs to the authorized root-scope owner. -/
def deopt (st : State Scope Object) : State Scope Object :=
  {st with physical := st.logical, optimized := fun _ => false}

omit [DecidableEq Scope] [DecidableEq Object] in
theorem deopt_preserves (st : State Scope Object) : Invariant (deopt st) := by
  constructor
  · intro s o
    simp [deopt]
  · intro s h
    simp [deopt] at h

omit [DecidableEq Scope] [DecidableEq Object] in
theorem deopt_restores_exact (st : State Scope Object) :
    (deopt st).physical = (deopt st).logical := rfl

omit [DecidableEq Scope] [DecidableEq Object] in
omit [DecidableEq Scope] [DecidableEq Object] in
theorem deopt_preserves_reachability {st : State Scope Object} (h : Invariant st)
    (edges : Object → Object → Prop) :
    ∀ o, Reach edges (roots st.physical) o ↔ Reach edges (roots (deopt st).physical) o :=
  reachability_equivalence h edges

/-- Fresh owner permission and matching epoch are checked before reuse. If
    either fails, restore the canonical representation before continuing. -/
def guardedKeep (st : State Scope Object) (s : Scope) (o : Object) : State Scope Object :=
  if st.permitted = true ∧ st.witness s = st.epoch then keepRaw st s o
  else keepRaw (deopt st) s o

theorem guardedKeep_preserves {st : State Scope Object} (h : Invariant st)
    (s : Scope) (o : Object) : Invariant (guardedKeep st s o) := by
  unfold guardedKeep
  split
  · exact keepRaw_preserves h s o
  · exact keepRaw_preserves (deopt_preserves st) s o

theorem guard_failure_restores_exact (st : State Scope Object) (s : Scope) (o : Object)
    (refused : ¬(st.permitted = true ∧ st.witness s = st.epoch)) :
    (guardedKeep st s o).physical = (guardedKeep st s o).logical := by
  simp [guardedKeep, refused, keepRaw, deopt, keepPhysical]

/-- Revocation is a barrier, not an epoch write followed by later repair. -/
def changePermission (st : State Scope Object) (permit : Bool) : State Scope Object :=
  {deopt st with permitted := permit, epoch := st.epoch + 1}

omit [DecidableEq Scope] [DecidableEq Object] in
theorem changePermission_preserves (st : State Scope Object) (permit : Bool) :
    Invariant (changePermission st permit) := by
  constructor
  · intro s o
    simp [changePermission, deopt]
  · intro s h
    simp [changePermission, deopt] at h

omit [DecidableEq Scope] [DecidableEq Object] in
theorem revocation_restores_exact (st : State Scope Object) :
    (changePermission st false).physical = (changePermission st false).logical := rfl

omit [DecidableEq Scope] [DecidableEq Object] in
/-- Without permission, even multiplicity is preserved. This does not assert
    token-id or callback-order equivalence for a different concrete protocol. -/
theorem unpermitted_exact {st : State Scope Object} (h : Invariant st)
    (refused : st.permitted = false) : st.physical = st.logical := by
  funext s o
  have hm : st.optimized s = false := by
    cases m : st.optimized s
    · rfl
    · have hp := (h.2 s m).1
      rw [refused] at hp
      contradiction
  have hs := h.1 s o
  simpa [hm] using hs

/-- Permission can be acquired only for an empty scope. This avoids silently
    collapsing already-observable canonical registrations. -/
noncomputable def acquire (st : State Scope Object) (scope : Scope) : State Scope Object := by
  classical
  exact if st.permitted = true ∧ (∀ o, st.logical scope o = 0) then
    {st with optimized := fun s => if s = scope then true else st.optimized s,
             witness := fun s => if s = scope then st.epoch else st.witness s}
    else st

omit [DecidableEq Object] in
theorem acquire_preserves {st : State Scope Object} (h : Invariant st)
    (scope : Scope) : Invariant (acquire st scope) := by
  classical
  unfold acquire
  split
  · rename_i admitted
    constructor
    · intro s o
      by_cases same : s = scope
      · subst s
        have old := h.1 scope o
        have zero := admitted.2 o
        cases hm : st.optimized scope <;> simp [hm, zero, compact] at old
        all_goals simp [zero, old, compact]
      · simpa [same] using h.1 s o
    · intro s hm
      by_cases same : s = scope
      · subst s
        simpa using admitted.1
      · simpa [same] using h.2 s (by simpa [same] using hm)
  · exact h

/-- Scope exit also represents exception unwinding. Escaped/returned values
    must be handed off to another scope before this operation. -/
def clearScope (st : State Scope Object) (scope : Scope) : State Scope Object :=
  {st with logical := fun s o => if s = scope then 0 else st.logical s o,
           physical := fun s o => if s = scope then 0 else st.physical s o,
           optimized := fun s => if s = scope then false else st.optimized s}

omit [DecidableEq Object] in
theorem clearScope_preserves {st : State Scope Object} (h : Invariant st)
    (scope : Scope) : Invariant (clearScope st scope) := by
  constructor
  · intro s o
    by_cases same : s = scope
    · simp [clearScope, same]
    · simpa [clearScope, same] using h.1 s o
  · intro s hm
    by_cases same : s = scope
    · simp [clearScope, same] at hm
    · exact h.2 s (by simpa [clearScope, same] using hm)

/-- One incremental owner materialization step, guarded against overshoot.
    GC may occur between these steps: the coverage theorem still applies. -/
def materializeOne (st : State Scope Object) (s : Scope) (o : Object) : State Scope Object :=
  if st.physical s o < st.logical s o then
    {st with physical := update st.physical s o (st.physical s o + 1)}
  else st

theorem materializeOne_preserves {st : State Scope Object} (h : Invariant st)
    (s : Scope) (o : Object) : Invariant (materializeOne st s o) := by
  unfold materializeOne
  split
  · rename_i more
    constructor
    · intro s' o'
      by_cases same : s' = s ∧ o' = o
      · rcases same with ⟨sameScope, sameObject⟩
        subst s'
        subst o'
        have old := h.1 s o
        cases hm : st.optimized s <;> simp [hm] at old
        · omega
        · simp [update]
          omega
      · simpa [update, same] using h.1 s' o'
    · exact h.2
  · exact h

theorem materialization_progress {n p : Nat} (more : p < n) :
    n - (p+1) < n-p := by omega

inductive Event (Scope Object : Type) where
  | keep (s : Scope) (o : Object)
  | acquire (s : Scope)
  | exitScope (s : Scope)
  | materialize (s : Scope) (o : Object)
  | permission (allowed : Bool)
  | deopt

noncomputable def step (st : State Scope Object) : Event Scope Object → State Scope Object
  | .keep s o => guardedKeep st s o
  | .acquire s => acquire st s
  | .exitScope s => clearScope st s
  | .materialize s o => materializeOne st s o
  | .permission permit => changePermission st permit
  | .deopt => deopt st

theorem step_preserves {st : State Scope Object} (h : Invariant st)
    (event : Event Scope Object) : Invariant (step st event) := by
  cases event with
  | keep s o => exact guardedKeep_preserves h s o
  | acquire s => exact acquire_preserves h s
  | exitScope s => exact clearScope_preserves h s
  | materialize s o => exact materializeOne_preserves h s o
  | permission permit => exact changePermission_preserves st permit
  | deopt => exact deopt_preserves st

noncomputable def run (st : State Scope Object) : List (Event Scope Object) → State Scope Object
  | [] => st
  | event :: rest => run (step st event) rest

/-- No TLC-style bound on object count, scope count, multiplicity, or trace
    length. Applies at every prefix, including reentry and materialization. -/
theorem run_preserves {st : State Scope Object} (h : Invariant st)
    (events : List (Event Scope Object)) : Invariant (run st events) := by
  induction events generalizing st with
  | nil => exact h
  | cons event rest ih => exact ih (step_preserves h event)

theorem compiler_gc_equivalence {st : State Scope Object} (h : Invariant st)
    (events : List (Event Scope Object))
    (heap : Object → Prop) (edges : Object → Object → Prop) :
    ∀ o, collect heap (roots (run st events).physical) edges o ↔
         collect heap (roots (run st events).logical) edges o :=
  collector_equivalence (run_preserves h events) heap edges

/-- Essential for lifetime means deletion destroys needed reachability. This
    is separate from the question of observable message effects. -/
def Essential (edges : Object → Object → Prop) (before after needed : Object → Prop) : Prop :=
  ∃ o, needed o ∧ Reach edges before o ∧ ¬ Reach edges after o

omit [DecidableEq Scope] [DecidableEq Object] in
theorem nonessential_iff_coverage (edges : Object → Object → Prop)
    (before after needed : Object → Prop)
    (coverage : ∀ o, needed o → Reach edges before o) :
    ¬ Essential edges before after needed ↔
    (∀ o, needed o → Reach edges after o) := by
  classical
  constructor
  · intro nonessential o hn
    apply Classical.byContradiction
    intro missing
    exact nonessential ⟨o, hn, coverage o hn, missing⟩
  · intro safe ⟨o, hn, _, missing⟩
    exact missing (safe o hn)

end Birnam.Rooting
