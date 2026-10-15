/-! Failure-channel obligations at an authored publication boundary. This models
    the observed carrier conversion and dynamic policy, not compiler execution,
    native ABI layout, GC, or the resumable engine's additional failure paths. -/
namespace Birnam.PublicationFailure
inductive Value where
  | ok | error
  deriving DecidableEq
inductive Outcome where
  | success (value : Value)
  | failure (status detail : Nat)
  deriving DecidableEq

def adapt (preserve : Bool) (result : Outcome) : Outcome :=
  match result with
  | .success value => .success value
  | .failure status detail =>
    if preserve then .failure status detail else .success .error

/-- The current notification protocol ignores every successful value, including
    an ordinary Error. Only the native failure channel aborts publication. -/
def notify (result : Outcome) : Outcome :=
  match result with
  | .success _ => .success .ok
  | .failure status detail => .failure status detail

/-- A callback explicitly handles the Error produced by an inner native send. -/
def callback (preserve : Bool) : Outcome :=
  match adapt preserve (.failure 881 17) with
  | .success .error => .success .ok
  | other => other

theorem conversion_collapses_channels :
    adapt false (.failure 881 17) = adapt false (.success .error) := by rfl

theorem conversion_drops_detail :
    adapt false (.failure 881 17) = adapt false (.failure 882 99) := by rfl

theorem no_total_decoder :
    ¬ ∃ decode : Outcome → Outcome, ∀ result, decode (adapt false result) = result := by
  rintro ⟨decode, exact⟩
  have a := exact (.failure 881 17)
  have b := exact (.success .error)
  change decode (.success .error) = .failure 881 17 at a
  change decode (.success .error) = .success .error at b
  have bad := a.symm.trans b
  cases bad

theorem plain_notification_swallows_failure :
    notify (adapt false (.failure 881 17)) ≠ notify (.failure 881 17) := by decide

theorem ordinary_error_is_void : notify (.success .error) = .success .ok := by rfl

theorem preservation_changes_callback :
    callback false = .success .ok ∧ callback true = .failure 881 17 := by constructor <;> rfl

/-- A raw tail send need not change the callback's internal conversion policy.
    This alone is not a sequencing or general outcome-capture protocol. -/
def rawTail (result : Outcome) := result

theorem raw_tail_preserves_both_cases :
    rawTail (.failure 881 17) = .failure 881 17 ∧
    rawTail (callback false) = .success .ok := by constructor <;> rfl
end Birnam.PublicationFailure
