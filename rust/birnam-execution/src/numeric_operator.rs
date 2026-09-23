//! Checked numeric evaluation shared privately with compiler lowering.
/// Stable operation codes shared with Integer's borrowed evaluator protocol.
/// Only successful canonical-integer behavior is computed here. Integer owns
/// errors and mixed operands; Execution owns observation and dispatch admission.
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

    /// Preserve checked i64 arithmetic with i128 intermediates, including MIN % -1 == 0.
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
