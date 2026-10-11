import Std

/-! Indexed diagnostic transport, not a proof of Rust, the bridge or all Trace
    dispatch. An immutable snapshot assigns each admitted object one index.
    Decoding uses that snapshot, not numeric allocation identities. The caller
    explicitly selects this wire protocol before receiving any facts. -/
namespace Birnam.IndexedSnapshot

variable {Object Index : Type}

def encodeEdges (index : Object → Index) (edges : List (Object × Object)) :=
  edges.map fun (a, b) => (index a, index b)

def decodeEdges (object : Index → Object) (edges : List (Index × Index)) :=
  edges.map fun (a, b) => (object a, object b)

/-- A left inverse preserves aliases, root order and duplicates. -/
theorem roots_roundtrip (index : Object → Index) (object : Index → Object)
    (inverse : ∀ x, object (index x) = x) (roots : List Object) :
    (roots.map index).map object = roots := by
  induction roots with
  | nil => rfl
  | cons root rest ih => simp [inverse, ih]

/-- Every edge, including an unreachable edge, survives in its original order. -/
theorem edges_roundtrip (index : Object → Index) (object : Index → Object)
    (inverse : ∀ x, object (index x) = x) (edges : List (Object × Object)) :
    decodeEdges object (encodeEdges index edges) = edges := by
  induction edges with
  | nil => rfl
  | cons edge rest ih =>
    rcases edge with ⟨a, b⟩
    simp_all [encodeEdges, decodeEdges]

/-- The same graph policy has the same result on decoded inputs. This does not
    assert that internal protocol messages or physical allocations are equal. -/
theorem graph_policy_preserved {Result : Type}
    (policy : List Object → List (Object × Object) → Result)
    (index : Object → Index) (object : Index → Object)
    (inverse : ∀ x, object (index x) = x)
    (roots : List Object) (edges : List (Object × Object)) :
    policy ((roots.map index).map object) (decodeEdges object (encodeEdges index edges)) =
      policy roots edges := by
  rw [roots_roundtrip index object inverse, edges_roundtrip index object inverse]

/-- Different allocations must not collapse onto one index. -/
theorem inverse_implies_injective (index : Object → Index) (object : Index → Object)
    (inverse : ∀ x, object (index x) = x) (a b : Object) (same : index a = index b) :
    a = b := by
  calc a = object (index a) := (inverse a).symm
       _ = object (index b) := congrArg object same
       _ = b := inverse b

/-- Failure recovery retains completed effects; fallback is only valid before
    any indexed fact has been consumed, never by replaying a prefix. -/
def resume {Event : Type} (prior suffix : List Event) := prior ++ suffix

theorem resume_keeps_prefix {Event : Type} (prior suffix : List Event) :
    (resume prior suffix).take prior.length = prior := by simp [resume]

end Birnam.IndexedSnapshot
