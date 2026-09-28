// Iterative tree encoding and decoding boundaries; the PortableProgram codec is unchanged.
use super::*;
use std::collections::HashSet;

enum OwnedNode {
    Integer(i64),
    FloatBits(u64),
    String(Vec<u8>),
    Symbol(Vec<u8>),
    Nil,
    True,
    False,
    LexicalRead(u32),
    GlobalRead(Vec<u8>),
    Send {
        selector: Vec<u8>,
        operands: Vec<u32>,
    },
    Block {
        program: Vec<u8>,
        captures: Vec<u32>,
    },
}

impl OwnedNode {
    fn as_spec(&self) -> birnam_artifact::portable_program::NodeSpec<'_> {
        use birnam_artifact::portable_program::NodeSpec;
        match self {
            Self::Integer(value) => NodeSpec::Integer(*value),
            Self::FloatBits(bits) => NodeSpec::FloatBits(*bits),
            Self::String(bytes) => NodeSpec::String(bytes),
            Self::Symbol(bytes) => NodeSpec::Symbol(bytes),
            Self::Nil => NodeSpec::Nil,
            Self::True => NodeSpec::True,
            Self::False => NodeSpec::False,
            Self::LexicalRead(index) => NodeSpec::LexicalRead(*index),
            Self::GlobalRead(bytes) => NodeSpec::GlobalRead(bytes),
            Self::Send { selector, operands } => NodeSpec::Send { selector, operands },
            Self::Block { program, captures } => NodeSpec::BlockLiteral { program, captures },
        }
    }
}

enum NodeWork {
    Enter {
        node: Oop,
        depth: usize,
    },
    FinishSend {
        address: usize,
        selector: Vec<u8>,
        operand_count: usize,
    },
}

fn append_node_tree(state: &State, root: Oop, nodes: &mut Vec<OwnedNode>) -> Result<u32, Failure> {
    let mut work = vec![NodeWork::Enter {
        node: root,
        depth: 1,
    }];
    let mut results = Vec::<u32>::new();
    let mut active = HashSet::<usize>::new();
    while let Some(item) = work.pop() {
        match item {
            NodeWork::Enter { node, depth } => {
                if depth > 4096 {
                    return Err(Failure::PortableProgramLimit);
                }
                let value = state.node(node).ok_or(Failure::TypeMismatch)?;
                let owned = match &*value {
                    NodeValue::Integer(value) => Some(OwnedNode::Integer(*value)),
                    NodeValue::FloatBits(bits) => {
                        Some(OwnedNode::FloatBits(u64::from_le_bytes(*bits)))
                    }
                    NodeValue::String(bytes) => Some(OwnedNode::String(bytes.clone())),
                    NodeValue::Symbol(bytes) => Some(OwnedNode::Symbol(bytes.clone())),
                    NodeValue::Singleton(Singleton::Nil) => Some(OwnedNode::Nil),
                    NodeValue::Singleton(Singleton::True) => Some(OwnedNode::True),
                    NodeValue::Singleton(Singleton::False) => Some(OwnedNode::False),
                    NodeValue::LexicalRead(index) => Some(OwnedNode::LexicalRead(*index)),
                    NodeValue::GlobalRead(bytes) => Some(OwnedNode::GlobalRead(bytes.clone())),
                    NodeValue::Send { selector, operands } => {
                        if !active.insert(node.addr()) {
                            return Err(Failure::InvalidPortableProgram);
                        }
                        work.push(NodeWork::FinishSend {
                            address: node.addr(),
                            selector: selector.clone(),
                            operand_count: operands.len(),
                        });
                        for operand in operands.iter().rev() {
                            work.push(NodeWork::Enter {
                                node: *operand,
                                depth: depth + 1,
                            });
                        }
                        None
                    }
                    NodeValue::Block { program, captures } => {
                        let nested = state
                            .program(*program)
                            .ok_or(Failure::InvalidPortableProgram)?;
                        Some(OwnedNode::Block {
                            program: nested.to_vec(),
                            captures: captures.clone(),
                        })
                    }
                };
                if let Some(owned) = owned {
                    let index =
                        u32::try_from(nodes.len()).map_err(|_| Failure::PortableProgramLimit)?;
                    nodes.push(owned);
                    results.push(index);
                }
            }
            NodeWork::FinishSend {
                address,
                selector,
                operand_count,
            } => {
                if !active.remove(&address) || results.len() < operand_count {
                    return Err(Failure::InvalidPortableProgram);
                }
                let operands = results.split_off(results.len() - operand_count);
                let index =
                    u32::try_from(nodes.len()).map_err(|_| Failure::PortableProgramLimit)?;
                nodes.push(OwnedNode::Send { selector, operands });
                results.push(index);
            }
        }
    }
    if results.len() == 1 {
        results.pop().ok_or(Failure::InvalidPortableProgram)
    } else {
        Err(Failure::InvalidPortableProgram)
    }
}

