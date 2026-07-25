//! Deterministic ABI metadata derived from the frozen Alien signature tree.
//!
//! This module intentionally does not calculate C layout or hidden sret
//! parameters. The existing TCC compiler remains the platform ABI authority for
//! aggregate size, alignment, HFA lowering, and return convention. Rust only
//! records the stable scalar classes and the homogeneous-float metadata needed
//! by the later shim emitter.

use crate::{Aggregate, Scalar, Signature, Type};

/// Scalar ABI class used by generated C shims.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScalarAbi {
    SignedInteger32,
    SignedInteger64,
    UnsignedInteger64,
    Float32,
    Float64,
    StringPointer,
    Pointer,
    Void,
}

impl ScalarAbi {
    fn from_scalar(scalar: Scalar) -> Self {
        match scalar {
            Scalar::Int => Self::SignedInteger32,
            Scalar::Long => Self::SignedInteger64,
            Scalar::UnsignedLong => Self::UnsignedInteger64,
            Scalar::Double => Self::Float64,
            Scalar::Float => Self::Float32,
            Scalar::StringPointer => Self::StringPointer,
            Scalar::Pointer => Self::Pointer,
            Scalar::Void => Self::Void,
        }
    }
}

/// Homogeneous floating-point aggregate metadata. AAPCS64 supports up to four
/// members; the emitter still lets TCC make the final platform decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Hfa {
    Float { members: usize },
    Double { members: usize },
}

/// ABI class for one top-level signature position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbiClass {
    Scalar(ScalarAbi),
    Aggregate { index: usize, hfa: Option<Hfa> },
}

/// Classification for a complete parsed signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AbiClassification {
    pub return_class: AbiClass,
    pub arguments: Vec<AbiClass>,
}

/// Classify the frozen signature without introducing an independent C layout
/// or calling convention implementation.
pub fn classify_signature(signature: &Signature) -> AbiClassification {
    AbiClassification {
        return_class: classify_type(&signature.return_type, &signature.aggregates),
        arguments: signature
            .arguments
            .iter()
            .map(|argument| classify_type(argument, &signature.aggregates))
            .collect(),
    }
}

fn classify_type(ty: &Type, aggregates: &[Aggregate]) -> AbiClass {
    match ty {
        Type::Scalar(scalar) => AbiClass::Scalar(ScalarAbi::from_scalar(*scalar)),
        Type::Array { .. } => {
            // The parser only admits arrays inside aggregates. Keeping this
            // branch total makes the classifier robust if a future parser
            // constructs a malformed tree in a test.
            AbiClass::Aggregate {
                index: usize::MAX,
                hfa: None,
            }
        }
        Type::Aggregate(index) => AbiClass::Aggregate {
            index: *index,
            hfa: classify_hfa(*index, aggregates),
        },
    }
}

fn classify_hfa(index: usize, aggregates: &[Aggregate]) -> Option<Hfa> {
    let aggregate = aggregates.get(index)?;
    let mut kind = None;
    let mut members = 0usize;
    for field in &aggregate.fields {
        if !collect_float_members(field, aggregates, &mut kind, &mut members) {
            return None;
        }
    }
    if (1..=4).contains(&members) {
        match kind? {
            FloatKind::Float => Some(Hfa::Float { members }),
            FloatKind::Double => Some(Hfa::Double { members }),
        }
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FloatKind {
    Float,
    Double,
}

fn collect_float_members(
    ty: &Type,
    aggregates: &[Aggregate],
    kind: &mut Option<FloatKind>,
    members: &mut usize,
) -> bool {
    match ty {
        Type::Scalar(Scalar::Float) => {
            if !matches!(kind, None | Some(FloatKind::Float)) {
                return false;
            }
            *kind = Some(FloatKind::Float);
            let Some(next) = members.checked_add(1) else {
                return false;
            };
            *members = next;
            true
        }
        Type::Scalar(Scalar::Double) => {
            if !matches!(kind, None | Some(FloatKind::Double)) {
                return false;
            }
            *kind = Some(FloatKind::Double);
            let Some(next) = members.checked_add(1) else {
                return false;
            };
            *members = next;
            true
        }
        Type::Aggregate(index) => {
            let Some(nested) = classify_hfa(*index, aggregates) else {
                return false;
            };
            let (nested_kind, nested_members) = match nested {
                Hfa::Float { members } => (FloatKind::Float, members),
                Hfa::Double { members } => (FloatKind::Double, members),
            };
            if let Some(existing) = *kind
                && existing != nested_kind
            {
                return false;
            }
            let Some(next) = members.checked_add(nested_members) else {
                return false;
            };
            *kind = Some(nested_kind);
            *members = next;
            true
        }
        Type::Array { element, count } => {
            let scalar = match element {
                Scalar::Float => FloatKind::Float,
                Scalar::Double => FloatKind::Double,
                _ => return false,
            };
            if let Some(existing) = *kind
                && existing != scalar
            {
                return false;
            }
            *kind = Some(scalar);
            let Some(next) = members.checked_add(*count) else {
                return false;
            };
            *members = next;
            true
        }
        Type::Scalar(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_signature;

    #[test]
    fn classifies_frozen_scalars() {
        let signature = parse_signature(b"v i l u d f s p").unwrap();
        let classes = classify_signature(&signature);
        assert_eq!(classes.return_class, AbiClass::Scalar(ScalarAbi::Void));
        assert_eq!(
            classes.arguments,
            vec![
                AbiClass::Scalar(ScalarAbi::SignedInteger32),
                AbiClass::Scalar(ScalarAbi::SignedInteger64),
                AbiClass::Scalar(ScalarAbi::UnsignedInteger64),
                AbiClass::Scalar(ScalarAbi::Float64),
                AbiClass::Scalar(ScalarAbi::Float32),
                AbiClass::Scalar(ScalarAbi::StringPointer),
                AbiClass::Scalar(ScalarAbi::Pointer),
            ]
        );
    }

    #[test]
    fn identifies_hfa_and_general_aggregates_without_claiming_layout() {
        let signature = parse_signature(b"{ff} {dd} {fd} {2f} {l3i}").unwrap();
        let classes = classify_signature(&signature);
        assert_eq!(
            classes.return_class,
            AbiClass::Aggregate {
                index: 0,
                hfa: Some(Hfa::Float { members: 2 })
            }
        );
        assert_eq!(
            classes.arguments[0],
            AbiClass::Aggregate {
                index: 1,
                hfa: Some(Hfa::Double { members: 2 })
            }
        );
        assert_eq!(
            classes.arguments[1],
            AbiClass::Aggregate {
                index: 2,
                hfa: None
            }
        );
        assert_eq!(
            classes.arguments[2],
            AbiClass::Aggregate {
                index: 3,
                hfa: Some(Hfa::Float { members: 2 })
            }
        );
        assert_eq!(
            classes.arguments[3],
            AbiClass::Aggregate {
                index: 4,
                hfa: None
            }
        );

        let nested = parse_signature(b"{{ff}{ff}}").unwrap();
        assert_eq!(
            classify_signature(&nested).return_class,
            AbiClass::Aggregate {
                index: 2,
                hfa: Some(Hfa::Float { members: 4 })
            }
        );
    }
}
