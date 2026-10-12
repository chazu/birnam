import Std
/-! Canonical valid JSON tokens only. Digit count is bounded by token byte
    length; optional sign/float punctuation cannot increase the digit count.
    Conversion contracts are abstract, not a proof of native parsing or of
    identical internal callbacks after an authored algorithm rewrite. -/
namespace Birnam.JsonNumbers

def decimal : List Nat → Nat
  | [] => 0
  | d :: ds => d * 10 ^ ds.length + decimal ds

theorem decimal_bound (ds : List Nat) (digits : ∀ d ∈ ds, d < 10) :
    decimal ds < 10 ^ ds.length := by
  induction ds with
  | nil => simp [decimal]
  | cons d ds ih =>
    have hd : d ≤ 9 := by have := digits d (by simp); omega
    have tail : ∀ x ∈ ds, x < 10 := by intro x hx; exact digits x (by simp [hx])
    have hmul := Nat.mul_le_mul_right (10 ^ ds.length) hd
    have hrest := ih tail
    simp only [decimal, List.length_cons, Nat.pow_succ]
    omega

theorem short_decimal_fits (ds : List Nat) (digits : ∀ d ∈ ds, d < 10)
    (short : ds.length ≤ 18) : decimal ds < 2 ^ 63 := by
  have h := decimal_bound ds digits
  have p := Nat.pow_le_pow_right (by decide : 10 > 0) short
  have limit : 10 ^ 18 < 2 ^ 63 := by decide
  omega

theorem short_token_fits (ds : List Nat) (bytes : Nat)
    (digits : ∀ d ∈ ds, d < 10) (contained : ds.length ≤ bytes)
    (short : bytes < 19) : decimal ds < 2 ^ 63 := by
  apply short_decimal_fits ds digits
  omega

theorem signed_short_fits (m : Nat) (small : m < 2 ^ 63) :
    -(2 ^ 63 : Int) ≤ -(m : Int) ∧ (m : Int) < (2 ^ 63 : Int) := by
  change m < 9223372036854775808 at small
  change -(9223372036854775808 : Int) ≤ -(m : Int) ∧ (m : Int) < 9223372036854775808
  omega

-- Native conversion results are abstract; this assumes their documented
-- canonical contracts. All actual operations remain current Birnam sends.
def prior {α : Type} (floating : Bool) (number integer : α) : α :=
  if floating then number else integer

def bounded {α : Type} (short floating : Bool) (number integer : α) : α :=
  if short then number else prior floating number integer

theorem bounded_same_value {α : Type} (short floating : Bool) (number integer : α)
    (fits : short = true → floating = false → number = integer) :
    bounded short floating number integer = prior floating number integer := by
  cases short <;> cases floating <;> simp_all [bounded, prior]

theorem long_uses_original {α : Type} (floating : Bool) (number integer : α) :
    bounded false floating number integer = prior floating number integer := by rfl

def lazyClassify {α : Type} (dot lower upper : Bool) (number integer : α) : α :=
  if dot then number else if lower then number else if upper then number else integer

theorem lazy_same_value {α : Type} (dot lower upper : Bool) (number integer : α) :
    lazyClassify dot lower upper number integer = prior (dot || lower || upper) number integer := by
  cases dot <;> cases lower <;> cases upper <;> rfl

end Birnam.JsonNumbers
