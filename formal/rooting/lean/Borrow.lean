import Std
namespace Birnam.Borrow
variable {Byte Result : Type}
/-- A fresh native byte copy preserves the complete sequence. -/
def copy (bytes : List Byte) : List Byte := bytes.map id

theorem copy_contents (bytes : List Byte) : copy bytes = bytes := by simp [copy]

/-- A pure parser sees identical input. Result includes errors and owned shape
    scalars. Callback-free lease validity is established by the TLA+ protocol
    and by the provider-free implementation boundary, not assumed from GC. -/
theorem pure_inspection_equivalent (parse : List Byte → Result) (bytes : List Byte) :
    parse (copy bytes) = parse bytes := by rw [copy_contents]
/-- Native bundle slices and copied objects contain the same complete bytes.
    The private snapshot's lifetime across callbacks is a Rust ownership and
    owner-scope obligation; this result does not license a foreign borrow. -/
theorem owned_slice_equivalent (parse : List Byte → Result)
    (bytes : List Byte) (start count : Nat) :
    parse (copy ((bytes.drop start).take count)) =
      parse ((bytes.drop start).take count) := by rw [copy_contents]
end Birnam.Borrow
