import Rooting

/-! A live initialized Block covers its immutable capture edges. Replacement
readers returning fresh values require ordinary result protection. These are
reachability rules with explicit premises, not a proof of the generated C ABI. -/
namespace Birnam.Capture
open Birnam.Rooting
variable {Object : Type}

def withResults (base results : Object → Prop) : Object → Prop :=
  fun o => base o ∨ results o

theorem reachable_results_do_not_need_separate_roots
    (edges : Object → Object → Prop) (base results : Object → Prop)
    (covered : ∀ o, results o → Reach edges base o) :
    ∀ o, Reach edges (withResults base results) o ↔ Reach edges base o := by
  intro o
  constructor
  · intro path
    induction path with
    | root h => exact h.elim Reach.root (covered _)
    | edge _ e ih => exact Reach.edge ih e
  · intro path
    induction path with
    | root h => exact Reach.root (Or.inl h)
    | edge _ e ih => exact Reach.edge ih e

theorem capture_edge_covers_result
    (edges : Object → Object → Prop) (base : Object → Prop)
    (block result : Object) (live : base block) (capture : edges block result) :
    Reach edges base result := Reach.edge (Reach.root live) capture

/-- The receiver can itself be covered transitively by an invocation scope.
    The factory is not needed to keep a receiver-owned capture alive. -/
theorem receiver_covers_capture_results
    (edges : Object → Object → Prop) (base results : Object → Prop)
    (block : Object) (live : Reach edges base block)
    (captures : ∀ o, results o → edges block o) :
    ∀ o, results o → Reach edges base o := by
  intro o h
  exact Reach.edge live (captures o h)

/-- A batch of pure receiver reads survives precise GC without an extra
    factory root or per-result roots. Stable edges, current pure-reader meaning
    and an unchanged covering scope are concrete implementation obligations. -/
theorem receiver_read_batch_survives
    (edges : Object → Object → Prop) (heap base results : Object → Prop)
    (block : Object) (live : Reach edges base block)
    (captures : ∀ o, results o → edges block o)
    (alive : ∀ o, results o → heap o) :
    ∀ o, results o → collect heap base edges o := by
  intro o h
  exact ⟨alive o h, receiver_covers_capture_results edges base results block live captures o h⟩

/-- Copying the handed-off references preserves slot order and repeated aliases;
    the native ABI's byte encoding/memcpy correspondence is checked separately. -/
theorem batch_copy_preserves_order_and_aliases (slots : List Object) :
    slots.map id = slots := by simp

theorem copied_capture_batch_survives
    (edges : Object → Object → Prop) (heap base : Object → Prop)
    (block : Object) (slots : List Object) (live : Reach edges base block)
    (captures : ∀ o, o ∈ slots → edges block o)
    (alive : ∀ o, o ∈ slots → heap o) :
    ∀ o, o ∈ slots.map id → collect heap base edges o := by
  simpa using receiver_read_batch_survives edges heap base (fun o => o ∈ slots)
    block live captures alive

def admitBulk (bulkCurrent readerCurrent queryCurrent servicesCurrent : Bool) : Bool :=
  bulkCurrent && readerCurrent && queryCurrent && servicesCurrent

theorem bulk_admission_requires_own_current_binding
    (bulk reader query services : Bool) :
    admitBulk bulk reader query services = true ↔
      bulk = true ∧ reader = true ∧ query = true ∧ services = true := by
  simp [admitBulk, and_assoc]

def admitBorrow (readerCurrent queryCurrent servicesCurrent : Bool) : Bool :=
  readerCurrent && queryCurrent && servicesCurrent

/-- A query's forwarding wrapper is not a canonical grant, even if it calls
    the canonical query before replacing the reader. Admission is per batch. -/
theorem borrow_admission_requires_current_bindings
    (reader query services : Bool) :
    admitBorrow reader query services = true ↔
      reader = true ∧ query = true ∧ services = true := by
  simp [admitBorrow, and_assoc]

theorem borrowed_needed_survives
    (edges : Object → Object → Prop) (heap base results needed : Object → Prop)
    (covered : ∀ o, results o → Reach edges base o)
    (alive : ∀ o, needed o → heap o)
    (reachable : ∀ o, needed o → Reach edges (withResults base results) o) :
    ∀ o, needed o → collect heap base edges o := by
  intro o h
  exact ⟨alive o h, (reachable_results_do_not_need_separate_roots edges base results covered o).mp (reachable o h)⟩

-- A fresh replacement result is not protected by keeping only its Block alive.
theorem fresh_result_requires_coverage :
    ¬ collect (fun _ : Nat => True) (fun o => o = 0) (fun _ _ => False) 1 := by
  rintro ⟨_, path⟩
  cases path with
  | root h => omega
  | edge _ h => contradiction
/-- All selectors are supplied by a live owner inventory before admission.
    Name-service replacements are therefore outside the capture-read dependency
    set; lookup, binding and the matcher remain current witnesses. -/
def selectorResolvedAdmission (lookup binding matcher _intern _symbols : Bool) : Bool :=
  lookup && binding && matcher

theorem resolved_admission_requires_dispatch_services (lookup binding matcher intern symbols : Bool) :
    selectorResolvedAdmission lookup binding matcher intern symbols = true ↔
      lookup = true ∧ binding = true ∧ matcher = true := by
  simp [selectorResolvedAdmission, and_assoc]

theorem unrelated_name_service_does_not_change_admission
    (lookup binding matcher oldIntern oldSymbols newIntern newSymbols : Bool) :
    selectorResolvedAdmission lookup binding matcher oldIntern oldSymbols =
      selectorResolvedAdmission lookup binding matcher newIntern newSymbols := by rfl

end Birnam.Capture
