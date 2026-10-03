//! Alien signature, ABI classification and wrapper compiler. No foreign
//! addresses, native mappings, libraries or callbacks belong to this owner.

pub mod abi;
pub mod codegen;

use core::fmt;

/// Scalar codes accepted by the frozen Alien signature grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scalar {
    /// 32-bit signed C `int` (`i`).
    Int,
    /// 64-bit signed C `long` (`l`).
    Long,
    /// 64-bit unsigned C `unsigned long` (`u`).
    UnsignedLong,
    /// C `double` (`d`).
    Double,
    /// C `float` (`f`).
    Float,
    /// C `const char *` (`s`).
    StringPointer,
    /// C `void *` (`p`).
    Pointer,
    /// C `void` (`v`), legal only as the return type.
    Void,
}

impl Scalar {
    fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            b'i' => Self::Int,
            b'l' => Self::Long,
            b'u' => Self::UnsignedLong,
            b'd' => Self::Double,
            b'f' => Self::Float,
            b's' => Self::StringPointer,
            b'p' => Self::Pointer,
            b'v' => Self::Void,
            _ => return None,
        })
    }

    fn field_from_byte(byte: u8) -> Option<Self> {
        let scalar = Self::from_byte(byte)?;
        (!matches!(scalar, Self::StringPointer | Self::Void)).then_some(scalar)
    }
}

/// One parsed signature type. Aggregate indices refer to [`Signature::aggregates`]
/// and are registered post-order, so nested definitions always precede users.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    Scalar(Scalar),
    Array { count: usize, element: Scalar },
    Aggregate(usize),
}

/// A by-value C aggregate. Every array or nested aggregate is one positional
/// field in the resulting Birnam Vector representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Aggregate {
    pub fields: Vec<Type>,
}

/// Parsed return/argument types and post-order aggregate definitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub return_type: Type,
    pub arguments: Vec<Type>,
    pub aggregates: Vec<Aggregate>,
}

/// Why a signature failed to parse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    Empty,
    InvalidType { byte: u8, offset: usize },
    VoidArgument { offset: usize },
    EmptyAggregate { offset: usize },
    UnclosedAggregate { offset: usize },
    MissingArrayElement { offset: usize },
    ZeroLengthArray { offset: usize },
    ArrayCountOverflow { offset: usize },
    InvalidArrayElement { byte: u8, offset: usize },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("signature is empty"),
            Self::InvalidType { byte, offset } => {
                write!(formatter, "invalid type byte {byte:#x} at offset {offset}")
            }
            Self::VoidArgument { offset } => {
                write!(
                    formatter,
                    "void is not legal as an argument at offset {offset}"
                )
            }
            Self::EmptyAggregate { offset } => {
                write!(formatter, "aggregate at offset {offset} has no fields")
            }
            Self::UnclosedAggregate { offset } => {
                write!(formatter, "aggregate opened at offset {offset} is unclosed")
            }
            Self::MissingArrayElement { offset } => {
                write!(formatter, "array at offset {offset} has no element type")
            }
            Self::ZeroLengthArray { offset } => {
                write!(formatter, "array at offset {offset} has zero length")
            }
            Self::ArrayCountOverflow { offset } => {
                write!(formatter, "array count overflows at offset {offset}")
            }
            Self::InvalidArrayElement { byte, offset } => write!(
                formatter,
                "invalid array element byte {byte:#x} at offset {offset}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

struct Parser<'a> {
    input: &'a [u8],
    offset: usize,
    aggregates: Vec<Aggregate>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.input.get(self.offset).copied()
    }

    fn parse_field(&mut self) -> Result<Type, ParseError> {
        let offset = self.offset;
        match self.peek() {
            Some(b'{') => self.parse_aggregate(),
            Some(b'0'..=b'9') => {
                let mut count = 0usize;
                while let Some(digit @ b'0'..=b'9') = self.peek() {
                    let digit = usize::from(digit - b'0');
                    count = count
                        .checked_mul(10)
                        .and_then(|value| value.checked_add(digit))
                        .ok_or(ParseError::ArrayCountOverflow { offset })?;
                    self.offset += 1;
                }
                let element_offset = self.offset;
                let element = self
                    .peek()
                    .ok_or(ParseError::MissingArrayElement { offset })?;
                if count == 0 {
                    return Err(ParseError::ZeroLengthArray { offset });
                }
                let Some(element) = Scalar::field_from_byte(element) else {
                    return Err(ParseError::InvalidArrayElement {
                        byte: element,
                        offset: element_offset,
                    });
                };
                self.offset += 1;
                Ok(Type::Array { count, element })
            }
            Some(byte) => {
                let Some(scalar) = Scalar::field_from_byte(byte) else {
                    return Err(ParseError::InvalidArrayElement { byte, offset });
                };
                self.offset += 1;
                Ok(Type::Scalar(scalar))
            }
            None => Err(ParseError::UnclosedAggregate { offset }),
        }
    }

    fn parse_aggregate(&mut self) -> Result<Type, ParseError> {
        let opening_offset = self.offset;
        self.offset += 1;
        let mut fields = Vec::new();
        loop {
            match self.peek() {
                Some(b'}') => {
                    self.offset += 1;
                    break;
                }
                Some(_) => fields.push(self.parse_field()?),
                None => {
                    return Err(ParseError::UnclosedAggregate {
                        offset: opening_offset,
                    });
                }
            }
        }
        if fields.is_empty() {
            return Err(ParseError::EmptyAggregate {
                offset: opening_offset,
            });
        }
        let index = self.aggregates.len();
        self.aggregates.push(Aggregate { fields });
        Ok(Type::Aggregate(index))
    }

    fn parse_top(&mut self, is_return: bool) -> Result<Type, ParseError> {
        let offset = self.offset;
        if self.peek() == Some(b'{') {
            return self.parse_aggregate();
        }
        let Some(byte) = self.peek() else {
            return Err(ParseError::InvalidType { byte: 0, offset });
        };
        let Some(scalar) = Scalar::from_byte(byte) else {
            return Err(ParseError::InvalidType { byte, offset });
        };
        if scalar == Scalar::Void && !is_return {
            return Err(ParseError::VoidArgument { offset });
        }
        self.offset += 1;
        Ok(Type::Scalar(scalar))
    }
}

