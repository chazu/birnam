//! Canonical PortableProgram lowering from the compiler's semantic Block AST.
//!
//! This is a pure compiler transform. It has no runtime, Oop, IdApi, native
//! layout, or source-annotation authority.

use birnam_artifact::portable_program::{LetSpec, NodeSpec, ProgramSpec, StatementSpec, encode};

use crate::{Block, Expr, Statement, resumable::lexical_capture_names};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidLiteral,
    InvalidLexicalScope,
    DepthLimit,
    InvalidProgram,
    ResourceLimit,
}

const MAX_DEPTH: usize = 4096;

/// Validate canonical bytes and return the Block signature required by
/// materialization orchestration without exposing the artifact decoder type.
pub fn program_signature(bytes: &[u8]) -> Result<(u32, u32), Error> {
    let program =
        birnam_artifact::portable_program::inspect(bytes).map_err(|_| Error::InvalidProgram)?;
    Ok((program.parameter_count, program.capture_count))
}

#[derive(Debug)]
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
    BlockLiteral {
        program: Vec<u8>,
        captures: Vec<u32>,
    },
}

impl OwnedNode {
    fn as_spec(&self) -> NodeSpec<'_> {
        match self {
            Self::Integer(value) => NodeSpec::Integer(*value),
            Self::FloatBits(bits) => NodeSpec::FloatBits(*bits),
            Self::String(bytes) => NodeSpec::String(bytes),
            Self::Symbol(name) => NodeSpec::Symbol(name),
            Self::Nil => NodeSpec::Nil,
            Self::True => NodeSpec::True,
            Self::False => NodeSpec::False,
            Self::LexicalRead(index) => NodeSpec::LexicalRead(*index),
            Self::GlobalRead(name) => NodeSpec::GlobalRead(name),
            Self::Send { selector, operands } => NodeSpec::Send { selector, operands },
            Self::BlockLiteral { program, captures } => {
                NodeSpec::BlockLiteral { program, captures }
            }
        }
    }
}

struct Builder {
    parameters: Vec<Vec<u8>>,
    captures: Vec<Vec<u8>>,
    lets: Vec<(Vec<u8>, u32)>,
    statements: Vec<StatementSpec>,
    nodes: Vec<OwnedNode>,
    bindings: Vec<Vec<u8>>,
}

impl Builder {
    fn new(block: &Block<'_>, outer_bindings: &[Vec<u8>]) -> Self {
        let parameters = block
            .params
            .iter()
            .map(|name| name.to_vec())
            .collect::<Vec<_>>();
        let captures = lexical_capture_names(block, outer_bindings);
        let bindings = parameters.iter().chain(captures.iter()).cloned().collect();
        Self {
            parameters,
            captures,
            lets: Vec::new(),
            statements: Vec::new(),
            nodes: Vec::new(),
            bindings,
        }
    }

    fn node(&mut self, node: OwnedNode) -> Result<u32, Error> {
        let index = u32::try_from(self.nodes.len()).map_err(|_| Error::ResourceLimit)?;
        self.nodes.push(node);
        Ok(index)
    }

    fn binding_index(&self, name: &[u8]) -> Option<u32> {
        self.bindings
            .iter()
            .rposition(|binding| binding.as_slice() == name)
            .and_then(|index| u32::try_from(index).ok())
    }

