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

end Birnam.Admission
