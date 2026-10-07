import Std
import Rooting

/-! Small reusable semantics for observations, dispatch certificates, protected
    lifetimes, refinement composition, recovery and owner-boundary cost.
    This is not an executable semantics for all of Birnam or its compiler.
-/
namespace Birnam.Semantics

/-- Payloads contain complete externally relevant data, not just event names.
    A root snapshot includes duplicates/order; a frame snapshot includes its
    entered generation, identity and current continuation. -/
inductive Observation where
  | lookup (receiver selector meaning : Nat)
  | mutation (key oldMeaning newMeaning : Nat)
  | identity (object identity : Nat)
  | allocationHook (object kind : Nat)
  | frame (identity generation pc : Nat) (fields : List Nat)
  | roots (ordered : List Nat)
  | finalizer (object : Nat)
  | userEffect (owner payload : Nat)
  | failure (kind detail : Nat)
  | result (value : Nat)
  deriving DecidableEq, Repr

structure World (Key : Type) where
  meanings : Key → Nat
  grants : Key → Bool
  epoch : Nat

structure Certificate (Key : Type) where
  dependencies : List Key
  permissions : List Key
  meanings : Key → Nat

def Valid {Key : Type} (w : World Key) (c : Certificate Key) : Prop :=
  (∀ k, k ∈ c.dependencies → w.meanings k = c.meanings k) ∧
  (∀ k, k ∈ c.permissions → w.grants k = true)

def capture {Key : Type} (w : World Key) (dependencies permissions : List Key) : Certificate Key :=
  ⟨dependencies, permissions, w.meanings⟩

theorem capture_valid {Key : Type} (w : World Key) (dependencies permissions : List Key)
    (allowed : ∀ k, k ∈ permissions → w.grants k = true) :
    Valid w (capture w dependencies permissions) := by
  exact ⟨fun _ _ => rfl, allowed⟩

/-- Global epoch equality is not the definition of validity. Current grants
    and every relevant meaning are separate obligations. -/
theorem unchanged_dependencies_preserve {Key : Type} {before after : World Key}
    {c : Certificate Key} (h : Valid before c)
    (bindings : ∀ k, k ∈ c.dependencies → after.meanings k = before.meanings k)
    (permissions : ∀ k, k ∈ c.permissions → after.grants k = before.grants k) : Valid after c := by
  exact ⟨fun k hk => (bindings k hk).trans (h.1 k hk),
         fun k hk => (permissions k hk).trans (h.2 k hk)⟩

theorem permission_revocation_invalidates {Key : Type} {w : World Key} {c : Certificate Key}
    (key : Key) (required : key ∈ c.permissions) (revoked : w.grants key = false) : ¬ Valid w c := by
  intro valid
  have grant := valid.2 key required
  rw [revoked] at grant
  contradiction

theorem replacement_invalidates {Key : Type} {w : World Key} {c : Certificate Key}
    (key : Key) (required : key ∈ c.dependencies) (changed : w.meanings key ≠ c.meanings key) : ¬ Valid w c := by
  exact fun valid => changed (valid.1 key required)

def resolve {Key : Type} (w : World Key) (key : Key) : Nat := w.meanings key

theorem certified_dispatch {Key : Type} {w : World Key} {c : Certificate Key}
    (valid : Valid w c) (key : Key) (required : key ∈ c.dependencies) :
    resolve w key = c.meanings key := valid.1 key required

/-- The entered implementation remains fixed while later sends use the new
    world. This separates active-code ownership from future dispatch. -/
structure Entered where
  generation : Nat
  codeObject : Nat
  deriving DecidableEq

theorem mutation_does_not_rewrite_entered (entered : Entered) (future : Nat) :
    (entered, future).1 = entered := rfl

/-- Existing rooting theorem transfers exact live-input coverage. The caller
    must establish this at every safepoint, not merely at region entry. -/