    fn expression(&mut self, expression: &Expr<'_>, depth: usize) -> Result<u32, Error> {
        if depth >= MAX_DEPTH {
            return Err(Error::DepthLimit);
        }
        let node = match expression {
            Expr::SmallInt(value) => OwnedNode::Integer(*value),
            Expr::SmallIntLiteral(source) => OwnedNode::Integer(parse_integer(source)?),
            Expr::Float(source) => OwnedNode::FloatBits(parse_float_bits(source)?),
            Expr::String(bytes) => OwnedNode::String(bytes.to_vec()),
            Expr::StringOwned(bytes) => OwnedNode::String(bytes.clone()),
            Expr::StringLiteral(source) => OwnedNode::String(decode_string(source)?),
            Expr::Symbol(name) => OwnedNode::Symbol(name.to_vec()),
            Expr::Global(name) => match self.binding_index(name) {
                Some(index) => OwnedNode::LexicalRead(index),
                None => OwnedNode::GlobalRead(name.to_vec()),
            },
            Expr::Nil => OwnedNode::Nil,
            Expr::True => OwnedNode::True,
            Expr::False => OwnedNode::False,
            Expr::Send {
                receiver,
                selector,
                args,
                ..
            } => {
                let mut operands = Vec::with_capacity(args.len() + 1);
                operands.push(self.expression(receiver, depth + 1)?);
                for argument in args {
                    operands.push(self.expression(argument, depth + 1)?);
                }
                OwnedNode::Send {
                    selector: selector.as_ref().to_vec(),
                    operands,
                }
            }
            Expr::Block(block) => {
                let capture_names = lexical_capture_names(block, &self.bindings);
                let captures = capture_names
                    .iter()
                    .map(|name| self.binding_index(name).ok_or(Error::InvalidLexicalScope))
                    .collect::<Result<Vec<_>, _>>()?;
                let program = encode_block_at_depth(block, &self.bindings, depth + 1)?;
                OwnedNode::BlockLiteral { program, captures }
            }
        };
        self.node(node)
    }

    fn finish(mut self, block: &Block<'_>, depth: usize) -> Result<Vec<u8>, Error> {
        for (statement_index, statement) in block.body.iter().enumerate() {
            let statement_index =
                u32::try_from(statement_index).map_err(|_| Error::ResourceLimit)?;
            match statement {
                Statement::Expr(expression) => {
                    let root_node = self.expression(expression, depth)?;
                    self.statements
                        .push(StatementSpec::Expression { root_node });
                }
                Statement::Let { name, value } => {
                    let root_node = self.expression(value, depth)?;
                    let let_index =
                        u32::try_from(self.lets.len()).map_err(|_| Error::ResourceLimit)?;
                    self.lets.push((name.to_vec(), statement_index));
                    self.statements.push(StatementSpec::Let {
                        root_node,
                        let_index,
                    });
                    self.bindings.push(name.to_vec());
                }
            }
        }

        let parameter_refs = self
            .parameters
            .iter()
            .map(Vec::as_slice)
            .collect::<Vec<_>>();
        let capture_refs = self.captures.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let lets = self
            .lets
            .iter()
            .map(|(name, defining_statement)| LetSpec {
                name,
                defining_statement: *defining_statement,
            })
            .collect::<Vec<_>>();
        let nodes = self
            .nodes
            .iter()
            .map(OwnedNode::as_spec)
            .collect::<Vec<_>>();
        encode(ProgramSpec {
            parameters: &parameter_refs,
            captures: &capture_refs,
            lets: &lets,
            statements: &self.statements,
            nodes: &nodes,
        })
        .map_err(|error| match error {
            birnam_artifact::portable_program::Error::Quantity => Error::ResourceLimit,
            birnam_artifact::portable_program::Error::Format
            | birnam_artifact::portable_program::Error::Name => Error::InvalidProgram,
        })
    }
}

pub fn encode_block(block: &Block<'_>, outer_bindings: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    encode_block_at_depth(block, outer_bindings, 0)
}

fn encode_block_at_depth(
    block: &Block<'_>,
    outer_bindings: &[Vec<u8>],
    depth: usize,
) -> Result<Vec<u8>, Error> {
    if depth >= MAX_DEPTH {
        return Err(Error::DepthLimit);
    }
    Builder::new(block, outer_bindings).finish(block, depth + 1)
}

fn parse_integer(source: &[u8]) -> Result<i64, Error> {
    core::str::from_utf8(source)
        .ok()
        .and_then(|source| source.parse().ok())
        .ok_or(Error::InvalidLiteral)
}

