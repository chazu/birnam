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
-- Proposed ordinary-result contract. Native diagnostics are data at the
-- ordinary-send boundary; a returned Error is not a separate hidden failure bit.
inductive ErrorValue where
  | application (identity : Nat)
  | native (status detail : Nat)
  deriving DecidableEq
inductive MessageResult where
  | value (identity : Nat)
  | error (error : ErrorValue)
  deriving DecidableEq
inductive TransportResult where
  | loaded (result : MessageResult)
  | failed (status detail : Nat)
  deriving DecidableEq

def normalize : TransportResult → MessageResult
  | .loaded result => result
  | .failed status detail => .error (.native status detail)

def acknowledge (result : MessageResult) : MessageResult :=
  match result with
  | .error error => .error error
  | .value _ => .value 0

theorem ordinary_error_identity_preserved (e : ErrorValue) :
    acknowledge (normalize (.loaded (.error e))) = .error e := by rfl

theorem native_diagnostics_are_data (status detail : Nat) :
    acknowledge (normalize (.failed status detail)) = .error (.native status detail) := by rfl

-- The rollback token must contain the planned installed value, captured before
-- callbacks. A fresh read after callbacks is not authority to undo that value.
def undoIfOwned (before expected current : Nat) : Nat :=
  if current = expected then before else current

theorem rollback_preserves_intervening_write (before expected current : Nat)
    (changed : current ≠ expected) :
    undoIfOwned before expected current = current := by simp [undoIfOwned, changed]

theorem rollback_restores_own_write (before expected : Nat) :
    undoIfOwned before expected expected = before := by simp [undoIfOwned]

theorem learning_expected_after_callback_is_wrong :
    undoIfOwned 1 3 3 ≠ undoIfOwned 1 2 3 := by decide

-- Basic live loading reports failure without undoing applied values. This models
-- the failure boundary, not staging, native resource safety or arbitrary callbacks.
structure PartialLoad where
  values : Nat → Nat
  prepared : Bool
  composition : Option Nat
  failure : Nat
  deriving Inhabited

def failLiveLoad (world : PartialLoad) (error : Nat) : PartialLoad :=
  { world with prepared := false, composition := none, failure := error }

theorem failed_load_keeps_values (world : PartialLoad) (error key : Nat) :
    (failLiveLoad world error).values key = world.values key := by rfl

theorem failed_load_drops_authority (world : PartialLoad) (error : Nat) :
    (failLiveLoad world error).prepared = false ∧
    (failLiveLoad world error).composition = none := by constructor <;> rfl

theorem failed_load_preserves_error (world : PartialLoad) (error : Nat) :
    (failLiveLoad world error).failure = error := by rfl

-- Image application releases old authority at the boundary. Cleanup has no
-- saved predecessor to reinstate; executable ownership is independent.
structure ImageApplication where
  capabilities : Bool
  applying : Bool
  sources : Nat
  staged : Option Nat
  codeOwners : Nat
  deriving DecidableEq

def beginApplication (w : ImageApplication) : ImageApplication :=
  { w with capabilities := false, applying := true }

def cleanApplication (w : ImageApplication) : ImageApplication :=
  { w with applying := false, staged := none }

def installSources (w : ImageApplication) (incoming : Nat) : ImageApplication :=
  { w with sources := incoming, staged := none }

theorem preapplication_cleanup_preserves_capabilities (w : ImageApplication) :
    (cleanApplication w).capabilities = w.capabilities := by rfl

theorem application_releases_capabilities (w : ImageApplication) :
    (beginApplication w).capabilities = false := by rfl

theorem cleanup_cannot_revive_capabilities (w : ImageApplication) :
    (cleanApplication (beginApplication w)).capabilities = false := by rfl

theorem application_preserves_code_owners (w : ImageApplication) :
    (beginApplication w).codeOwners = w.codeOwners := by rfl

theorem cleanup_preserves_installed_sources (w : ImageApplication) (incoming : Nat) :
    (cleanApplication (installSources w incoming)).sources = incoming := by rfl

theorem installation_consumes_stage (w : ImageApplication) (incoming : Nat) :
    (installSources w incoming).staged = none := by rfl

end Birnam.PublicationFailure
