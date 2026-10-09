import Semantics

/-! Fixed, callback-free Id query admission. Services must have canonical
    meanings; a changed service executes the original query path. A native
    entry is the pair (method, context), not a method address alone.
    This is a contract proof, not verification of the Rust or Id dictionary.
-/
namespace Birnam.Admission
open Birnam.Semantics

structure Facts where
  defaultLookup : Bool
  method : Nat
  context : Nat
  expectedMethod : Nat
  expectedContext : Nat
  servicesCanonical : Bool
  deriving DecidableEq

def separate (f : Facts) : Bool :=
  f.defaultLookup && (f.method == f.expectedMethod) && (f.context == f.expectedContext)

def packed (f : Facts) : Bool :=
  f.servicesCanonical && f.defaultLookup &&
    (f.method == f.expectedMethod) && (f.context == f.expectedContext)

/-- A declined optional query cannot replace the original guard execution. -/
def selected (f : Facts) : Bool := if packed f then true else separate f

theorem packed_sound (f : Facts) (h : packed f = true) : separate f = true := by
  simp_all [packed, separate, Bool.and_eq_true]

theorem selected_equivalent (f : Facts) : selected f = separate f := by
  unfold selected
  split
  · rename_i h
    exact (packed_sound f h).symm
  · rfl

theorem changed_service_uses_fallback (f : Facts) (h : f.servicesCanonical = false) :
    packed f = false := by simp [packed, h]

theorem changed_context_declines (f : Facts) (h : f.context ≠ f.expectedContext) :
    packed f = false := by simp [packed, h]

theorem custom_lookup_declines (f : Facts) (h : f.defaultLookup = false) :
    packed f = false := by simp [packed, h]

/-- Counts here exclude selector resolution, internals of Id and subsequent
    permissions, signature access, boxing, roots and the actual target send. -/
def ordinaryGuard (f : Facts) : Outcome Facts :=
  ⟨f, [.result (if separate f then 1 else 0)], 2⟩
def optimizedGuard (f : Facts) : Outcome Facts :=
  ⟨f, [.result (if selected f then 1 else 0)], if packed f then 1 else 3⟩

theorem guard_behavior_equivalent (f : Facts) :
    (ordinaryGuard f).state = (optimizedGuard f).state ∧
    (ordinaryGuard f).observations = (optimizedGuard f).observations := by
  simp [ordinaryGuard, optimizedGuard, selected_equivalent]

theorem successful_guard_saves_one (f : Facts) (h : packed f = true) :
    (optimizedGuard f).sends + 1 = (ordinaryGuard f).sends := by
  simp [ordinaryGuard, optimizedGuard, h]

theorem declined_guard_cost_is_explicit (f : Facts) (h : packed f = false) :
    (optimizedGuard f).sends = (ordinaryGuard f).sends + 1 := by
  simp [ordinaryGuard, optimizedGuard, h]

/-- Generic costed refinement applies only to the successful, pure guard
    domain. It would be false to claim non-increasing sends on fallback. -/
theorem admitted_guard_refines :
    Refines (fun a b : Facts => a = b ∧ packed a = true) ordinaryGuard optimizedGuard := by
  intro a b related
  rcases related with ⟨rfl, allowed⟩
  exact ⟨⟨rfl, allowed⟩, (guard_behavior_equivalent a).2,
    by simp [ordinaryGuard, optimizedGuard, allowed]⟩

/-- The native query certifies exact shape with a nonzero identity; zero denotes
    mismatch, and unknown identity or noncanonical services decline. -/
def metadataResult (current shapeMatches : Bool) (identity : Nat) : Option Nat :=
  if current then
    if shapeMatches then if identity = 0 then none else some identity else some 0
  else none

def selectedShape (current shapeMatches : Bool) (identity : Nat) (legacy : Bool) : Bool :=
  match metadataResult current shapeMatches identity with
  | some result => result != 0
  | none => legacy