fn parse_float_bits(source: &[u8]) -> Result<u64, Error> {
    core::str::from_utf8(source)
        .ok()
        .and_then(|source| source.parse::<f64>().ok())
        .map(f64::to_bits)
        .ok_or(Error::InvalidLiteral)
}

fn decode_string(source: &[u8]) -> Result<Vec<u8>, Error> {
    let inner = source
        .strip_prefix(b"\"")
        .and_then(|source| source.strip_suffix(b"\""))
        .ok_or(Error::InvalidLiteral)?;
    let mut decoded = Vec::with_capacity(inner.len());
    let mut index = 0;
    while index < inner.len() {
        if inner[index] == b'\\' && index + 1 < inner.len() {
            let escape = match inner[index + 1] {
                b'"' => Some(b'"'),
                b'\\' => Some(b'\\'),
                b'n' => Some(b'\n'),
                b't' => Some(b'\t'),
                b'r' => Some(b'\r'),
                b'0' => Some(0),
                _ => None,
            };
            if let Some(byte) = escape {
                decoded.push(byte);
                index += 2;
                continue;
            }
        }
        decoded.push(inner[index]);
        index += 1;
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::encode_block;
    use crate::{Block, Expr, Statement};
    use birnam_artifact::portable_program::{BindingKind, inspect};
    use std::borrow::Cow;

    #[test]
    fn lowers_lexical_scope_nested_blocks_and_postorder_sends() {
        let nested = Block {
            params: vec![],
            body: vec![Statement::Expr(Expr::Global(b"outer"))],
        };
        let block = Block {
            params: vec![b"argument"],
            body: vec![
                Statement::Let {
                    name: b"local",
                    value: Expr::SmallIntLiteral(b"7"),
                },
                Statement::Expr(Expr::Send {
                    receiver: Box::new(Expr::Global(b"argument")),
                    selector: Cow::Borrowed(b"value:"),
                    args: vec![Expr::Block(Box::new(nested))],
                    source: b"[argument value: [| outer]]",
                }),
            ],
        };
        let bytes = encode_block(&block, &[b"outer".to_vec()]).unwrap();
        let program = inspect(&bytes).unwrap();
        assert_eq!(program.parameter_count, 1);
        assert_eq!(program.capture_count, 1);
        assert_eq!(program.binding(0).unwrap().kind, BindingKind::Parameter);
        assert_eq!(program.binding(1).unwrap().kind, BindingKind::Capture);
        assert_eq!(program.binding(2).unwrap().kind, BindingKind::Let);
        assert_eq!(program.binding(1).unwrap().name, b"outer");
        assert_eq!(program.node_count, 4);
        assert_eq!(program.node(0).unwrap().kind, 1);
        assert_eq!(program.node(1).unwrap().kind, 8);
        assert_eq!(program.node(2).unwrap().kind, 11);
        assert_eq!(program.node(3).unwrap().kind, 10);
        assert_eq!(program.node_operand(3, 0), Some(1));
        assert_eq!(program.node_operand(3, 1), Some(2));
    }

    #[test]
    fn preserves_literal_meaning() {
        let block = Block {
            params: vec![],
            body: vec![
                Statement::Expr(Expr::Float(b"-0.0")),
                Statement::Expr(Expr::StringLiteral(br#""a\n\0""#)),
                Statement::Expr(Expr::Symbol(b"value")),
                Statement::Expr(Expr::Nil),
                Statement::Expr(Expr::True),
                Statement::Expr(Expr::False),
            ],
        };
        let bytes = encode_block(&block, &[]).unwrap();
        let program = inspect(&bytes).unwrap();
        assert_eq!(program.node(0).unwrap().payload, (-0.0_f64).to_bits());
        assert_eq!(program.node(1).unwrap().data, b"a\n\0");
        assert_eq!(program.node(2).unwrap().data, b"value");
        assert_eq!(program.node(3).unwrap().kind, 5);
        assert_eq!(program.node(4).unwrap().kind, 6);
        assert_eq!(program.node(5).unwrap().kind, 7);
    }
}
