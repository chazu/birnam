import Semantics
namespace Birnam.SemanticsExamples
open Birnam.Semantics
inductive Owner where
  | integer | word | bytes | vector
  deriving DecidableEq

/-- Payload-owner obligations only, under prevalidated, callback-free kernel
    assumptions. These are not total-send minima for the real language calls. -/
def additionKernel : List Owner := [.integer,.integer,.word,.word,.word,.word,.word,.integer]
def stringLengthKernel : List Owner := [.bytes,.integer]
def collectionKernel : List Owner := [.integer,.vector]

theorem addition_owner_floor : ownerRuns additionKernel = 3 := by decide
theorem string_owner_floor : ownerRuns stringLengthKernel = 2 := by decide
theorem collection_owner_floor : ownerRuns collectionKernel = 2 := by decide

theorem addition_abstract_schedule : OwnerSchedule additionKernel 3 := by
  exact .batch Owner.integer 1 (.batch Owner.word 4 (.batch Owner.integer 0 .done))
theorem string_abstract_schedule : OwnerSchedule stringLengthKernel 2 := by
  exact .batch Owner.bytes 0 (.batch Owner.integer 0 .done)
theorem collection_abstract_schedule : OwnerSchedule collectionKernel 2 := by
  exact .batch Owner.integer 0 (.batch Owner.vector 0 .done)

def world : World Nat := ⟨fun _ => 7, fun _ => true, 1⟩
def certificate : Certificate Nat := capture world [0,1] [2]

theorem valid_initial : Valid world certificate :=
  capture_valid world [0,1] [2] (by simp [world])

theorem epoch_change_alone_is_not_invalidation :
    Valid {world with epoch := 20} certificate := by
  exact valid_initial

theorem same_epoch_revoked_permission_is_invalid :
    ¬ Valid {world with grants := fun _ => false} certificate :=
  permission_revocation_invalidates 2 (by simp [certificate,capture]) rfl

theorem changed_dependency_is_invalid :
    ¬ Valid {world with meanings := fun _ => 8} certificate :=
  replacement_invalidates 0 (by simp [certificate,capture]) (by decide)

def referenceFirst : Action Nat := fun n => ⟨n+1,[.userEffect 1 10],4⟩
def optimizedFirst : Action Nat := fun n => ⟨n+1,[.userEffect 1 10],1⟩
def referenceSecond : Action Nat := fun n => ⟨n*2,[.result (n*2)],3⟩
def optimizedSecond : Action Nat := fun n => ⟨n*2,[.result (n*2)],1⟩

theorem first_refines : Refines Eq referenceFirst optimizedFirst := by
  intro a b same; subst b; exact ⟨rfl,rfl,by simp [referenceFirst,optimizedFirst]⟩
theorem second_refines : Refines Eq referenceSecond optimizedSecond := by
  intro a b same; subst b; exact ⟨rfl,rfl,by simp [referenceSecond,optimizedSecond]⟩
theorem composed_example_refines :
    Refines Eq (thenDo referenceFirst referenceSecond) (thenDo optimizedFirst optimizedSecond) :=
  compose_refinement first_refines second_refines

theorem dropping_visible_effect_is_invalid :
    ¬ Refines Eq referenceFirst (fun n => ⟨n+1,[],1⟩) := by
  intro h
  have events := (h 0 0 rfl).2.1
  simp [referenceFirst] at events

def interfaceCapacity : Owner → Nat
  | .word => 32
  | _ => 1

theorem unary_integer_access_schedule : BoundedOwnerSchedule interfaceCapacity additionKernel 4 := by
  exact .batch Owner.integer 0 (by decide)
    (.batch Owner.integer 0 (by decide)
      (.batch Owner.word 4 (by decide) (.batch Owner.integer 0 (by decide) .done)))

/-- Even a lexical temporary can escape through an ordinary replaced
    constructor. Eliminating its allocation must preserve the observable hook
    and identity, or decline before executing that hook. -/
def boxedConversion : Action Nat := fun n =>
  ⟨n, [.allocationHook 1 2, .identity 1 100, .result n], 3⟩
def rawConversion : Action Nat := fun n => ⟨n, [.result n], 0⟩

theorem unguarded_box_elision_is_not_a_refinement :
    ¬ Refines Eq boxedConversion rawConversion := by
  intro h
  have observations := (h 0 0 rfl).2.1
  simp [boxedConversion, rawConversion] at observations

/-- Constructor/getter bindings are explicit dependencies, not implied by
    unchanged arithmetic operators or a valid unsigned overflow proof. -/
def conversionCertificate : Certificate Nat := capture world [10,11] [2]

theorem changed_conversion_binding_invalidates
    (key : Nat) (required : key ∈ [10,11]) :
    ¬ Valid {world with meanings := fun _ => 8} conversionCertificate := by
  apply replacement_invalidates key
  · simpa [conversionCertificate, capture] using required
  · simp [conversionCertificate, capture, world]

end Birnam.SemanticsExamples