theorem metadata_query_requires_current (current shapeMatches : Bool) (identity result : Nat)
    (success : metadataResult current shapeMatches identity = some result) : current = true := by
  cases current <;> simp_all [metadataResult]

theorem nonzero_metadata_identity_requires_shape (current shapeMatches : Bool) (identity result : Nat)
    (success : metadataResult current shapeMatches identity = some result) (nonzero : result ≠ 0) :
    shapeMatches = true := by
  cases current <;> cases shapeMatches <;> simp_all [metadataResult]

theorem unknown_metadata_identity_declines : metadataResult true true 0 = none := by rfl

theorem changed_metadata_preserves_legacy (shapeMatches : Bool) (identity : Nat) (legacy : Bool) :
    selectedShape false shapeMatches identity legacy = legacy := by rfl

theorem selected_shape_preserves_result (current shapeMatches : Bool) (identity : Nat) (legacy : Bool)
    (canonical : current = true → legacy = shapeMatches) :
    selectedShape current shapeMatches identity legacy = legacy := by
  cases current with
  | false => rfl
  | true =>
    have h := canonical rfl
    subst legacy
    cases shapeMatches <;> by_cases zero : identity = 0 <;>
      simp [selectedShape, metadataResult, zero]

/-- GC batches two independent entries. A positive answer requires both;
    a decline retains the whole original short-circuit path. -/
def packedPair (a b : Facts) : Bool := packed a && packed b

def selectedPair (a b : Facts) : Bool :=
  if packedPair a b then true else separate a && separate b

theorem packed_pair_sound (a b : Facts) (h : packedPair a b = true) :
    separate a = true ∧ separate b = true := by
  have both : packed a = true ∧ packed b = true := by
    simpa [packedPair, Bool.and_eq_true] using h
  exact ⟨packed_sound a both.1, packed_sound b both.2⟩

theorem selected_pair_equivalent (a b : Facts) :
    selectedPair a b = (separate a && separate b) := by
  unfold selectedPair
  split
  · rename_i h
    rcases packed_pair_sound a b h with ⟨ha, hb⟩
    simp [ha, hb]
  · rfl

/-- Stable binding admission never substitutes for current policy or the
    final fence after dependency callbacks. -/
def commitGuard (bindings permission : Bool) (captured current : Nat) : Bool :=
  bindings && permission && (captured == current)

theorem revoked_permission_blocks_commit (bindings : Bool) (captured current : Nat) :
    commitGuard bindings false captured current = false := by simp [commitGuard]

theorem changed_epoch_blocks_commit (bindings permission : Bool) (captured current : Nat)
    (changed : captured ≠ current) : commitGuard bindings permission captured current = false := by
  simp [commitGuard, changed]

theorem pair_admission_saves_three (a b : Facts) (h : packedPair a b = true) :
    (if packedPair a b then 1 else 5) + 3 = (4 : Nat) := by simp [h]

/-- One metadata request covers all inputs only under current canonical
    query/getter meanings and representable nonzero allocation identities. -/
def batchMetadata (services own single known : Bool) (shapes : List Bool) : Option Bool :=
  if services && own && single && known then some (shapes.all id) else none

theorem metadata_batch_requires_every_shape (services own single known : Bool) (shapes : List Bool)
    (success : batchMetadata services own single known shapes = some true) :
    services = true ∧ own = true ∧ single = true ∧ known = true ∧ shapes.all id = true := by
  unfold batchMetadata at success
  split at success
  · rename_i gate
    have g : services = true ∧ own = true ∧ single = true ∧ known = true := by
      simpa [Bool.and_eq_true, and_assoc] using gate
    exact ⟨g.1, g.2.1, g.2.2.1, g.2.2.2, Option.some.inj success⟩
  · contradiction

theorem unknown_batch_identity_declines (services own single : Bool) (shapes : List Bool) :
    batchMetadata services own single false shapes = none := by simp [batchMetadata]

theorem metadata_batch_saves_inputs_minus_one (inputs : Nat) (positive : 1 ≤ inputs) :
    1 + (inputs - 1) = inputs := by omega

end Birnam.Admission
