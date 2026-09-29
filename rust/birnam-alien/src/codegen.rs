//! C ABI marshalling for Alien-generated native modules. The private generated
//! helpers send to live owners and retain intermediate objects per invocation.

use core::fmt;

use crate::{Aggregate, Scalar, Signature, Type};

/// A source-generation failure that is safe to report before TCC is called.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodegenError {
    EmptyFunctionName,
    InvalidFunctionName { byte: u8, offset: usize },
    InvalidAggregateIndex { index: usize },
    TopLevelArray,
    UnsupportedAggregateShim { index: usize },
    InvalidAggregateField { aggregate: usize, field: usize },
    UnsupportedCallbackReturn,
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
            Self::UnsupportedAggregateShim { index } => {
                write!(
                    formatter,
                    "aggregate {index} requires aggregate marshalling"
                )
            }
            Self::InvalidAggregateField { aggregate, field } => {
                write!(
                    formatter,
                    "aggregate {aggregate} field {field} is not ABI-marshalable"
                )
            }
            Self::UnsupportedCallbackReturn => {
                formatter.write_str("a callback cannot return a string")
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

/// Emit a complete method-ABI shim for a scalar-only signature.
///
/// The first C argument is the Birnam receiver; remaining C arguments are read
/// from `args`. The shim checks the exact method arity before unboxing values,
/// and returns nil on a mismatch. Aggregate arguments and returns are rejected
/// until their Vector marshalling helpers land in a later slice.
pub fn emit_scalar_shim(
    function_name: &[u8],
    signature: &Signature,
) -> Result<String, CodegenError> {
    reject_aggregate_shim(signature)?;
    let mut source = runtime_preamble();
    source.push_str(&emit_prototype(function_name, signature)?);
    source.push_str("void* __shim(void* self, void** args, long n){\n");
    let expected = signature.arguments.len().saturating_sub(1);
    source.push_str("if (n != ");
    source.push_str(&expected.to_string());
    source.push_str("L) return alien_nil();\n");

    for (index, ty) in signature.arguments.iter().enumerate() {
        let Type::Scalar(scalar) = ty else {
            unreachable!("aggregate shim was rejected before emission");
        };
        let expression = if index == 0 {
            "self".to_owned()
        } else {
            format!("args[{}]", index - 1)
        };
        emit_pointer_guard(&mut source, *scalar, &expression, "alien_nil()");
        source.push_str(scalar_c_type(*scalar).expect("scalar has a C spelling"));
        source.push_str(" a");
        source.push_str(&index.to_string());
        source.push_str(" = ");
        emit_unbox(&mut source, *scalar, &expression);
        source.push_str(";\n");
    }

    let return_scalar = match &signature.return_type {
        Type::Scalar(scalar) => *scalar,
        Type::Aggregate(index) => {
            return Err(CodegenError::UnsupportedAggregateShim { index: *index });
        }
        Type::Array { .. } => return Err(CodegenError::TopLevelArray),
    };
    source.push_str("if (active->failure.status) return alien_nil();\n  ");
    if return_scalar == Scalar::Void {
        source.push_str("(void)");
    } else {
        source.push_str(scalar_c_type(return_scalar).expect("scalar has a C spelling"));
        source.push_str(" __r = ");
    }
    source.push_str(std::str::from_utf8(function_name).expect("C identifier is ASCII"));
    source.push('(');
    for (index, _) in signature.arguments.iter().enumerate() {
        if index != 0 {
            source.push_str(", ");
        }
        source.push('a');
        source.push_str(&index.to_string());
    }
    source.push_str(");\n");
    for (index, ty) in signature.arguments.iter().enumerate() {
        if *ty == Type::Scalar(Scalar::StringPointer) {
            source.push_str("alien_free_cstr(a");
            source.push_str(&index.to_string());
            source.push_str(");\n");
        }
    }
    emit_box_return(&mut source, return_scalar);
    source.push_str("}\n");
    Ok(source)
}

/// Emit shape-checked Vector-to-C and C-to-Vector helpers for every aggregate
/// in a signature. The eventual wrapper can return nil when an unmarshal shape
/// check fails, rather than corrupting a native stack frame.
pub fn emit_aggregate_support(signature: &Signature) -> Result<String, CodegenError> {
    validate_aggregate_fields(signature)?;
    let mut source = String::new();
    for (index, aggregate) in signature.aggregates.iter().enumerate() {
        source.push_str("static int __unm");
        source.push_str(&index.to_string());
        source.push_str("(void* v, __S");
        source.push_str(&index.to_string());
        source.push_str("* out){\n");
        source
            .push_str("if (alien_smallIntValue(alien_send(v, alien_intern(\"size\"), 0, 0L)) != ");
        source.push_str(&aggregate.fields.len().to_string());
        source.push_str("L) return 0;\nvoid* e;\n");
        for (field_index, field) in aggregate.fields.iter().enumerate() {
            source.push_str("e = alien_send(v, alien_intern(\"at:\"), (void*[]){alien_mkSmallInt(");
            source.push_str(&field_index.to_string());
            source.push_str(")}, 1L);\n");
            emit_unmarshal_field(&mut source, field, field_index)?;
        }
        source.push_str("return 1;\n}\n");
    }
    for (index, aggregate) in signature.aggregates.iter().enumerate() {
        source.push_str("static void* __mk");
        source.push_str(&index.to_string());
        source.push_str("(__S");
        source.push_str(&index.to_string());
        source.push_str("* v){\nvoid* r = alien_send(named(active,\"Vector\"), alien_intern(\"new\"), 0, 0L);\nvoid* e;\n");
        for (field_index, field) in aggregate.fields.iter().enumerate() {
            emit_marshal_field(&mut source, field, field_index)?;
            source.push_str("r = alien_send(r, alien_intern(\"add:\"), (void*[]){e}, 1L);\n");
        }
        source.push_str("return r;\n}\n");
    }
    Ok(source)
}

/// Emit the complete wrapper for scalar and aggregate signatures.
pub fn emit_shim(function_name: &[u8], signature: &Signature) -> Result<String, CodegenError> {
    if signature.aggregates.is_empty() {
        return emit_scalar_shim(function_name, signature);
    }
    validate_identifier(function_name)?;
    validate_aggregate_fields(signature)?;
    let mut source = runtime_preamble();

    source.push_str(&emit_prototype(function_name, signature)?);
    source.push_str(&emit_aggregate_support(signature)?);
    source.push_str("void* __shim(void* self, void** args, long n){\n");
    let expected = signature.arguments.len().saturating_sub(1);
    source.push_str("if (n != ");
    source.push_str(&expected.to_string());
    source.push_str("L) return alien_nil();\n");
    for (index, ty) in signature.arguments.iter().enumerate() {
        let expression = if index == 0 {
            "self".to_owned()
        } else {
            format!("args[{}]", index - 1)
        };
        match ty {
            Type::Scalar(scalar) => {
                emit_pointer_guard(&mut source, *scalar, &expression, "alien_nil()");
                source.push_str(scalar_c_type(*scalar).expect("scalar has a C spelling"));
                source.push_str(" a");
                source.push_str(&index.to_string());
                source.push_str(" = ");
                emit_unbox(&mut source, *scalar, &expression);
                source.push_str(";\n");
            }
            Type::Aggregate(aggregate) => {
                source.push_str("__S");
                source.push_str(&aggregate.to_string());
                source.push_str(" a");
                source.push_str(&index.to_string());
                source.push_str(";\nif (!__unm");
                source.push_str(&aggregate.to_string());
                source.push('(');
                source.push_str(&expression);
                source.push_str(", &a");
                source.push_str(&index.to_string());
                source.push_str(")) return alien_nil();\n");
            }
            Type::Array { .. } => return Err(CodegenError::TopLevelArray),
        }
    }

    source.push_str("if (active->failure.status) return alien_nil();\n");
    let return_type = match &signature.return_type {
        Type::Scalar(scalar) => {
            source.push_str("  ");
            if *scalar == Scalar::Void {
                source.push_str("(void)");
            } else {
                source.push_str(scalar_c_type(*scalar).expect("scalar has a C spelling"));
                source.push_str(" __r = ");
            }
            None
        }
        Type::Aggregate(index) => {
            source.push_str("  __S");
            source.push_str(&index.to_string());
            source.push_str(" __r = ");
            Some(*index)
        }
        Type::Array { .. } => return Err(CodegenError::TopLevelArray),
    };
    source.push_str(std::str::from_utf8(function_name).expect("C identifier is ASCII"));
    source.push('(');
    for (index, _) in signature.arguments.iter().enumerate() {
        if index != 0 {
            source.push_str(", ");
        }
        source.push('a');
        source.push_str(&index.to_string());
    }
    source.push_str(");\n");
    for (index, ty) in signature.arguments.iter().enumerate() {
        if *ty == Type::Scalar(Scalar::StringPointer) {
            source.push_str("alien_free_cstr(a");
            source.push_str(&index.to_string());
            source.push_str(");\n");
        }
    }
    if let Some(index) = return_type {
        source.push_str("return __mk");
        source.push_str(&index.to_string());
        source.push_str("(&__r);\n");
    } else if let Type::Scalar(scalar) = &signature.return_type {
        emit_box_return(&mut source, *scalar);
    }
    source.push_str("}\n");
    Ok(source)
}

/// Emit a complete C callback for a signature read from the C side.
///
/// The generated function boxes each native argument, sends `value` (or its
/// arity-matched keyword form) to the process-owned `__cbBlock`, then unboxes
/// the answer for the C caller.  The block address is patched by the live
/// runtime before relocation; no Rust pointer or lock is captured by this C
/// code.  String returns are rejected because there is no ownership contract
/// for a C caller to free them.
pub fn emit_callback(signature: &Signature) -> Result<String, CodegenError> {
    if matches!(signature.return_type, Type::Scalar(Scalar::StringPointer)) {
        return Err(CodegenError::UnsupportedCallbackReturn);
    }
    validate_aggregate_fields(signature)?;
    let mut source = runtime_preamble();

    if !signature.aggregates.is_empty() {}
    for (index, aggregate) in signature.aggregates.iter().enumerate() {
        emit_aggregate(&mut source, index, aggregate, signature.aggregates.len())?;
    }
    source.push_str(&emit_aggregate_support(signature)?);
    source.push('\n');
    emit_type(
        &mut source,
        &signature.return_type,
        signature.aggregates.len(),
    )?;
    source.push_str(" __cb(");
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
    source.push_str("){\n");
    for (index, argument) in signature.arguments.iter().enumerate() {
        source.push_str("void* b");
        source.push_str(&index.to_string());
        source.push_str(" = ");
        match argument {
            Type::Scalar(Scalar::StringPointer) => {
                source.push('a');
                source.push_str(&index.to_string());
                source.push_str(" ? alien_mkString(a");
                source.push_str(&index.to_string());
                source.push_str(", strlen(a");
                source.push_str(&index.to_string());
                source.push_str(")) : alien_nil()");
            }
            Type::Scalar(scalar) => {
                emit_box_expr(&mut source, *scalar, &format!("a{index}"));
            }
            Type::Aggregate(aggregate) => {
                source.push_str("__mk");
                source.push_str(&aggregate.to_string());
                source.push_str("(&a");
                source.push_str(&index.to_string());
                source.push(')');
            }
            Type::Array { .. } => return Err(CodegenError::TopLevelArray),
        }
        source.push_str(";\n");
    }
    source.push_str("void* r = alien_send((void*)__cbBlock, alien_intern(\"");
    if signature.arguments.is_empty() {
        source.push_str("value");
    } else {
        for _ in &signature.arguments {
            source.push_str("value:");
        }
    }
    source.push_str("\"), ");
    if signature.arguments.is_empty() {
        source.push_str("0, 0L);\n");
    } else {
        source.push_str("(void*[]){");
        for (index, _) in signature.arguments.iter().enumerate() {
            if index != 0 {
                source.push_str(", ");
            }
            source.push('b');
            source.push_str(&index.to_string());
        }
        source.push_str("}, ");
        source.push_str(&signature.arguments.len().to_string());
        source.push_str("L);\n");
    }
    match &signature.return_type {
        Type::Scalar(Scalar::Void) => source.push_str("(void)r;\nreturn;\n"),
        Type::Scalar(Scalar::Int) => source.push_str("return (int)alien_smallIntValue(r);\n"),
        Type::Scalar(Scalar::Long) => source.push_str("return alien_smallIntValue(r);\n"),
        Type::Scalar(Scalar::UnsignedLong) => source.push_str("return alien_uintValue(r);\n"),
        Type::Scalar(Scalar::Double) => source.push_str("return alien_floatValue(r);\n"),
        Type::Scalar(Scalar::Float) => source.push_str("return (float)alien_floatValue(r);\n"),
        Type::Scalar(Scalar::Pointer) => source.push_str(
            "return alien_isForeignAddress(r) ? (void*)alien_foreignAddressValue(r) : (void*)0;\n",
        ),
        Type::Scalar(Scalar::StringPointer) => unreachable!("rejected above"),
        Type::Aggregate(index) => {
            source.push_str("__S");
            source.push_str(&index.to_string());
            source.push_str(" __out = {0};\n__unm");
            source.push_str(&index.to_string());
            source.push_str("(r, &__out);\nreturn __out;\n");
        }
        Type::Array { .. } => return Err(CodegenError::TopLevelArray),
    }
    source.push_str("}\n");
    Ok(source)
}

fn validate_aggregate_fields(signature: &Signature) -> Result<(), CodegenError> {
    for (aggregate_index, aggregate) in signature.aggregates.iter().enumerate() {
        for (field_index, field) in aggregate.fields.iter().enumerate() {
            match field {
                Type::Scalar(Scalar::StringPointer | Scalar::Void)
                | Type::Array {
                    element: Scalar::StringPointer | Scalar::Void,
                    ..
                } => {
                    return Err(CodegenError::InvalidAggregateField {
                        aggregate: aggregate_index,
                        field: field_index,
                    });
                }
                Type::Aggregate(index) => {
                    validate_aggregate_index(*index, signature.aggregates.len())?
                }
                Type::Scalar(_) | Type::Array { .. } => {}
            }
        }
    }
    Ok(())
}

fn emit_unmarshal_field(
    source: &mut String,
    field: &Type,
    field_index: usize,
) -> Result<(), CodegenError> {
    match field {
        Type::Scalar(scalar) => {
            if *scalar == Scalar::Pointer {
                source.push_str("if (!alien_isForeignAddress(e)) return 0;\n");
            }
            source.push_str("out->f");
            source.push_str(&field_index.to_string());
            source.push_str(" = ");
            emit_unbox(source, *scalar, "e");
            source.push_str(";\n");
        }
        Type::Aggregate(index) => {
            source.push_str("if (!__unm");
            source.push_str(&index.to_string());
            source.push_str("(e, &out->f");
            source.push_str(&field_index.to_string());
            source.push_str(")) return 0;\n");
        }
        Type::Array { count, element } => {
            source.push_str(
                "if (alien_smallIntValue(alien_send(e, alien_intern(\"size\"), 0, 0L)) != ",
            );
            source.push_str(&count.to_string());
            source.push_str("L) return 0;\n{ long __k; for (__k = 0; __k < ");
            source.push_str(&count.to_string());
            source.push_str("L; __k++) {\nvoid* __ae = alien_send(e, alien_intern(\"at:\"), (void*[]){alien_mkSmallInt(__k)}, 1L);\n");
            if *element == Scalar::Pointer {
                source.push_str("if (!alien_isForeignAddress(__ae)) return 0;\n");
            }
            source.push_str("out->f");
            source.push_str(&field_index.to_string());
            source.push_str("[__k] = ");
            emit_unbox(source, *element, "__ae");
            source.push_str(";\n} }\n");
        }
    }
    Ok(())
}

fn emit_marshal_field(
    source: &mut String,
    field: &Type,
    field_index: usize,
) -> Result<(), CodegenError> {
    match field {
        Type::Scalar(scalar) => {
            source.push_str("e = ");
            emit_box_expr(source, *scalar, &format!("v->f{field_index}"));
            source.push_str(";\n");
        }
        Type::Aggregate(index) => {
            source.push_str("e = __mk");
            source.push_str(&index.to_string());
            source.push_str("(&v->f");
            source.push_str(&field_index.to_string());
            source.push_str(");\n");
        }
        Type::Array { count, element } => {
            source.push_str("{ void* s = alien_send(named(active,\"Vector\"), alien_intern(\"new\"), 0, 0L); long __k;\nfor (__k = 0; __k < ");
            source.push_str(&count.to_string());
            source.push_str("L; __k++) {\nvoid* __ae = ");
            emit_box_expr(source, *element, &format!("v->f{field_index}[__k]"));
            source.push_str(
                ";\ns = alien_send(s, alien_intern(\"add:\"), (void*[]){__ae}, 1L);\n} e = s; }\n",
            );
        }
    }
    Ok(())
}

fn reject_aggregate_shim(signature: &Signature) -> Result<(), CodegenError> {
    for ty in signature
        .arguments
        .iter()
        .chain(std::iter::once(&signature.return_type))
    {
        if let Type::Aggregate(index) = ty {
            return Err(CodegenError::UnsupportedAggregateShim { index: *index });
        }
        if matches!(ty, Type::Array { .. }) {
            return Err(CodegenError::TopLevelArray);
        }
    }
    Ok(())
}

fn runtime_preamble() -> String {
    include_str!("native_prelude.c").to_owned()
}

fn emit_pointer_guard(source: &mut String, scalar: Scalar, expression: &str, failure: &str) {
    if scalar == Scalar::Pointer {
        source.push_str("if (!alien_isForeignAddress(");
        source.push_str(expression);
        source.push_str(")) return ");
        source.push_str(failure);
        source.push_str(";\n");
    }
}

fn emit_unbox(source: &mut String, scalar: Scalar, expression: &str) {
    match scalar {
        Scalar::Int => {
            source.push_str("(int)alien_smallIntValue(");
            source.push_str(expression);
            source.push(')');
        }
        Scalar::Long => {
            source.push_str("alien_smallIntValue(");
            source.push_str(expression);
            source.push(')');
        }
        Scalar::UnsignedLong => {
            source.push_str("alien_uintValue(");
            source.push_str(expression);
            source.push(')');
        }
        Scalar::Double | Scalar::Float => {
            if scalar == Scalar::Float {
                source.push_str("(float)");
            }
            source.push_str("alien_floatValue(");
            source.push_str(expression);
            source.push(')');
        }
        Scalar::StringPointer => {
            source.push_str("alien_cstr(");
            source.push_str(expression);
            source.push(')');
        }
        Scalar::Pointer => {
            source.push_str("(void*)alien_foreignAddressValue(");
            source.push_str(expression);
            source.push(')');
        }
        Scalar::Void => unreachable!("void cannot be an argument"),
    }
}

fn emit_box_expr(source: &mut String, scalar: Scalar, expression: &str) {
    match scalar {
        Scalar::Int | Scalar::Long => {
            source.push_str("alien_mkSmallInt((long)(");
            source.push_str(expression);
            source.push_str("))")
        }
        Scalar::UnsignedLong => {
            source.push_str("(((unsigned long)(");
            source.push_str(expression);
            source.push_str(") >> 63) ? alien_mkWord((unsigned long)(");
            source.push_str(expression);
            source.push_str(")) : alien_mkSmallInt((long)(");
            source.push_str(expression);
            source.push_str(")))")
        }
        Scalar::Double | Scalar::Float => {
            source.push_str("alien_mkFloat((double)(");
            source.push_str(expression);
            source.push_str("))")
        }
        Scalar::Pointer => {
            source.push_str("alien_mkForeignAddress((unsigned long)(");
            source.push_str(expression);
            source.push_str("))")
        }
        Scalar::StringPointer | Scalar::Void => {
            unreachable!("aggregate validation rejects string and void fields")
        }
    }
}

fn emit_box_return(source: &mut String, scalar: Scalar) {
    match scalar {
        Scalar::Void => source.push_str("return alien_nil();\n"),
        Scalar::Int | Scalar::Long => source.push_str("return alien_mkSmallInt((long)__r);\n"),
        Scalar::UnsignedLong => source.push_str(
            "return (((unsigned long)__r >> 63) ? alien_mkWord((unsigned long)__r) : alien_mkSmallInt((long)__r));\n",
        ),
        Scalar::Double | Scalar::Float => {
            source.push_str("return alien_mkFloat((double)__r);\n")
        }
        Scalar::StringPointer => {
            source.push_str("return __r ? alien_mkString(__r, strlen(__r)) : alien_nil();\n")
        }
        Scalar::Pointer => {
            source.push_str("return alien_mkForeignAddress((unsigned long)__r);\n")
        }
    }
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
        let source = emit_prototype(b"alien_mix", &signature).unwrap();
        assert_eq!(
            source,
            "typedef struct { int f0; int f1; } __S0;\n".to_owned()
                + "typedef struct { long f0; int f1[3]; } __S1;\n"
                + "typedef struct { double f0; double f1; } __S2;\n"
                + "typedef struct { double f0; double f1; } __S3;\n"
                + "typedef struct { __S2 f0; __S3 f1; } __S4;\n"
                + "__S0 alien_mix(__S1 a0, __S4 a1);\n"
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

    #[test]
    fn emits_scalar_receiver_shim_with_arity_and_string_cleanup() {
        let signature = parse_signature(b"l s").unwrap();
        let source = emit_scalar_shim(b"strlen", &signature).unwrap();
        assert!(source.contains("if (n != 0L) return alien_nil();"));
        assert!(source.contains("const char* a0 = alien_cstr(self);"));
        assert!(source.contains("long __r = strlen(a0);"));
        assert!(source.contains("alien_free_cstr(a0);"));
        assert!(source.contains("return alien_mkSmallInt((long)__r);"));
    }

    #[test]
    fn pointer_signatures_guard_foreign_address_membership_before_unboxing() {
        let scalar = emit_scalar_shim(b"free", &parse_signature(b"v p").unwrap()).unwrap();
        assert!(
            scalar.contains(
                "if (!alien_isForeignAddress(self)) return alien_nil();\nvoid* a0 = (void*)alien_foreignAddressValue(self);"
            )
        );

        let aggregate = emit_aggregate_support(&parse_signature(b"{p2p}").unwrap()).unwrap();
        assert!(aggregate.contains("if (!alien_isForeignAddress(e)) return 0;"));
        assert!(aggregate.contains("if (!alien_isForeignAddress(__ae)) return 0;"));

        let callback = emit_callback(&parse_signature(b"p").unwrap()).unwrap();
        assert!(callback.contains(
            "return alien_isForeignAddress(r) ? (void*)alien_foreignAddressValue(r) : (void*)0;"
        ));
    }

    #[test]
    fn rejects_aggregate_shims_until_vector_marshalling_exists() {
        let signature = parse_signature(b"{ii} i i").unwrap();
        assert_eq!(
            emit_scalar_shim(b"alien_div", &signature),
            Err(CodegenError::UnsupportedAggregateShim { index: 0 })
        );
    }

    #[test]
    fn emits_shape_checked_nested_and_array_vector_helpers() {
        let signature = parse_signature(b"{l3i} {{dd}{dd}}").unwrap();
        let source = emit_aggregate_support(&signature).unwrap();
        assert!(source.contains("static int __unm0(void* v, __S0* out)"));
        assert!(source.contains("out->f1[__k] = (int)alien_smallIntValue(__ae);"));
        assert!(source.contains("static void* __mk0(__S0* v)"));
        assert!(source.contains("static int __unm2(void* v, __S2* out)"));
        assert!(source.contains("static int __unm3(void* v, __S3* out)"));
        assert!(source.contains("if (!__unm1(e, &out->f0)) return 0;"));
        assert!(source.contains("e = __mk2(&v->f1);"));
    }

    #[test]
    fn rejects_string_and_void_aggregate_fields_before_emission() {
        let malformed = Signature {
            return_type: Type::Aggregate(0),
            arguments: Vec::new(),
            aggregates: vec![Aggregate {
                fields: vec![Type::Scalar(Scalar::StringPointer)],
            }],
        };
        assert_eq!(
            emit_aggregate_support(&malformed),
            Err(CodegenError::InvalidAggregateField {
                aggregate: 0,
                field: 0
            })
        );
    }

    #[test]
    fn composes_aggregate_helpers_into_receiver_first_wrapper() {
        let signature = parse_signature(b"{ii} {ii} i").unwrap();
        let source = emit_shim(b"alien_div", &signature).unwrap();
        assert!(source.contains("if (n != 1L) return alien_nil();"));
        assert!(source.contains("if (!__unm1(self, &a0)) return alien_nil();"));
        assert!(source.contains("__S0 __r = alien_div(a0, a1);"));
        assert!(source.contains("return __mk0(&__r);"));
    }

    #[test]
    fn emits_scalar_callback_dispatch_and_rejects_string_returns() {
        let signature = parse_signature(b"l l").unwrap();
        let source = emit_callback(&signature).unwrap();
        assert!(source.contains("static O __cbBlock;"));
        assert!(source.contains("long __cb(long a0)"));
        assert!(source.contains("alien_send((void*)__cbBlock, alien_intern(\"value:\")"));
        assert!(source.contains("return alien_smallIntValue(r);"));
        let string_return = parse_signature(b"s l").unwrap();
        assert_eq!(
            emit_callback(&string_return),
            Err(CodegenError::UnsupportedCallbackReturn)
        );
    }

    #[test]
    fn emits_aggregate_callback_marshalling_in_both_directions() {
        let signature = parse_signature(b"{ll} l").unwrap();
        let source = emit_callback(&signature).unwrap();
        assert!(source.contains("named(active,\"Vector\")"));
        assert!(source.contains("static void* __mk0(__S0* v)"));
        assert!(source.contains("void* b0 = alien_mkSmallInt((long)(a0));"));
        assert!(source.contains("__S0 __out = {0};"));
        assert!(source.contains("__unm0(r, &__out);"));
    }
}

/// Assemble a current native module around the existing C ABI marshalling.
/// Foreign addresses belong to this process; these units are not image artifacts.
pub fn emit_native(
    function: Option<(&[u8], usize)>,
    signature: &Signature,
) -> Result<String, CodegenError> {
    let callback = function.is_none();
    let mut source = if let Some((name, address)) = function {
        validate_identifier(name)?;
        let name = b"__alien_target";
        let source = emit_shim(name, signature)?;
        let declaration = emit_prototype(name, signature)?;
        let name = std::str::from_utf8(name).unwrap();
        // Keep aggregate declarations and replace only the foreign function prototype.
        let lines = declaration.lines().collect::<Vec<_>>();
        let prototype = lines.last().unwrap();
        let pointer = prototype.replace(&format!(" {name}("), &format!(" (*{name})("));
        let pointer = format!(
            "static {} = (void*){address}UL;",
            pointer.trim_end_matches(';')
        );
        source.replace(prototype, &pointer)
    } else {
        emit_callback(signature)?
    };
    if callback {
        // Split the raw C entry from its rooted, reentrant invocation scope.
        source = source.replace(" __cb(", " __cb_body(");
        let mut declaration = emit_prototype(b"__cb", signature)?;
        declaration = declaration
            .lines()
            .last()
            .unwrap()
            .trim_end_matches(';')
            .to_owned();
        source.push_str(&declaration);
        source.push_str(
            "{ Call c={0}; Call *prior=active; active=&c; begin(&c); keep(&c,__cbBlock); ",
        );
        let args = (0..signature.arguments.len())
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        if signature.return_type == Type::Scalar(Scalar::Void) {
            source.push_str(&format!("__cb_body({args}); active=prior; R r=finish(&c,(A){{0}}); if(r.status) abort(); return; }}\n"));
        } else {
            emit_type(
                &mut source,
                &signature.return_type,
                signature.aggregates.len(),
            )?;
            source.push_str(&format!(" result=__cb_body({args}); active=prior; R r=finish(&c,(A){{0}}); if(r.status) abort(); return result; }}\n"));
        }
    } else {
        source.push_str(r#"
static R invoke(void*unused,O receiver,const A*a,size_t n){
 Call c={0}; Call*prior=active;active=&c;begin(&c);keep(&c,receiver);
 O *args=calloc(n?n:1,sizeof(O));if(!args)c.failure=bad();
 for(size_t i=0;i<n&&!c.failure.status;i++)args[i]=keep(&c,object(&c,a[i]));
 O result=c.failure.status?0:__shim(receiver,args,n);free(args);active=prior;return finish(&c,obj(result));
}
"#);
    }
    source.push_str(r#"
static R initialize(const P*p,const A*a,size_t n){
 if(!p||p->version!=1||p->byte_size!=sizeof(P)||n<2||a[0].kind!=1||a[1].kind!=1)return bad();
 id=*p;ns=a[0].data.object;owner=a[1].data.object;Call c={0};Call*prior=active;active=&c;
 A key=obj(intern(&c,"Roots"));R rr=raw(&c,ns,"at:",&key,1);if(rr.status||rr.value.kind!=1){active=prior;return bad();}roots=rr.value.data.object;
 keep(&c,owner);
"#);
    if callback {
        source.push_str("if(n!=3){active=prior;return finish(&c,(A){0});}__cbBlock=keep(&c,object(&c,a[2]));A address=word((size_t)__cb);O result=call(&c,named(&c,\"Alien\"),\"fromAddress:\",&address,1);\n");
    } else {
        source.push_str("A args[2]={entry(invoke),obj(owner)};O result=call(&c,named(&c,\"Method\"),\"fromNative:owner:\",args,2);\n");
    }
    source.push_str("active=prior;return finish(&c,obj(result));}\nconst Module birnam_native_module_v1={1,sizeof(Module),initialize};\n");
    Ok(source)
}