pub(super) fn encode_program(
    state: &State,
    parameters: &[Vec<u8>],
    captures: &[Vec<u8>],
    statement_values: &[Oop],
) -> Result<Vec<u8>, Failure> {
    use birnam_artifact::portable_program::{LetSpec, ProgramSpec, StatementSpec};

    let mut owned_lets = Vec::<(Vec<u8>, u32)>::new();
    let mut statements = Vec::<StatementSpec>::with_capacity(statement_values.len());
    let mut nodes = Vec::<OwnedNode>::new();
    for (statement_index, value) in statement_values.iter().enumerate() {
        let statement = state.statement(*value).ok_or(Failure::TypeMismatch)?;
        let (expression, let_name) = match &*statement {
            StatementValue::Expression(expression) => (*expression, None),
            StatementValue::Let { name, expression } => (*expression, Some(name.clone())),
        };
        let root_node = append_node_tree(state, expression, &mut nodes)?;
        if let Some(name) = let_name {
            let let_index =
                u32::try_from(owned_lets.len()).map_err(|_| Failure::PortableProgramLimit)?;
            owned_lets.push((
                name,
                u32::try_from(statement_index).map_err(|_| Failure::PortableProgramLimit)?,
            ));
            statements.push(StatementSpec::Let {
                root_node,
                let_index,
            });
        } else {
            statements.push(StatementSpec::Expression { root_node });
        }
    }
    let parameter_refs = parameters.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let capture_refs = captures.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let lets = owned_lets
        .iter()
        .map(|(name, defining_statement)| LetSpec {
            name,
            defining_statement: *defining_statement,
        })
        .collect::<Vec<_>>();
    let node_specs = nodes.iter().map(OwnedNode::as_spec).collect::<Vec<_>>();
    birnam_artifact::portable_program::encode(ProgramSpec {
        parameters: &parameter_refs,
        captures: &capture_refs,
        lets: &lets,
        statements: &statements,
        nodes: &node_specs,
    })
    .map_err(|error| match error {
        birnam_artifact::portable_program::Error::Quantity => Failure::PortableProgramLimit,
        birnam_artifact::portable_program::Error::Format
        | birnam_artifact::portable_program::Error::Name => Failure::InvalidPortableProgram,
    })
}

pub(super) fn decoded_subtree_start(
    program: &birnam_artifact::portable_program::Program<'_>,
    root: usize,
) -> Option<usize> {
    let mut needed = 1usize;
    let mut index = root;
    loop {
        let node = program.node(index)?;
        needed = needed.checked_sub(1)?;
        if node.kind == 10 {
            needed = needed.checked_add(node.operand_count as usize)?;
        }
        if needed == 0 {
            return Some(index);
        }
        index = index.checked_sub(1)?;
    }
}

#[cfg(test)]
mod byte_rule_contract_tests {
    #[test]
    fn unchecked_arity_preserves_all_byte_classes() {
        for (name, expected) in [
            (&b""[..], 0),
            (&b"_private"[..], 0),
            (&b"value"[..], 0),
            (&b"at:put:"[..], 2),
            (&b"+"[..], 1),
            (&b"\xff"[..], 1),
            (&b"\0"[..], 1),
            (&b"_x:\0:"[..], 2),
        ] {
            assert_eq!(birnam_artifact::selector::arity(name), expected, "{name:?}");
        }
    }
}