theorem protected_inputs_survive {Scope Object : Type}
    {roots : Birnam.Rooting.State Scope Object} (h : Birnam.Rooting.Invariant roots)
    (heap needed : Object → Prop) (edges : Object → Object → Prop)
    (alive : ∀ o, needed o → heap o)
    (coverage : ∀ o, needed o → Birnam.Rooting.Reach edges (Birnam.Rooting.roots roots.logical) o) :
    ∀ o, needed o → Birnam.Rooting.collect heap (Birnam.Rooting.roots roots.physical) edges o :=
  Birnam.Rooting.needed_survives h heap needed edges alive coverage

structure Outcome (State : Type) where
  state : State
  observations : List Observation
  sends : Nat

abbrev Action (State : Type) := State → Outcome State

def thenDo {State : Type} (first second : Action State) : Action State := fun st =>
  let a := first st
  let b := second a.state
  ⟨b.state, a.observations ++ b.observations, a.sends + b.sends⟩

/-- Explicit simulation relation must include the public state/identity and
    lifetime facts of this optimization. Trace equality preserves ordering.
    Nothing here silently labels an observable Root/Frame effect 'internal'. -/
def Refines {Reference Optimized : Type} (relation : Reference → Optimized → Prop)
    (reference : Action Reference) (optimized : Action Optimized) : Prop :=
  ∀ a b, relation a b →
    relation (reference a).state (optimized b).state ∧
    (reference a).observations = (optimized b).observations ∧
    (optimized b).sends ≤ (reference a).sends

/-- An effect visible under the agreed observation contract cannot disappear
    merely because values and send costs match. -/
theorem visible_effect_cannot_be_erased (event : Observation) :
    ¬ Refines Eq (fun u : Unit => ⟨u,[event],1⟩) (fun u : Unit => ⟨u,[],0⟩) := by
  intro refinement
  have trace := (refinement () () rfl).2.1
  simp at trace

theorem compose_refinement {Reference Optimized : Type} {rel : Reference → Optimized → Prop}
    {a b : Action Reference} {x y : Action Optimized}
    (first : Refines rel a x) (second : Refines rel b y) :
    Refines rel (thenDo a b) (thenDo x y) := by
  intro r o related
  have ha := first r o related
  have hb := second (a r).state (x o).state ha.1
  exact ⟨hb.1, by simp [thenDo, ha.2.1, hb.2.1],
    by simp only [thenDo]; exact Nat.add_le_add ha.2.2 hb.2.2⟩

def idle {State : Type} : Action State := fun st => ⟨st, [], 0⟩

def sequence {State : Type} : List (Action State) → Action State
  | [] => idle
  | a :: rest => thenDo a (sequence rest)

inductive Paired {A B : Type} (relation : A → B → Prop) : List A → List B → Prop
  | nil : Paired relation [] []
  | cons {a : A} {b : B} {as : List A} {bs : List B} :
      relation a b → Paired relation as bs → Paired relation (a::as) (b::bs)

theorem sequence_refinement {Reference Optimized : Type} {rel : Reference → Optimized → Prop}
    (reference : List (Action Reference)) (optimized : List (Action Optimized))
    (paired : Paired (Refines rel) reference optimized) :
    Refines rel (sequence reference) (sequence optimized) := by
  induction paired with
  | nil => intro a b h; exact ⟨h, rfl, Nat.le_refl 0⟩
  | cons head tail ih => exact compose_refinement head ih

/-- Reusable recovery rule. It starts the continuation at recovered current
    state and preserves the committed prefix. It never reruns that prefix.
    The faithful recovery premise must be proved for a concrete representation;
    it is not permission to invent identity or replay omitted callbacks. -/
def recoveredContinuation {Reference Optimized : Type}
    (recover : Optimized → Reference) (continuation : Action Reference)
    (prefixEvents : List Observation) (prefixSends recoverySends : Nat) (st : Optimized) : Outcome Reference :=
  let next := continuation (recover st)
  ⟨next.state, prefixEvents ++ next.observations, prefixSends + recoverySends + next.sends⟩

