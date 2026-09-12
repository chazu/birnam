//! Integer dataflow derived after native implementation admission. Selector
//! spellings never establish the operation represented here.

pub const MAX_INPUTS: usize = 64;
pub const MAX_OPERATIONS: usize = 32;

/// Codes returned by the optional native numeric semantics v1 declaration.
/// This declares only successful canonical-integer behavior. Errors, mixed
/// operands and allocation/observation admission remain runtime obligations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Operator {
    Add = 1,
    Subtract = 2,
    Multiply = 3,
    Divide = 4,
    Remainder = 5,
}

impl Operator {
    pub fn from_declaration(code: u32) -> Option<Self> {
        Some(match code {
            1 => Self::Add,
            2 => Self::Subtract,
            3 => Self::Multiply,
            4 => Self::Divide,
            5 => Self::Remainder,
            _ => return None,
        })
    }

    /// The native Prelude uses i128 intermediates, including MIN % -1 == 0.
    /// A refusal means execute the original operation; it is not an Id Error.
    pub fn evaluate(self, left: i64, right: i64) -> Option<i64> {
        let (left, right) = (left as i128, right as i128);
        let value = match self {
            Self::Add => left + right,
            Self::Subtract => left - right,
            Self::Multiply => left * right,
            Self::Divide if right != 0 => left / right,
            Self::Remainder if right != 0 => left % right,
            _ => return None,
        };
        i64::try_from(value).ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Input(u32),
    Result(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    pub operator: Operator,
    pub left: Value,
    pub right: Value,
}

#[derive(Clone, Debug)]
pub struct Plan {
    inputs: usize,
    operations: Vec<Operation>,
}

impl Plan {
    pub fn new(inputs: usize, operations: Vec<Operation>) -> Option<Self> {
        if inputs > MAX_INPUTS || operations.is_empty() || operations.len() > MAX_OPERATIONS {
            return None;
        }
        for (index, operation) in operations.iter().enumerate() {
            for value in [operation.left, operation.right] {
                match value {
                    Value::Input(input) if input as usize >= inputs => return None,
                    Value::Result(result) if result as usize >= index => return None,
                    _ => {}
                }
            }
        }
        Some(Self { inputs, operations })
    }

    pub fn input_count(&self) -> usize {
        self.inputs
    }
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Inputs and outputs are disjoint arrays of signed 64-bit words. Zero
    /// means success; N+1 means the first failing operation is N. Only outputs
    /// preceding that operation have been written. All intermediate values
    /// remain raw, with no allocation or Id callback in the generated code.
    pub fn emit(&self) -> Vec<u8> {
        let mut source = String::from(HELPERS);
        source.push_str("unsigned long numeric_chain(const long long *in, long long *out){\n");
        let reference = |value| match value {
            Value::Input(index) => format!("in[{index}]"),
            Value::Result(index) => format!("out[{index}]"),
        };
        for (index, operation) in self.operations.iter().enumerate() {
            source.push_str(&format!(
                "if(!numeric_{}({}, {}, &out[{}])) return {};\n",
                operation.operator as u32,
                reference(operation.left),
                reference(operation.right),
                index,
                index + 1,
            ));
        }
        source.push_str("return 0;\n}\n");
        source.into_bytes()
    }
}

// TinyCC does not need a native 128-bit type or compiler overflow builtins.
// Each signed operation executes only after a range proof. The MIN/-1 cases
// avoid C division/remainder UB and retain Prelude's i128 result semantics.
const HELPERS: &str = r#"
#define NUM_MAX 9223372036854775807LL
#define NUM_MIN (-9223372036854775807LL-1LL)
static int numeric_1(long long a,long long b,long long *r){
 if((b>0 && a>NUM_MAX-b)||(b<0 && a<NUM_MIN-b))return 0;
 *r=a+b;return 1;
}
static int numeric_2(long long a,long long b,long long *r){
 if((b<0 && a>NUM_MAX+b)||(b>0 && a<NUM_MIN+b))return 0;
 *r=a-b;return 1;
}
static int numeric_3(long long a,long long b,long long *r){
 if(a>0){if((b>0 && a>NUM_MAX/b)||(b<0 && b<NUM_MIN/a))return 0;}
 else if(a<0){if((b>0 && a<NUM_MIN/b)||(b<0 && a<NUM_MAX/b))return 0;}
 *r=a*b;return 1;
}
static int numeric_4(long long a,long long b,long long *r){
 if(b==0 || (a==NUM_MIN && b==-1))return 0;
 *r=a/b;return 1;
}
static int numeric_5(long long a,long long b,long long *r){
 if(b==0)return 0;
 *r=(a==NUM_MIN && b==-1)?0:a%b;return 1;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_prelude_extrema_and_refuses_errors() {
        assert_eq!(Operator::Add.evaluate(i64::MAX, 1), None);
        assert_eq!(Operator::Subtract.evaluate(i64::MIN, 1), None);
        assert_eq!(Operator::Multiply.evaluate(i64::MIN, -1), None);
        assert_eq!(Operator::Divide.evaluate(i64::MIN, -1), None);
        assert_eq!(Operator::Remainder.evaluate(i64::MIN, -1), Some(0));
        for op in [Operator::Divide, Operator::Remainder] {
            assert_eq!(op.evaluate(1, 0), None);
        }
        assert_eq!(Operator::Remainder.evaluate(-11, 3), Some(-2));
    }

    #[test]
    fn refuses_invalid_dataflow_and_resource_limits() {
        let operation = Operation {
            operator: Operator::Add,
            left: Value::Input(0),
            right: Value::Input(1),
        };
        assert!(Plan::new(1, vec![operation]).is_none());
        assert!(Plan::new(MAX_INPUTS + 1, vec![operation]).is_none());
        assert!(Plan::new(2, vec![operation; MAX_OPERATIONS + 1]).is_none());
        assert!(Plan::new(2, vec![]).is_none());
        assert!(
            Plan::new(
                2,
                vec![Operation {
                    right: Value::Result(0),
                    ..operation
                }]
            )
            .is_none()
        );
        assert!(
            Plan::new(
                2,
                vec![
                    operation,
                    Operation {
                        left: Value::Result(0),
                        ..operation
                    }
                ]
            )
            .is_some()
        );
    }
}
