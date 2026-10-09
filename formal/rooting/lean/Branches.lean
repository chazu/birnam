import Invocation
import Selection

/-! Default branch policy expressed as reused handlers. Helper construction and
    private root traces are not observations. Actual operand/error/factory order
    is. Custom retained operations execute normally; this is not their proof. -/
namespace Birnam.Branches
open Birnam.Rooting
inductive Truth where | yes | no | other deriving DecidableEq
inductive Event where
  | leftRead | errorCheck | rightRead | floatRead | asFloat | integerOp
  | floatOp | wrappingAdd | overflowCheck | integerFactory | errorFactory
  deriving DecidableEq
inductive Answer where | nil | sameError | integer | float | typeError | overflow
  deriving DecidableEq
structure Inputs where
  leftNil : Truth
  exactError : Truth
  rightInteger : Truth
  floatReady : Truth
  fits : Truth

def choose {α : Type} (t : Truth) (yes no other : α) : α :=
  match t with | .yes => yes | .no => no | .other => other

abbrev Result := List Event × Answer
def referenceAdd (c : Inputs) : Result :=
  match c.fits with
  | .yes => ([.wrappingAdd, .overflowCheck, .integerFactory], .integer)
  | .no => ([.wrappingAdd, .overflowCheck, .errorFactory], .overflow)
  | .other => ([.wrappingAdd, .overflowCheck], .nil)
def handlerAdd (c : Inputs) : Result :=
  let branch := choose c.fits ([.integerFactory], Answer.integer)
    ([.errorFactory], Answer.overflow) ([], Answer.nil)
  ([.wrappingAdd, .overflowCheck] ++ branch.1, branch.2)

def prepend (event : Event) (r : Result) : Result := (event :: r.1, r.2)
def reference (c : Inputs) : Result :=
  prepend .leftRead (match c.leftNil with
  | .yes => ([], .nil)
  | .other => ([], .nil)
  | .no => prepend .errorCheck (match c.exactError with
    | .yes => ([], .sameError)
    | .other => ([], .nil)
    | .no => prepend .rightRead (match c.rightInteger with
      | .yes => prepend .integerOp (referenceAdd c)
      | .other => ([], .nil)
      | .no => prepend .floatRead (match c.floatReady with
        | .yes => ([.asFloat, .floatOp], .float)
        | .no => ([.errorFactory], .typeError)
        | .other => ([], .nil)))))
def floatHandler (c : Inputs) : Result :=
  prepend .floatRead (choose c.floatReady ([.asFloat, .floatOp], Answer.float)
    ([.errorFactory], Answer.typeError) ([], Answer.nil))
def integerHandler (c : Inputs) : Result :=
  prepend .rightRead (choose c.rightInteger (prepend .integerOp (handlerAdd c))
    (floatHandler c) ([], Answer.nil))
def errorHandler (c : Inputs) : Result :=
  prepend .errorCheck (choose c.exactError ([], Answer.sameError)
    (integerHandler c) ([], Answer.nil))
def reused (c : Inputs) : Result :=
  prepend .leftRead (choose c.leftNil ([], Answer.nil) (errorHandler c) ([], Answer.nil))

theorem shared_add_preserves_checks_and_result (c : Inputs) : handlerAdd c = referenceAdd c := by
  cases h : c.fits <;> simp [handlerAdd, referenceAdd, choose, h]

theorem shared_handlers_preserve_effect_order_and_result (c : Inputs) : reused c = reference c := by
  rcases c with ⟨l, e, r, f, fit⟩
  cases l <;> cases e <;> cases r <;> cases f <;> cases fit <;>
    rfl

theorem only_selected_handler_runs {α : Type} (t : Truth) (yes no other : α) :
    choose t yes no other = yes ∨ choose t yes no other = no ∨ choose t yes no other = other := by
  cases t <;> simp [choose]

/-- Native numeric wrappers use the same unbounded invocation rule as generated
    bodies. Caller coverage lasts through reentry; fresh allocations inside the
    callee require their own protection. -/
theorem native_numeric_inputs_need_no_duplicate_roots
    {Object : Type} (caller inputs locals heap needed : Object → Prop)
    (covered : Birnam.Invocation.Covered caller inputs)
    (edges : Object → Object → Prop)
    (alive : ∀ o, needed o → heap o)
    (live : ∀ o, needed o → Reach edges (Birnam.Invocation.ordinary caller inputs locals) o) :
    ∀ o, needed o → collect heap (Birnam.Invocation.borrowed caller locals) edges o :=
  Birnam.Invocation.needed_survives caller inputs locals heap needed covered edges alive live

inductive Operation where | add | subtract | multiply | divide | remainder deriving DecidableEq

def integerTable : List Operation := [.add, .subtract, .multiply, .divide, .remainder]
def floatTable : List Operation := [.add, .subtract, .multiply, .divide]

def oldIntegerSelection (op : Operation) : Option Operation := integerTable.find? (· == op)
def directIntegerSelection (op : Operation) : Option Operation := some op
def oldFloatSelection (op : Operation) : Option Operation := floatTable.find? (· == op)
def directFloatSelection (op : Operation) : Option Operation :=
  match op with | .remainder => none | other => some other

theorem operation_objects_preserve_integer_selection (op : Operation) :
    directIntegerSelection op = oldIntegerSelection op := by
  cases op <;> decide

theorem operation_objects_preserve_float_selection (op : Operation) :
    directFloatSelection op = oldFloatSelection op := by
  cases op <;> decide

theorem remainder_has_no_float_handler : directFloatSelection .remainder = none := by rfl

inductive LookupEvent where | nil | globals | value | export deriving DecidableEq
structure LookupInputs where
  nilValue : Nat
  globalValue : Nat
  exportedValue : Option Nat

def oldGlobalResolution (c : LookupInputs) : List LookupEvent × Nat :=
  if c.globalValue = c.nilValue then
    ([.nil, .globals, .value, .export], c.exportedValue.getD c.nilValue)
  else ([.nil, .globals, .value], c.globalValue)
def scopedGlobalResolution (c : LookupInputs) : List LookupEvent × Nat :=
  let nil := c.nilValue
  let found := c.globalValue
  if found ≠ nil then ([.nil, .globals, .value], found)
  else ([.nil, .globals, .value, .export], c.exportedValue.getD nil)

theorem scoped_global_resolution_preserves_order_and_value (c : LookupInputs) :
    scopedGlobalResolution c = oldGlobalResolution c := by
  by_cases same : c.globalValue = c.nilValue <;>
    simp [scopedGlobalResolution, oldGlobalResolution, same]

end Birnam.Branches
