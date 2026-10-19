/-! Ordinary resource-owner protocol. Nat is a connection-record identity, not a
    numeric OS descriptor. Claim precedes cleanup callbacks; no automatic retry
    follows a close error. Callbacks may install a fresh replacement. Arbitrary
    resurrection of consumed capabilities and replaced protocol implementations
    are outside this default protocol, not forbidden by a runtime guard. -/
namespace Birnam.ResourceOwner

def claim (current : Option Nat) (expected : Nat) : Option Nat × Option Nat :=
  if current = some expected then (none, some expected) else (current, none)

theorem claim_detaches (expected : Nat) :
    (claim (some expected) expected).1 = none := by simp [claim]

theorem claim_retains_cleanup_input (expected : Nat) :
    (claim (some expected) expected).2 = some expected := by simp [claim]

theorem reentry_cannot_reclaim (expected : Nat) :
    (claim (claim (some expected) expected).1 expected).2 = none := by simp [claim]

theorem stale_observation_preserves_owner (current : Option Nat) (expected : Nat)
    (changed : current ≠ some expected) :
    claim current expected = (current, none) := by simp [claim, changed]

def finish (current : Option Nat) (held : Nat) (status : Int) : Option Nat × List Nat × Int :=
  (current, [held], status)

theorem cleanup_preserves_replacement (current : Option Nat) (held : Nat) (status : Int) :
    (finish current held status).1 = current := by rfl

theorem cleanup_attempts_once_even_on_error (current : Option Nat) (held : Nat) (status : Int) :
    (finish current held status).2.1.length = 1 := by rfl

theorem cleanup_preserves_status (current : Option Nat) (held : Nat) (status : Int) :
    (finish current held status).2.2 = status := by rfl

def imageable : Option Nat → Bool
  | none => true
  | some _ => false

theorem live_owner_refuses_image (held : Nat) : imageable (some held) = false := by rfl

theorem closed_owner_allows_image : imageable none = true := by rfl

end Birnam.ResourceOwner