/// Parse a signature after ignoring the grammar's readability whitespace
/// (ASCII spaces and tabs). Arrays are legal only inside aggregates; `v` is
/// legal only in the return position.
pub fn parse_signature(input: &[u8]) -> Result<Signature, ParseError> {
    let compact: Vec<u8> = input
        .iter()
        .copied()
        .filter(|byte| !matches!(byte, b' ' | b'\t'))
        .collect();
    if compact.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut parser = Parser {
        input: &compact,
        offset: 0,
        aggregates: Vec::new(),
    };
    let return_type = parser.parse_top(true)?;
    let mut arguments = Vec::new();
    while parser.offset < parser.input.len() {
        let argument = parser.parse_top(false)?;
        arguments.push(argument);
    }
    Ok(Signature {
        return_type,
        arguments,
        aggregates: parser.aggregates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalars_and_ignores_readability_whitespace() {
        let signature = parse_signature(b"l s p {dd} i").expect("valid signature");
        assert_eq!(signature.return_type, Type::Scalar(Scalar::Long));
        assert_eq!(
            signature.arguments,
            vec![
                Type::Scalar(Scalar::StringPointer),
                Type::Scalar(Scalar::Pointer),
                Type::Aggregate(0),
                Type::Scalar(Scalar::Int),
            ]
        );
        assert_eq!(
            signature.aggregates,
            vec![Aggregate {
                fields: vec![Type::Scalar(Scalar::Double), Type::Scalar(Scalar::Double),],
            }]
        );
        assert_eq!(signature.arguments[3], Type::Scalar(Scalar::Int));
    }

    #[test]
    fn registers_nested_aggregates_post_order() {
        let signature = parse_signature(b"{{ll}{ll}}{l3i}").expect("valid signature");
        assert_eq!(signature.return_type, Type::Aggregate(2));
        assert_eq!(signature.arguments, vec![Type::Aggregate(3)]);
        assert_eq!(signature.aggregates.len(), 4);
        assert_eq!(
            signature.aggregates[2],
            Aggregate {
                fields: vec![Type::Aggregate(0), Type::Aggregate(1)]
            }
        );
        assert_eq!(
            signature.aggregates[3],
            Aggregate {
                fields: vec![
                    Type::Scalar(Scalar::Long),
                    Type::Array {
                        count: 3,
                        element: Scalar::Int
                    }
                ]
            }
        );
    }

    #[test]
    fn rejects_invalid_top_level_and_field_shapes() {
        for input in [b"".as_slice(), b" ", b"x", b"{", b"{}", b"{s}", b"{v}"] {
            assert!(parse_signature(input).is_err(), "accepted {input:?}");
        }
        assert!(matches!(
            parse_signature(b"iv"),
            Err(ParseError::VoidArgument { .. })
        ));
        assert!(matches!(
            parse_signature(b"{0i}"),
            Err(ParseError::ZeroLengthArray { .. })
        ));
        assert!(matches!(
            parse_signature(b"{3s}"),
            Err(ParseError::InvalidArrayElement { .. })
        ));
        assert!(matches!(
            parse_signature(b"{3}"),
            Err(ParseError::InvalidArrayElement { .. })
        ));
        assert!(matches!(
            parse_signature(b"{l"),
            Err(ParseError::UnclosedAggregate { .. })
        ));
    }

    #[test]
    fn rejects_top_level_arrays_and_count_overflow() {
        assert!(matches!(
            parse_signature(b"i3i"),
            Err(ParseError::InvalidType { .. })
        ));
        let huge = format!("{{{}i}}", "9".repeat(usize::BITS as usize + 2));
        assert!(matches!(
            parse_signature(huge.as_bytes()),
            Err(ParseError::ArrayCountOverflow { .. })
        ));
    }

    #[test]
    fn invalid_bytes_are_handled_without_utf8_assumptions() {
        assert!(matches!(
            parse_signature(&[b'i', 0xff]),
            Err(ParseError::InvalidType { byte: 0xff, .. })
        ));
    }
}

mod native;
