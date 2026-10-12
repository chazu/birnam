import Std

-- Contents-only model of the existing two-state decoder. It is deliberately
-- not a claim that the rewritten algorithm makes the old internal sends.
namespace Birnam.JsonStrings

def escaped (byte : Nat) : Nat :=
  if byte = 110 then 10 else if byte = 116 then 9 else if byte = 114 then 13 else byte

def decode : Bool → List Nat → List Nat
  | _, [] => []
  | true, _ :: rest => decode false rest
  | false, byte :: rest =>
    if byte = 92 then
      match rest with
      | [] => [] -- Outside the valid-token domain; fast branch cannot take it.
      | following :: _ => escaped following :: decode true rest
    else byte :: decode false rest

theorem plain_decode (bytes : List Nat) (plain : 92 ∉ bytes) :
    decode false bytes = bytes := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
    have different : byte ≠ 92 := by intro h; subst byte; simp at plain
    have tailPlain : 92 ∉ rest := by intro h; exact plain (List.mem_cons_of_mem byte h)
    simp [decode, different, ih tailPlain]

def fast (bytes : List Nat) := if 92 ∈ bytes then decode false bytes else bytes

theorem same_contents (bytes : List Nat) : fast bytes = decode false bytes := by
  unfold fast
  split
  · rfl
  · rename_i plain
    exact (plain_decode bytes plain).symm

-- Ordinary nonempty decoding copies; empty decoding returns its receiver.
def resultIdentity (input fresh : Nat) (bytes : List Nat) :=
  if bytes.isEmpty then input else fresh

theorem empty_identity (input fresh : Nat) : resultIdentity input fresh [] = input := by rfl

theorem nonempty_fresh (input fresh byte : Nat) (rest : List Nat) (distinct : fresh ≠ input) :
    resultIdentity input fresh (byte :: rest) ≠ input := by simpa [resultIdentity] using distinct

-- Quoted decoding already owns a fresh slice. Reusing that slice is different
-- from returning the original quoted token, even for an empty decoded body.
def quotedIdentity (body decoded : Nat) (needsEscapes : Bool) : Nat :=
  if needsEscapes then decoded else body

theorem quoted_result_fresh (token body decoded : Nat) (needsEscapes : Bool)
    (bodyFresh : body ≠ token) (decodedFresh : decoded ≠ token) :
    quotedIdentity body decoded needsEscapes ≠ token := by
  cases needsEscapes <;> simp_all [quotedIdentity]

theorem quoted_calls_distinct (body1 body2 decoded1 decoded2 : Nat) (needsEscapes : Bool)
    (bodies : body1 ≠ body2) (decoded : decoded1 ≠ decoded2) :
    quotedIdentity body1 decoded1 needsEscapes ≠ quotedIdentity body2 decoded2 needsEscapes := by
  cases needsEscapes <;> simp_all [quotedIdentity]

end Birnam.JsonStrings
