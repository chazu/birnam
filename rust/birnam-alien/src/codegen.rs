//! Deterministic C declarations for the Alien TCC boundary.
//!
//! This first generated-C slice emits only aggregate typedefs and a function
//! prototype. It has no runtime pointers or IdApi calls, so source stability
//! and C-identifier validation can be tested before wrapper bodies introduce
//! foreign callbacks or object authority.

use core::fmt;

use crate::{Aggregate, Scalar, Signature, Type};

/// A source-generation failure that is safe to report before TCC is called.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodegenError {
    EmptyFunctionName,
    InvalidFunctionName { byte: u8, offset: usize },
    InvalidAggregateIndex { index: usize },
    TopLevelArray,
}

impl fmt::Display for CodegenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyFunctionName => formatter.write_str("function name is empty"),
            Self::InvalidFunctionName { byte, offset } => {
                write!(
                    formatter,
                    "invalid C identifier byte {byte:#x} at offset {offset}"
                )
            }
            Self::InvalidAggregateIndex { index } => {
                write!(formatter, "aggregate index {index} is not defined")
            }
            Self::TopLevelArray => {
                formatter.write_str("top-level arrays are not legal C ABI values")
            }
        }
    }
}

impl std::error::Error for CodegenError {}

/// Emit post-order aggregate typedefs followed by one C function prototype.
pub fn emit_prototype(function_name: &[u8], signature: &Signature) -> Result<String, CodegenError> {
    validate_identifier(function_name)?;
    let mut source = String::new();
    for (index, aggregate) in signature.aggregates.iter().enumerate() {
        emit_aggregate(&mut source, index, aggregate, signature.aggregates.len())?;
    }
    emit_type(
        &mut source,
        &signature.return_type,
        signature.aggregates.len(),
    )?;
    source.push(' ');
    source.push_str(std::str::from_utf8(function_name).expect("C identifier is ASCII"));
    source.push('(');
    if signature.arguments.is_empty() {
        source.push_str("void");
    } else {
        for (index, argument) in signature.arguments.iter().enumerate() {
            if index != 0 {
                source.push_str(", ");
            }
            emit_type(&mut source, argument, signature.aggregates.len())?;
            source.push_str(" a");
            source.push_str(&index.to_string());
        }
    }
    source.push_str(");\n");
    Ok(source)
}

fn validate_identifier(name: &[u8]) -> Result<(), CodegenError> {
    let Some(&first) = name.first() else {
        return Err(CodegenError::EmptyFunctionName);
    };
    if !(first == b'_' || first.is_ascii_alphabetic()) {
        return Err(CodegenError::InvalidFunctionName {
            byte: first,
            offset: 0,
        });
    }
    for (offset, &byte) in name.iter().enumerate().skip(1) {
        if !(byte == b'_' || byte.is_ascii_alphanumeric()) {
            return Err(CodegenError::InvalidFunctionName { byte, offset });
        }
    }
    Ok(())
}

fn emit_aggregate(
    source: &mut String,
    index: usize,
    aggregate: &Aggregate,
    aggregate_count: usize,
) -> Result<(), CodegenError> {
    source.push_str("typedef struct { ");
    for (field_index, field) in aggregate.fields.iter().enumerate() {
        if let Type::Array { count, element } = field {
            source.push_str(scalar_c_type(*element).ok_or(CodegenError::TopLevelArray)?);
            source.push_str(" f");
            source.push_str(&field_index.to_string());
            source.push('[');
            source.push_str(&count.to_string());
            source.push_str("]; ");
        } else {
            emit_field_type(source, field, aggregate_count)?;
            source.push_str(" f");
            source.push_str(&field_index.to_string());
            source.push_str("; ");
        }
    }
    source.push_str("} __S");
    source.push_str(&index.to_string());
    source.push_str(";\n");
    Ok(())
}

fn emit_field_type(
    source: &mut String,
    ty: &Type,
    aggregate_count: usize,
) -> Result<(), CodegenError> {
    match ty {
        Type::Scalar(scalar) => {
            source.push_str(scalar_c_type(*scalar).ok_or(CodegenError::TopLevelArray)?)
        }
        Type::Array { count, element } => {
            source.push_str(scalar_c_type(*element).ok_or(CodegenError::TopLevelArray)?);
            source.push('[');
            source.push_str(&count.to_string());
            source.push(']');
        }
        Type::Aggregate(index) => {
            validate_aggregate_index(*index, aggregate_count)?;
            source.push_str("__S");
            source.push_str(&index.to_string());
        }
    }
    Ok(())
}

fn emit_type(source: &mut String, ty: &Type, aggregate_count: usize) -> Result<(), CodegenError> {
    match ty {
        Type::Scalar(scalar) => {
            source.push_str(scalar_c_type(*scalar).ok_or(CodegenError::TopLevelArray)?)
        }
        Type::Aggregate(index) => {
            validate_aggregate_index(*index, aggregate_count)?;
            source.push_str("__S");
            source.push_str(&index.to_string());
        }
        Type::Array { .. } => return Err(CodegenError::TopLevelArray),
    }
    Ok(())
}

fn validate_aggregate_index(index: usize, aggregate_count: usize) -> Result<(), CodegenError> {
    (index < aggregate_count)
        .then_some(())
        .ok_or(CodegenError::InvalidAggregateIndex { index })
}

fn scalar_c_type(scalar: Scalar) -> Option<&'static str> {
    Some(match scalar {
        Scalar::Int => "int",
        Scalar::Long => "long",
        Scalar::UnsignedLong => "unsigned long",
        Scalar::Double => "double",
        Scalar::Float => "float",
        Scalar::StringPointer => "const char*",
        Scalar::Pointer => "void*",
        Scalar::Void => "void",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_signature;

    #[test]
    fn emits_nested_aggregate_prototypes_in_post_order() {
        let signature = parse_signature(b"{ii} {l3i} {{dd}{dd}}").unwrap();
        let source = emit_prototype(b"birnam_mix", &signature).unwrap();
        assert_eq!(
            source,
            "typedef struct { int f0; int f1; } __S0;\n".to_owned()
                + "typedef struct { long f0; int f1[3]; } __S1;\n"
                + "typedef struct { double f0; double f1; } __S2;\n"
                + "typedef struct { double f0; double f1; } __S3;\n"
                + "typedef struct { __S2 f0; __S3 f1; } __S4;\n"
                + "__S0 birnam_mix(__S1 a0, __S4 a1);\n"
        );
    }

    #[test]
    fn rejects_identifier_injection_and_malformed_top_level_shapes() {
        let signature = parse_signature(b"i i").unwrap();
        assert_eq!(
            emit_prototype(b"", &signature),
            Err(CodegenError::EmptyFunctionName)
        );
        assert_eq!(
            emit_prototype(b"bad-name", &signature),
            Err(CodegenError::InvalidFunctionName {
                byte: b'-',
                offset: 3
            })
        );
        let malformed = Signature {
            return_type: Type::Array {
                count: 2,
                element: Scalar::Int,
            },
            arguments: Vec::new(),
            aggregates: Vec::new(),
        };
        assert_eq!(
            emit_prototype(b"f", &malformed),
            Err(CodegenError::TopLevelArray)
        );
    }
}