theorem recovery_preserves_prefix {Reference Optimized : Type}
    (recover : Optimized → Reference) (continuation : Action Reference)
    (reference : Reference) (optimized : Optimized) (faithful : recover optimized = reference)
    (events : List Observation) (before repair : Nat) :
    (recoveredContinuation recover continuation events before repair optimized).state = (continuation reference).state ∧
    (recoveredContinuation recover continuation events before repair optimized).observations = events ++ (continuation reference).observations := by
  simp [recoveredContinuation, faithful]

variable {Owner : Type} [DecidableEq Owner]

/-- Structural lower bound: count maximal consecutive runs of required owners.
    Owner-private local work is free here; each nonempty owner batch costs one
    Id send. Guards, entry dispatch, roots, adapters and GC add separate costs. -/
def ownerRuns : List Owner → Nat
  | [] => 0
  | [_] => 1
  | a :: b :: rest => if a = b then ownerRuns (b :: rest) else 1 + ownerRuns (b :: rest)

theorem cons_run_bound (owner : Owner) (rest : List Owner) :
    ownerRuns (owner :: rest) ≤ 1 + ownerRuns rest := by
  cases rest with
  | nil => simp [ownerRuns]
  | cons other tail => simp only [ownerRuns]; split <;> omega

theorem repeated_owner_collapse (owner : Owner) (n : Nat) (tail : List Owner) :
    ownerRuns (List.replicate (n+1) owner ++ tail) = ownerRuns (owner :: tail) := by
  induction n with
  | zero => simp
  | succ n ih => simpa [List.replicate_succ, List.cons_append, ownerRuns] using ih

/-- Allowed vocabulary is fixed: a batch executes positive-count consecutive
    operations at exactly one responsible owner. No universal service or
    foreign-payload access is part of this grammar. -/
inductive OwnerSchedule : List Owner → Nat → Prop
  | done : OwnerSchedule [] 0
  | batch (owner : Owner) (n : Nat) {tail : List Owner} {cost : Nat} :
      OwnerSchedule tail cost →
      OwnerSchedule (List.replicate (n+1) owner ++ tail) (cost+1)

theorem owner_boundary_lower_bound {operations : List Owner} {cost : Nat}
    (schedule : OwnerSchedule operations cost) : ownerRuns operations ≤ cost := by
  induction schedule with
  | done => simp [ownerRuns]
  | batch owner n h ih =>
    rw [repeated_owner_collapse]
    exact Nat.le_trans (cons_run_bound owner _) (by omega)

/-- Concrete interfaces can restrict batch sizes. Capacity is an explicit
    parameter, not permission to invent an arbitrary compound owner API. -/
inductive BoundedOwnerSchedule (capacity : Owner → Nat) : List Owner → Nat → Prop
  | done : BoundedOwnerSchedule capacity [] 0
  | batch (owner : Owner) (n : Nat) {tail : List Owner} {cost : Nat} :
      n+1 ≤ capacity owner → BoundedOwnerSchedule capacity tail cost →
      BoundedOwnerSchedule capacity (List.replicate (n+1) owner ++ tail) (cost+1)

omit [DecidableEq Owner] in
theorem bounded_schedule_forget {capacity : Owner → Nat} {operations : List Owner} {cost : Nat}
    (schedule : BoundedOwnerSchedule capacity operations cost) : OwnerSchedule operations cost := by
  induction schedule with
  | done => exact .done
  | batch owner n cap tail ih => exact .batch owner n ih

theorem bounded_owner_lower_bound {capacity : Owner → Nat} {operations : List Owner} {cost : Nat}
    (schedule : BoundedOwnerSchedule capacity operations cost) : ownerRuns operations ≤ cost :=
  owner_boundary_lower_bound (bounded_schedule_forget schedule)

end Birnam.Semantics
