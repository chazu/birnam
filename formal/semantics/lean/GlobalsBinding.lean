import PrivateScratch

/-! Binding-owner protocol model. Nat values are identities (0 may denote nil);
    absence is Option.none. This does not prove the Rust adapters, callbacks,
    GC or image recovery. Those retain executable gates. -/
namespace Birnam.GlobalsBinding
open Birnam.PrivateScratch

def encode : Option Nat → List Nat
  | none => []
  | some value => [value]

def decode : List Nat → Option (Option Nat)
  | [] => some none
  | [value] => some (some value)
  | _ => none

theorem snapshot_roundtrip (value : Option Nat) : decode (encode value) = some value := by
  cases value <;> rfl

theorem nil_is_not_absence : encode (some 0) ≠ encode none := by decide

theorem malformed_snapshot_rejected (a b : Nat) (rest : List Nat) :
    decode (a :: b :: rest) = none := by rfl

abbrev Bindings := Nat → Option Nat

def write (bindings : Bindings) (name : Nat) (value : Option Nat) :=
  put bindings name value

theorem write_installs_value (bindings : Bindings) (name : Nat) (value : Option Nat) :
    write bindings name value name = value := by simp [write, put]

theorem foreign_binding_preserved (bindings : Bindings) (name other : Nat)
    (value : Option Nat) (different : other ≠ name) :
    write bindings name value other = bindings other := by simp [write, put, different]

/-! Ordinary owner roots and unpublished dictionary construction. -/
def compareSwap (current expected replacement : Nat) :=
  if current = expected then replacement else current

theorem stale_root_not_overwritten (current expected replacement : Nat) (h : current ≠ expected) :
    compareSwap current expected replacement = current := by simp [compareSwap, h]

theorem matched_root_published (current replacement : Nat) :
    compareSwap current current replacement = replacement := by simp [compareSwap]

structure DictionaryWorld where
  dictionaries : Nat → Option Nat
  epoch : Nat

def construct (world : DictionaryWorld) (fresh method : Nat) : DictionaryWorld :=
  { world with dictionaries := put world.dictionaries fresh (some method) }

theorem construction_preserves_old_lookup (world : DictionaryWorld) (fresh old method : Nat)
    (different : old ≠ fresh) :
    (construct world fresh method).dictionaries old = world.dictionaries old := by
  simp [construct, put, different]

theorem construction_preserves_epoch (world : DictionaryWorld) (fresh method : Nat) :
    (construct world fresh method).epoch = world.epoch := by rfl

def edit (world : DictionaryWorld) (target : Nat) (method : Option Nat) : DictionaryWorld :=
  { dictionaries := put world.dictionaries target method, epoch := world.epoch + 1 }

theorem aba_advances_epoch (world : DictionaryWorld) (target : Nat) (temporary original : Option Nat) :
    (edit (edit world target temporary) target original).epoch = world.epoch + 2 := by
  simp [edit, Nat.add_assoc]

structure Name where
  identity : Nat
  spelling : Nat

def sameKey (a b : Name) : Bool := a.identity == b.identity

theorem equal_spelling_does_not_alias :
    sameKey ⟨1, 17⟩ ⟨2, 17⟩ = false := by decide

theorem same_identity_is_same_key (identity a b : Nat) :
    sameKey ⟨identity, a⟩ ⟨identity, b⟩ = true := by simp [sameKey]


def projectBindings (bindings : Bindings) (transient : Nat → Bool) : Bindings :=
  fun name => if transient name then none else bindings name

theorem projection_keeps_persistent (bindings : Bindings) (transient : Nat → Bool) (name : Nat)
    (h : transient name = false) : projectBindings bindings transient name = bindings name := by
  simp [projectBindings, h]

theorem projection_omits_transient (bindings : Bindings) (transient : Nat → Bool) (name : Nat)
    (h : transient name = true) : projectBindings bindings transient name = none := by
  simp [projectBindings, h]

end Birnam.GlobalsBinding
