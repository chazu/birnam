//! Deterministic, uncolored control-flow lowering for stackless implementations.
//!
//! This module stops at the semantic boundary. It assigns stable logical IDs,
//! makes every source send a driver-visible terminator, and provides a small
//! interpreter against which later slot coloring can be checked. Physical
//! frame slots, descriptors, and block-capture layouts belong to later S2
//! slices.

use std::collections::BTreeSet;

use birnam_artifact::portable_program::{self, NodeKind, StatementKind};

use crate::{Block, Expr, Program, Statement};

/// A conservative compiler-local bound. It is not part of the runtime ABI;
/// changing it changes PC assignment and therefore the compiler lowering
/// version's artifact identity.
pub const MAX_OPERATIONS_PER_BLOCK: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InventoryRootKind {
    NonMethodProgram,
    MethodOperation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InventoryImplementationKind {
    NamedAdapter,
    BlockBody,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImplementationKey {
    pub root_kind: InventoryRootKind,
    pub root_index: u32,
    pub path: Vec<u32>,
    pub implementation_kind: InventoryImplementationKind,
}

/// One top-level binding whose value is constructed without executing an
/// arbitrary initializer send. S2 generation projection admits this exact
/// shape so a candidate may replace a VM-owned global closure atomically while
/// leaving effectful module initialization outside the transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedBlockBinding {
    pub statement_index: usize,
    pub name: Vec<u8>,
    pub key: ImplementationKey,
    pub captures: Vec<Vec<u8>>,
}

/// Enumerate projection-safe top-level block bindings against the same lexical
/// environment and nonmethod ordinal space used by `implementation_inventory`.
/// Merely containing a block somewhere in an arbitrary expression is not
/// enough: the outer value must be the complete RHS of a top-level `let`.
pub fn projected_block_bindings(program: &Program<'_>) -> Vec<ProjectedBlockBinding> {
    let mut bindings = Vec::new();
    let mut environment = Vec::<Vec<u8>>::new();
    let mut non_method_block_ordinal = 0u32;
    for (statement_index, statement) in program.body.iter().enumerate() {
        let expression = match statement {
            Statement::Let { value, .. } | Statement::Expr(value) => value,
        };
        if method_operation(expression).is_none() {
            if let Statement::Let {
                name,
                value: Expr::Block(block),
            } = statement
            {
                bindings.push(ProjectedBlockBinding {
                    statement_index,
                    name: name.to_vec(),
                    key: ImplementationKey {
                        root_kind: InventoryRootKind::NonMethodProgram,
                        root_index: 0,
                        path: vec![non_method_block_ordinal],
                        implementation_kind: InventoryImplementationKind::BlockBody,
                    },
                    captures: lexical_capture_names(block, &environment),
                });
            }
            collect_block_keys(
                expression,
                InventoryRootKind::NonMethodProgram,
                0,
                &[],
                &mut non_method_block_ordinal,
                &mut Vec::new(),
            );
        }
        if let Statement::Let { name, .. } = statement {
            environment.push(name.to_vec());
        }
    }
    bindings
}

/// Enumerate the complete implementation inventory of one normalized source
/// revision. Method operation positions include removals; only installations
/// produce a NamedAdapter row. Block ordinals are source-ordered within their
/// immediately enclosing implementation.
pub fn implementation_inventory(program: &Program<'_>) -> Vec<ImplementationKey> {
    implementation_inventory_with_blocks(program)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

/// Keep the already visited Block alongside its stable key. Emission needs this
/// association, not a fresh whole-program resolution and capture analysis per key.
pub(crate) fn implementation_inventory_with_blocks<'a>(
    program: &'a Program<'a>,
) -> Vec<(ImplementationKey, Option<&'a Block<'a>>)> {
    let mut inventory = Vec::new();
    let mut operation_index = 0u32;
    let mut non_method_block_ordinal = 0u32;
    for statement in &program.body {
        let expression = match statement {
            Statement::Let { value, .. } | Statement::Expr(value) => value,
        };
        if let Some(install) = method_operation(expression) {
            if install {
                inventory.push((
                    ImplementationKey {
                        root_kind: InventoryRootKind::MethodOperation,
                        root_index: operation_index,
                        path: Vec::new(),
                        implementation_kind: InventoryImplementationKind::NamedAdapter,
                    },
                    None,
                ));
            }
            let mut ordinal = 0;
            collect_block_keys(
                expression,
                InventoryRootKind::MethodOperation,
                operation_index,
                &[],
                &mut ordinal,
                &mut inventory,
            );
            operation_index = operation_index
                .checked_add(1)
                .expect("normalized program operation count is bounded by source size");
        } else {
            collect_block_keys(
                expression,
                InventoryRootKind::NonMethodProgram,
                0,
                &[],
                &mut non_method_block_ordinal,
                &mut inventory,
            );
        }
    }
    inventory.sort_by(|(left, _), (right, _)| left.cmp(right));
    inventory
}

pub(crate) fn method_operation(expression: &Expr<'_>) -> Option<bool> {
    let Expr::Send {
        receiver,
        selector,
        args,
        ..
    } = expression
    else {
        return None;
    };
    if !matches!(receiver.as_ref(), Expr::Global(name) if *name == b"Reflect") {
        return None;
    }
    match (selector.as_ref(), args.len()) {
        (b"defineMethod:on:as:", 3) | (b"defineMethod:on:as:doc:", 4) => Some(true),
        (b"removeMethod:on:", 2) => Some(false),
        _ => None,
    }
}

fn collect_block_keys<'a>(
    expression: &'a Expr<'a>,
    root_kind: InventoryRootKind,
    root_index: u32,
    parent_path: &[u32],
    next_ordinal: &mut u32,
    inventory: &mut Vec<(ImplementationKey, Option<&'a Block<'a>>)>,
) {
    match expression {
        Expr::Block(block) => {
            let mut path = parent_path.to_vec();
            path.push(*next_ordinal);
            *next_ordinal = next_ordinal
                .checked_add(1)
                .expect("normalized block count is bounded by source size");
            inventory.push((
                ImplementationKey {
                    root_kind,
                    root_index,
                    path: path.clone(),
                    implementation_kind: InventoryImplementationKind::BlockBody,
                },
                Some(block),
            ));
            let mut nested_ordinal = 0;
            for statement in &block.body {
                let expression = match statement {
                    Statement::Let { value, .. } | Statement::Expr(value) => value,
                };
                collect_block_keys(
                    expression,
                    root_kind,
                    root_index,
                    &path,
                    &mut nested_ordinal,
                    inventory,
                );
            }
        }
        Expr::Send { receiver, args, .. } => {
            collect_block_keys(
                receiver,
                root_kind,
                root_index,
                parent_path,
                next_ordinal,
                inventory,
            );
            for argument in args {
                collect_block_keys(
                    argument,
                    root_kind,
                    root_index,
                    parent_path,
                    next_ordinal,
                    inventory,
                );
            }
        }
        _ => {}
    }
}

/// One exact lexical block resolved from the normalized implementation
/// inventory, together with the captures visible at its construction site.
pub struct ResolvedBlockImplementation<'a> {
    pub block: &'a Block<'a>,
    pub captures: Vec<Vec<u8>>,
}

/// Resolve one stable BlockBody key against the same traversal that produces
/// `implementation_inventory`.
pub fn resolve_block_implementation<'a>(
    program: &'a Program<'a>,
    key: &ImplementationKey,
) -> Option<ResolvedBlockImplementation<'a>> {
    if key.implementation_kind != InventoryImplementationKind::BlockBody || key.path.is_empty() {
        return None;
    }
    let mut environment = Vec::<Vec<u8>>::new();
    let mut operation_index = 0u32;
    let mut non_method_ordinal = 0u32;
    for statement in &program.body {
        let expression = match statement {
            Statement::Let { value, .. } | Statement::Expr(value) => value,
        };
        let found = if let Some(_install) = method_operation(expression) {
            let mut ordinal = 0;
            resolve_block_in_expression(
                expression,
                InventoryRootKind::MethodOperation,
                operation_index,
                &[],
                &mut ordinal,
                &environment,
                key,
            )
        } else {
            resolve_block_in_expression(
                expression,
                InventoryRootKind::NonMethodProgram,
                0,
                &[],
                &mut non_method_ordinal,
                &environment,
                key,
            )
        };
        if found.is_some() {
            return found;
        }
        if method_operation(expression).is_some() {
            operation_index = operation_index.checked_add(1)?;
        }
        if let Statement::Let { name, .. } = statement {
            environment.push(name.to_vec());
        }
    }
    None
}

/// Compute canonical lexical captures from an ordered enclosing environment.
/// Shadowed bindings retain only the most recent active identity; remaining
/// captures stay ordered by their enclosing binding identity.
pub(crate) fn lexical_capture_names(block: &Block<'_>, environment: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut references = Vec::new();
    for statement in &block.body {
        crate::collect_statement_refs(statement, &mut references);
    }
    environment
        .iter()
        .enumerate()
        .filter(|(index, name)| {
            references.iter().any(|reference| reference == *name)
                && !block.params.contains(&name.as_slice())
                && !crate::block_has_let(block, name)
                && !environment[index + 1..].iter().any(|later| later == *name)
        })
        .map(|(_, name)| name.clone())
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn resolve_block_in_expression<'a>(
    expression: &'a Expr<'a>,
    root_kind: InventoryRootKind,
    root_index: u32,
    parent_path: &[u32],
    next_ordinal: &mut u32,
    environment: &[Vec<u8>],
    wanted: &ImplementationKey,
) -> Option<ResolvedBlockImplementation<'a>> {
    match expression {
        Expr::Block(block) => {
            let mut path = parent_path.to_vec();
            path.push(*next_ordinal);
            *next_ordinal = next_ordinal.checked_add(1)?;
            // Descendant ordinals belong to this block, not its siblings. A
            // different root or path cannot contain the requested implementation.
            if wanted.root_kind != root_kind
                || wanted.root_index != root_index
                || !wanted.path.starts_with(&path)
            {
                return None;
            }
            let captures = lexical_capture_names(block, environment);
            if wanted.root_kind == root_kind
                && wanted.root_index == root_index
                && wanted.path == path
            {
                return Some(ResolvedBlockImplementation { block, captures });
            }
            let mut nested_environment = block
                .params
                .iter()
                .map(|name| name.to_vec())
                .chain(captures)
                .collect::<Vec<_>>();
            let mut nested_ordinal = 0;
            for statement in &block.body {
                let expression = match statement {
                    Statement::Let { value, .. } | Statement::Expr(value) => value,
                };
                if let Some(found) = resolve_block_in_expression(
                    expression,
                    root_kind,
                    root_index,
                    &path,
                    &mut nested_ordinal,
                    &nested_environment,
                    wanted,
                ) {
                    return Some(found);
                }
                if let Statement::Let { name, .. } = statement {
                    nested_environment.push(name.to_vec());
                }
            }
            None
        }
        Expr::Send { receiver, args, .. } => {
            if let Some(found) = resolve_block_in_expression(
                receiver,
                root_kind,
                root_index,
                parent_path,
                next_ordinal,
                environment,
                wanted,
            ) {
                return Some(found);
            }
            for argument in args {
                if let Some(found) = resolve_block_in_expression(
                    argument,
                    root_kind,
                    root_index,
                    parent_path,
                    next_ordinal,
                    environment,
                    wanted,
                ) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallSiteId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgramCounter(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub struct LogicalProgram<'a> {
    pub entry: ProgramCounter,
    pub argument_count: u32,
    pub capture_count: u32,
    pub value_count: u32,
    pub binding_count: u32,
    pub call_site_count: u32,
    pub blocks: Vec<BasicBlock<'a>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BasicBlock<'a> {
    pub pc: ProgramCounter,
    pub operations: Vec<Operation<'a>>,
    pub terminator: Terminator<'a>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Operation<'a> {
    pub result: ValueId,
    pub kind: OperationKind<'a>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OperationKind<'a> {
    EntryReceiver,
    /// Runtime-owned named-method implementation anchor copied into the Frame
    /// before PC zero runs. BlockBody programs never contain this operation.
    EntryImplementation,
    EntryArgument {
        index: u32,
        binding: BindingId,
        name: &'a [u8],
    },
    /// One lexically captured Oop copied from the block receiver payload into
    /// the Frame before PC zero runs.
    EntryCapture {
        index: u32,
        binding: BindingId,
        name: &'a [u8],
    },
    Literal(Literal<'a>),
    GlobalLookup(&'a [u8]),
    /// The nested implementation is lowered independently. Capture operands
    /// are the exact live values copied into its closure payload.
    BlockLiteral {
        block: &'a Block<'a>,
        captures: Vec<ValueId>,
    },
    /// A nested implementation named directly by admitted canonical semantic
    /// bytes. This is the production representation for PortableProgram
    /// lowering; `node_index` is the canonical occurrence identity within the
    /// enclosing PortableProgram and never reconstructs a source-shaped Block.
    PortableBlockLiteral {
        node_index: u32,
        program: &'a [u8],
        captures: Vec<ValueId>,
    },
    Bind {
        binding: BindingId,
        name: &'a [u8],
        source: ValueId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal<'a> {
    SmallInt(i64),
    SmallIntSource(&'a [u8]),
    Float(&'a [u8]),
    FloatBits(u64),
    String(&'a [u8]),
    StringSource(&'a [u8]),
    Symbol(&'a [u8]),
    Nil,
    True,
    False,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Terminator<'a> {
    Call {
        call_site: CallSiteId,
        receiver: ValueId,
        selector: &'a [u8],
        arguments: Vec<ValueId>,
        result: ValueId,
        successor: ProgramCounter,
        source: &'a [u8],
    },
    TailCall {
        call_site: CallSiteId,
        receiver: ValueId,
        selector: &'a [u8],
        arguments: Vec<ValueId>,
        source: &'a [u8],
    },
    Return(ValueId),
    Continue(ProgramCounter),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LowerError {
    DepthLimit,
    QuantityOverflow,
    InvalidCapture,
    InvalidProgram,
}

const MAX_EXPR_DEPTH: usize = 4096;

#[derive(Debug)]
enum Linear<'a> {
    Operation(Operation<'a>),
    Send {
        call_site: CallSiteId,
        receiver: ValueId,
        selector: &'a [u8],
        arguments: Vec<ValueId>,
        result: ValueId,
        source: &'a [u8],
    },
}

struct Lowerer<'a> {
    linear: Vec<Linear<'a>>,
    environment: Vec<(&'a [u8], ValueId)>,
    next_value: u32,
    next_binding: u32,
    next_call_site: u32,
}

impl<'a> Lowerer<'a> {
    fn value(&mut self) -> Result<ValueId, LowerError> {
        let value = ValueId(self.next_value);
        self.next_value = self
            .next_value
            .checked_add(1)
            .ok_or(LowerError::QuantityOverflow)?;
        Ok(value)
    }

    fn binding(&mut self) -> Result<BindingId, LowerError> {
        let binding = BindingId(self.next_binding);
        self.next_binding = self
            .next_binding
            .checked_add(1)
            .ok_or(LowerError::QuantityOverflow)?;
        Ok(binding)
    }

    fn call_site(&mut self) -> Result<CallSiteId, LowerError> {
        let call_site = CallSiteId(self.next_call_site);
        self.next_call_site = self
            .next_call_site
            .checked_add(1)
            .ok_or(LowerError::QuantityOverflow)?;
        Ok(call_site)
    }

    fn operation(&mut self, kind: OperationKind<'a>) -> Result<ValueId, LowerError> {
        let result = self.value()?;
        self.linear
            .push(Linear::Operation(Operation { result, kind }));
        Ok(result)
    }

    fn lookup(&self, name: &[u8]) -> Option<ValueId> {
        self.environment
            .iter()
            .rev()
            .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
    }

    fn expression(
        &mut self,
        expression: &'a Expr<'a>,
        depth: usize,
    ) -> Result<ValueId, LowerError> {
        if depth >= MAX_EXPR_DEPTH {
            return Err(LowerError::DepthLimit);
        }
        match expression {
            Expr::SmallInt(value) => {
                self.operation(OperationKind::Literal(Literal::SmallInt(*value)))
            }
            Expr::SmallIntLiteral(source) => {
                self.operation(OperationKind::Literal(Literal::SmallIntSource(source)))
            }
            Expr::Float(source) => self.operation(OperationKind::Literal(Literal::Float(source))),
            Expr::String(bytes) => self.operation(OperationKind::Literal(Literal::String(bytes))),
            Expr::StringOwned(bytes) => {
                self.operation(OperationKind::Literal(Literal::String(bytes.as_slice())))
            }
            Expr::StringLiteral(source) => {
                self.operation(OperationKind::Literal(Literal::StringSource(source)))
            }
            Expr::Symbol(name) => self.operation(OperationKind::Literal(Literal::Symbol(name))),
            Expr::Global(name) => match self.lookup(name) {
                Some(value) => Ok(value),
                None => self.operation(OperationKind::GlobalLookup(name)),
            },
            Expr::Nil => self.operation(OperationKind::Literal(Literal::Nil)),
            Expr::True => self.operation(OperationKind::Literal(Literal::True)),
            Expr::False => self.operation(OperationKind::Literal(Literal::False)),
            Expr::Block(block) => {
                let environment = self
                    .environment
                    .iter()
                    .map(|(name, _)| name.to_vec())
                    .collect::<Vec<_>>();
                let capture_names = lexical_capture_names(block, &environment);
                let captures = capture_names
                    .iter()
                    .map(|name| self.lookup(name).ok_or(LowerError::InvalidCapture))
                    .collect::<Result<Vec<_>, _>>()?;
                self.operation(OperationKind::BlockLiteral { block, captures })
            }
            Expr::Send {
                receiver,
                selector,
                args,
                source,
            } => {
                // Birnam order is receiver once, then arguments once from left
                // to right, then fresh resolution by the Machine.
                let receiver = self.expression(receiver, depth + 1)?;
                let arguments = args
                    .iter()
                    .map(|argument| self.expression(argument, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                let result = self.value()?;
                let call_site = self.call_site()?;
                self.linear.push(Linear::Send {
                    call_site,
                    receiver,
                    selector: selector.as_ref(),
                    arguments,
                    result,
                    source,
                });
                Ok(result)
            }
        }
    }

    fn statements(&mut self, statements: &'a [Statement<'a>]) -> Result<ValueId, LowerError> {
        if statements.is_empty() {
            return self.operation(OperationKind::Literal(Literal::Nil));
        }
        let mut result = None;
        for statement in statements {
            result = Some(match statement {
                Statement::Expr(expression) => self.expression(expression, 0)?,
                Statement::Let { name, value } => {
                    let source = self.expression(value, 0)?;
                    let binding = self.binding()?;
                    let value = self.operation(OperationKind::Bind {
                        binding,
                        name,
                        source,
                    })?;
                    self.environment.push((name, value));
                    value
                }
            });
        }
        Ok(result.expect("non-empty statements have a result"))
    }
}

struct PortableLowerer<'a> {
    linear: Vec<Linear<'a>>,
    bindings: Vec<Option<ValueId>>,
    next_node: u32,
    next_value: u32,
    next_binding: u32,
    next_call_site: u32,
}

impl<'a> PortableLowerer<'a> {
    fn value(&mut self) -> Result<ValueId, LowerError> {
        let value = ValueId(self.next_value);
        self.next_value = self
            .next_value
            .checked_add(1)
            .ok_or(LowerError::QuantityOverflow)?;
        Ok(value)
    }

    fn binding(&mut self, expected: u32) -> Result<BindingId, LowerError> {
        if self.next_binding != expected {
            return Err(LowerError::InvalidProgram);
        }
        self.next_binding = self
            .next_binding
            .checked_add(1)
            .ok_or(LowerError::QuantityOverflow)?;
        Ok(BindingId(expected))
    }

    fn call_site(&mut self) -> Result<CallSiteId, LowerError> {
        let call_site = CallSiteId(self.next_call_site);
        self.next_call_site = self
            .next_call_site
            .checked_add(1)
            .ok_or(LowerError::QuantityOverflow)?;
        Ok(call_site)
    }

    fn operation(&mut self, kind: OperationKind<'a>) -> Result<ValueId, LowerError> {
        let result = self.value()?;
        self.linear
            .push(Linear::Operation(Operation { result, kind }));
        Ok(result)
    }

    fn bound_value(&self, index: u32) -> Result<ValueId, LowerError> {
        self.bindings
            .get(index as usize)
            .copied()
            .flatten()
            .ok_or(LowerError::InvalidProgram)
    }

    fn expression(
        &mut self,
        program: &portable_program::Program<'a>,
        index: u32,
        depth: usize,
    ) -> Result<ValueId, LowerError> {
        if depth >= MAX_EXPR_DEPTH {
            return Err(LowerError::DepthLimit);
        }
        let node = program
            .node(index as usize)
            .ok_or(LowerError::InvalidProgram)?;
        let kind = node.semantic_kind().ok_or(LowerError::InvalidProgram)?;

        let result = match kind {
            NodeKind::Integer => self.operation(OperationKind::Literal(Literal::SmallInt(
                node.payload as i64,
            )))?,
            NodeKind::FloatBits => {
                self.operation(OperationKind::Literal(Literal::FloatBits(node.payload)))?
            }
            NodeKind::String => {
                self.operation(OperationKind::Literal(Literal::String(node.data)))?
            }
            NodeKind::Symbol => {
                self.operation(OperationKind::Literal(Literal::Symbol(node.data)))?
            }
            NodeKind::Nil => self.operation(OperationKind::Literal(Literal::Nil))?,
            NodeKind::True => self.operation(OperationKind::Literal(Literal::True))?,
            NodeKind::False => self.operation(OperationKind::Literal(Literal::False))?,
            NodeKind::LexicalRead => self.bound_value(node.auxiliary_0)?,
            NodeKind::GlobalRead => self.operation(OperationKind::GlobalLookup(node.data))?,
            NodeKind::Send => {
                let receiver_index = program
                    .node_operand(index as usize, 0)
                    .ok_or(LowerError::InvalidProgram)?;
                let receiver = self.expression(program, receiver_index, depth + 1)?;
                let mut arguments =
                    Vec::with_capacity(node.operand_count.saturating_sub(1) as usize);
                for operand_index in 1..node.operand_count as usize {
                    let child = program
                        .node_operand(index as usize, operand_index)
                        .ok_or(LowerError::InvalidProgram)?;
                    arguments.push(self.expression(program, child, depth + 1)?);
                }
                let result = self.value()?;
                let call_site = self.call_site()?;
                self.linear.push(Linear::Send {
                    call_site,
                    receiver,
                    selector: node.data,
                    arguments,
                    result,
                    source: b"",
                });
                result
            }
            NodeKind::BlockLiteral => {
                let mut captures = Vec::with_capacity(node.operand_count as usize);
                for operand_index in 0..node.operand_count as usize {
                    let binding = program
                        .node_operand(index as usize, operand_index)
                        .ok_or(LowerError::InvalidProgram)?;
                    captures.push(self.bound_value(binding)?);
                }
                self.operation(OperationKind::PortableBlockLiteral {
                    node_index: index,
                    program: node.data,
                    captures,
                })?
            }
        };

        if index != self.next_node {
            return Err(LowerError::InvalidProgram);
        }
        self.next_node = self
            .next_node
            .checked_add(1)
            .ok_or(LowerError::QuantityOverflow)?;
        Ok(result)
    }
}

/// Lower one admitted PortableProgram directly into deterministic, uncolored
/// CFG form. Lexical identity, float bits, and nested capture operands are
/// consumed from the canonical program and are never rediscovered from names.
pub fn lower_portable_block(bytes: &[u8]) -> Result<LogicalProgram<'_>, LowerError> {
    lower_portable_view(portable_program::inspect(bytes).map_err(|_| LowerError::InvalidProgram)?)
}
/// Lower an immutable view whose complete nested program tree was admitted.
pub fn lower_portable_view(program: portable_program::Program<'_>) -> Result<LogicalProgram<'_>, LowerError> {
    let mut lowerer = PortableLowerer {
        linear: Vec::new(),
        bindings: vec![None; program.binding_count as usize],
        next_node: 0,
        next_value: 0,
        next_binding: 0,
        next_call_site: 0,
    };
    lowerer.operation(OperationKind::EntryReceiver)?;

    for index in 0..program.parameter_count {
        let binding_value = program
            .binding(index as usize)
            .ok_or(LowerError::InvalidProgram)?;
        let binding = lowerer.binding(index)?;
        let value = lowerer.operation(OperationKind::EntryArgument {
            index,
            binding,
            name: binding_value.name,
        })?;
        lowerer.bindings[index as usize] = Some(value);
    }
    for capture_index in 0..program.capture_count {
        let binding_index = program
            .parameter_count
            .checked_add(capture_index)
            .ok_or(LowerError::QuantityOverflow)?;
        let binding_value = program
            .binding(binding_index as usize)
            .ok_or(LowerError::InvalidProgram)?;
        let binding = lowerer.binding(binding_index)?;
        let value = lowerer.operation(OperationKind::EntryCapture {
            index: capture_index,
            binding,
            name: binding_value.name,
        })?;
        lowerer.bindings[binding_index as usize] = Some(value);
    }

    let result = if program.statement_count == 0 {
        lowerer.operation(OperationKind::Literal(Literal::Nil))?
    } else {
        let mut result = None;
        for statement_index in 0..program.statement_count as usize {
            let statement = program
                .statement(statement_index)
                .ok_or(LowerError::InvalidProgram)?;
            let source = lowerer.expression(&program, statement.root_node, 0)?;
            result = Some(match statement.kind {
                StatementKind::Expression => source,
                StatementKind::Let => {
                    let binding_index =
                        statement.binding_index.ok_or(LowerError::InvalidProgram)?;
                    let binding_value = program
                        .binding(binding_index as usize)
                        .ok_or(LowerError::InvalidProgram)?;
                    let binding = lowerer.binding(binding_index)?;
                    let value = lowerer.operation(OperationKind::Bind {
                        binding,
                        name: binding_value.name,
                        source,
                    })?;
                    let slot = lowerer
                        .bindings
                        .get_mut(binding_index as usize)
                        .ok_or(LowerError::InvalidProgram)?;
                    if slot.replace(value).is_some() {
                        return Err(LowerError::InvalidProgram);
                    }
                    value
                }
            });
        }
        result.ok_or(LowerError::InvalidProgram)?
    };

    if lowerer.next_node != program.node_count || lowerer.next_binding != program.binding_count {
        return Err(LowerError::InvalidProgram);
    }
    let blocks = assemble_blocks(lowerer.linear, result)?;
    Ok(LogicalProgram {
        entry: ProgramCounter(0),
        argument_count: program.parameter_count,
        capture_count: program.capture_count,
        value_count: lowerer.next_value,
        binding_count: lowerer.next_binding,
        call_site_count: lowerer.next_call_site,
        blocks,
    })
}

/// Lower one implementation body into deterministic, uncolored CFG form.
///
/// PC zero initializes the receiver and ordered arguments. Every send becomes
/// a Call or TailCall terminator; no selector or receiver is classified here.
pub fn lower_block<'a>(block: &'a Block<'a>) -> Result<LogicalProgram<'a>, LowerError> {
    lower_block_with_captures(block, &[])
}

/// Lower one block body with its already-discovered canonical capture order.
/// Capture discovery belongs to the enclosing lexical traversal; this seam
/// makes the copied entry values explicit in the logical IR.
pub fn lower_block_with_captures<'a>(
    block: &'a Block<'a>,
    captures: &'a [&'a [u8]],
) -> Result<LogicalProgram<'a>, LowerError> {
    let mut lowerer = Lowerer {
        linear: Vec::new(),
        environment: Vec::new(),
        next_value: 0,
        next_binding: 0,
        next_call_site: 0,
    };
    lowerer.operation(OperationKind::EntryReceiver)?;
    for (index, name) in block.params.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| LowerError::QuantityOverflow)?;
        let binding = lowerer.binding()?;
        let value = lowerer.operation(OperationKind::EntryArgument {
            index,
            binding,
            name,
        })?;
        lowerer.environment.push((name, value));
    }
    for (index, name) in captures.iter().copied().enumerate() {
        let index = u32::try_from(index).map_err(|_| LowerError::QuantityOverflow)?;
        let binding = lowerer.binding()?;
        let value = lowerer.operation(OperationKind::EntryCapture {
            index,
            binding,
            name,
        })?;
        lowerer.environment.push((name, value));
    }
    let result = lowerer.statements(&block.body)?;
    let blocks = assemble_blocks(lowerer.linear, result)?;
    Ok(LogicalProgram {
        entry: ProgramCounter(0),
        argument_count: u32::try_from(block.params.len())
            .map_err(|_| LowerError::QuantityOverflow)?,
        capture_count: u32::try_from(captures.len()).map_err(|_| LowerError::QuantityOverflow)?,
        value_count: lowerer.next_value,
        binding_count: lowerer.next_binding,
        call_site_count: lowerer.next_call_site,
        blocks,
    })
}

/// Build the fixed one-step named-method adapter. It performs a fresh
/// `value:`-family lookup on the retained implementation object and tail-calls
/// through the ordinary Machine boundary.
pub fn lower_named_adapter<'a>(
    language_arity: u32,
    implementation_selector: &'a [u8],
) -> Result<LogicalProgram<'a>, LowerError> {
    let mut operations = Vec::new();
    let receiver = ValueId(0);
    operations.push(Operation {
        result: receiver,
        kind: OperationKind::EntryReceiver,
    });
    let mut arguments = Vec::with_capacity(language_arity as usize + 1);
    arguments.push(receiver);
    for index in 0..language_arity {
        let value = ValueId(index.checked_add(1).ok_or(LowerError::QuantityOverflow)?);
        operations.push(Operation {
            result: value,
            kind: OperationKind::EntryArgument {
                index,
                binding: BindingId(index),
                name: b"",
            },
        });
        arguments.push(value);
    }
    let implementation = ValueId(
        language_arity
            .checked_add(1)
            .ok_or(LowerError::QuantityOverflow)?,
    );
    operations.push(Operation {
        result: implementation,
        kind: OperationKind::EntryImplementation,
    });
    Ok(LogicalProgram {
        entry: ProgramCounter(0),
        argument_count: language_arity,
        capture_count: 0,
        value_count: language_arity
            .checked_add(2)
            .ok_or(LowerError::QuantityOverflow)?,
        binding_count: language_arity,
        call_site_count: 1,
        blocks: vec![BasicBlock {
            pc: ProgramCounter(0),
            operations,
            terminator: Terminator::TailCall {
                call_site: CallSiteId(0),
                receiver: implementation,
                selector: implementation_selector,
                arguments,
                source: b"",
            },
        }],
    })
}

fn assemble_blocks<'a>(
    linear: Vec<Linear<'a>>,
    final_value: ValueId,
) -> Result<Vec<BasicBlock<'a>>, LowerError> {
    let mut chunks: Vec<(Vec<Operation<'a>>, Option<Linear<'a>>)> = Vec::new();
    let mut operations = Vec::new();
    let linear_len = linear.len();
    for (index, item) in linear.into_iter().enumerate() {
        match item {
            Linear::Operation(operation) => {
                operations.push(operation);
                if operations.len() == MAX_OPERATIONS_PER_BLOCK && index + 1 < linear_len {
                    chunks.push((std::mem::take(&mut operations), None));
                }
            }
            send @ Linear::Send { .. } => {
                chunks.push((std::mem::take(&mut operations), Some(send)));
            }
        }
    }
    if !operations.is_empty() || chunks.is_empty() {
        chunks.push((operations, None));
    }
    if matches!(
        chunks.last(),
        Some((_, Some(Linear::Send { result, .. }))) if *result != final_value
    ) {
        // A discarded final send, or a final reference to a value established
        // before that send, still needs a resume block in which to return.
        chunks.push((Vec::new(), None));
    }

    let block_count = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, (operations, boundary))| {
            let block_pc = pc(index)?;
            let successor = (index + 1 < block_count)
                .then(|| pc(index + 1))
                .transpose()?;
            let terminator = match boundary {
                Some(Linear::Send {
                    call_site,
                    receiver,
                    selector,
                    arguments,
                    result,
                    source,
                }) if successor.is_none() && result == final_value => Terminator::TailCall {
                    call_site,
                    receiver,
                    selector,
                    arguments,
                    source,
                },
                Some(Linear::Send {
                    call_site,
                    receiver,
                    selector,
                    arguments,
                    result,
                    source,
                }) => Terminator::Call {
                    call_site,
                    receiver,
                    selector,
                    arguments,
                    result,
                    successor: successor.expect("a non-tail send has a successor"),
                    source,
                },
                Some(Linear::Operation(_)) => unreachable!("only sends form explicit boundaries"),
                None => match successor {
                    Some(successor) => Terminator::Continue(successor),
                    None => Terminator::Return(final_value),
                },
            };
            Ok(BasicBlock {
                pc: block_pc,
                operations,
                terminator,
            })
        })
        .collect()
}

fn pc(index: usize) -> Result<ProgramCounter, LowerError> {
    u32::try_from(index)
        .map(ProgramCounter)
        .map_err(|_| LowerError::QuantityOverflow)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockLiveness {
    pub pc: ProgramCounter,
    pub live_in: Vec<ValueId>,
    pub live_out: Vec<ValueId>,
}

/// Compute reference liveness over the logical CFG. All logical values are
/// Oops at this layer. Sorted sets keep the result deterministic and make it a
/// stable input to the later coloring pass.
pub fn reference_liveness(program: &LogicalProgram<'_>) -> Vec<BlockLiveness> {
    let mut positions = std::collections::HashMap::with_capacity(program.blocks.len());
    for (index, block) in program.blocks.iter().enumerate() {
        positions.entry(block.pc).or_insert(index);
    }
    let successors = program.blocks.iter().map(|block| match &block.terminator {
        Terminator::Call { successor, .. } | Terminator::Continue(successor) => positions.get(successor).copied(),
        Terminator::TailCall { .. } | Terminator::Return(_) => None,
    }).collect::<Vec<_>>();
    let mut live_in = vec![BTreeSet::new(); program.blocks.len()];
    let mut live_out = vec![BTreeSet::new(); program.blocks.len()];
    loop {
        let mut changed = false;
        for (index, block) in program.blocks.iter().enumerate().rev() {
            let successor = successors[index];
            let new_out = successor
                .map(|successor| live_in[successor].clone())
                .unwrap_or_default();
            let mut needed = new_out.clone();
            let mut defined = BTreeSet::new();
            match &block.terminator {
                Terminator::Call {
                    receiver,
                    arguments,
                    result,
                    ..
                } => {
                    defined.insert(*result);
                    needed.insert(*receiver);
                    needed.extend(arguments.iter().copied());
                }
                Terminator::TailCall {
                    receiver,
                    arguments,
                    ..
                } => {
                    needed.insert(*receiver);
                    needed.extend(arguments.iter().copied());
                }
                Terminator::Return(value) => {
                    needed.insert(*value);
                }
                Terminator::Continue(_) => {}
            }
            for operation in block.operations.iter().rev() {
                needed.remove(&operation.result);
                match &operation.kind {
                    OperationKind::Bind { source, .. } => {
                        needed.insert(*source);
                    }
                    OperationKind::BlockLiteral { captures, .. }
                    | OperationKind::PortableBlockLiteral { captures, .. } => {
                        needed.extend(captures.iter().copied());
                    }
                    _ => {}
                }
                defined.insert(operation.result);
            }
            // A Call's result is produced by the driver at the outgoing edge,
            // so it is not required on entry even when the successor uses it.
            needed.retain(|value| !defined.contains(value));
            if live_out[index] != new_out || live_in[index] != needed {
                live_out[index] = new_out;
                live_in[index] = needed;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    program
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| BlockLiveness {
            pc: block.pc,
            live_in: live_in[index].iter().copied().collect(),
            live_out: live_out[index].iter().copied().collect(),
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    InvalidProgram,
    QuantityOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotCopy {
    pub source: u32,
    pub destination: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColoredEdge {
    pub source: ProgramCounter,
    pub target: ProgramCounter,
    pub result_initialization_slot: Option<u32>,
    pub copies: Vec<SlotCopy>,
    pub clear_bitmap_words: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalLayout {
    /// Dense ValueId-indexed physical locations.
    pub value_slots: Vec<u32>,
    /// Sorted `(ValueId, slot)` rows for each dense PC. These are the complete
    /// step-safepoint unions and directly feed PCValueLocations.
    pub pc_value_locations: Vec<Vec<(ValueId, u32)>>,
    pub root_bitmap_words: Vec<u64>,
    pub root_words_per_pc: u32,
    pub slot_count: u32,
    pub entry_receiver_slot: u32,
    pub entry_argument_start: u32,
    pub entry_argument_count: u32,
    pub entry_capture_slots: Vec<u32>,
    pub implementation_slot: Option<u32>,
    pub call_window_start: Option<u32>,
    pub call_window_argument_capacity: u32,
    pub edges: Vec<ColoredEdge>,
}

/// Deterministically color one logical implementation into Oop frame slots.
///
/// Version 1 deliberately makes every value produced in a PC part of that
/// PC's step-safepoint union. This is conservative but closes the rooting
/// contract before later allocator refinements. Entry locations are fixed;
/// all other values choose the lowest noninterfering slot. Call staging is a
/// disjoint reusable suffix so contiguous argument requirements cannot affect
/// language-value coloring.
pub fn color_slots(program: &LogicalProgram<'_>) -> Result<PhysicalLayout, LayoutError> {
    if program.blocks.is_empty()
        || program.entry != ProgramCounter(0)
        || program.blocks.len() > u32::MAX as usize
        || program.value_count as usize == 0
    {
        return Err(LayoutError::InvalidProgram);
    }
    for (index, block) in program.blocks.iter().enumerate() {
        if block.pc.0 as usize != index {
            return Err(LayoutError::InvalidProgram);
        }
    }
    let liveness = reference_liveness(program);
    let mut pc_values = Vec::with_capacity(program.blocks.len());
    for (block, live) in program.blocks.iter().zip(&liveness) {
        if block.pc != live.pc {
            return Err(LayoutError::InvalidProgram);
        }
        let mut values: BTreeSet<ValueId> =
            live.live_in.iter().chain(&live.live_out).copied().collect();
        for operation in &block.operations {
            values.insert(operation.result);
            match &operation.kind {
                OperationKind::Bind { source, .. } => {
                    values.insert(*source);
                }
                OperationKind::BlockLiteral { captures, .. } => {
                    values.extend(captures.iter().copied());
                }
                _ => {}
            }
        }
        match &block.terminator {
            Terminator::Call {
                receiver,
                arguments,
                result,
                ..
            } => {
                values.insert(*receiver);
                values.extend(arguments.iter().copied());
                values.insert(*result);
            }
            Terminator::TailCall {
                receiver,
                arguments,
                ..
            } => {
                values.insert(*receiver);
                values.extend(arguments.iter().copied());
            }
            Terminator::Return(value) => {
                values.insert(*value);
            }
            Terminator::Continue(_) => {}
        }
        if values.iter().any(|value| value.0 >= program.value_count) {
            return Err(LayoutError::InvalidProgram);
        }
        pc_values.push(values);
    }
    for block in &program.blocks {
        if let Terminator::Call {
            result, successor, ..
        } = block.terminator
        {
            let target = pc_values
                .get_mut(successor.0 as usize)
                .ok_or(LayoutError::InvalidProgram)?;
            target.insert(result);
        }
    }

    let mut entry_receiver = None;
    let mut entry_arguments = vec![None; program.argument_count as usize];
    let mut entry_captures = vec![None; program.capture_count as usize];
    let mut entry_implementation = None;
    for operation in &program.blocks[0].operations {
        match operation.kind {
            OperationKind::EntryReceiver => {
                if entry_receiver.replace(operation.result).is_some() {
                    return Err(LayoutError::InvalidProgram);
                }
            }
            OperationKind::EntryImplementation => {
                if entry_implementation.replace(operation.result).is_some() {
                    return Err(LayoutError::InvalidProgram);
                }
            }
            OperationKind::EntryArgument { index, .. } => {
                let Some(argument) = entry_arguments.get_mut(index as usize) else {
                    return Err(LayoutError::InvalidProgram);
                };
                if argument.replace(operation.result).is_some() {
                    return Err(LayoutError::InvalidProgram);
                }
            }
            OperationKind::EntryCapture { index, .. } => {
                let Some(capture) = entry_captures.get_mut(index as usize) else {
                    return Err(LayoutError::InvalidProgram);
                };
                if capture.replace(operation.result).is_some() {
                    return Err(LayoutError::InvalidProgram);
                }
            }
            _ => {}
        }
    }
    let entry_receiver = entry_receiver.ok_or(LayoutError::InvalidProgram)?;
    let entry_arguments = entry_arguments
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(LayoutError::InvalidProgram)?;
    let entry_captures = entry_captures
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(LayoutError::InvalidProgram)?;
    pc_values[0].insert(entry_receiver);
    pc_values[0].extend(entry_arguments.iter().copied());
    pc_values[0].extend(entry_captures.iter().copied());

    let value_count = program.value_count as usize;
    let mut interference = vec![BTreeSet::new(); value_count];
    for values in &pc_values {
        let values: Vec<_> = values.iter().copied().collect();
        for (position, left) in values.iter().enumerate() {
            for right in &values[position + 1..] {
                interference[left.0 as usize].insert(*right);
                interference[right.0 as usize].insert(*left);
            }
        }
    }
    let mut entry_values = vec![entry_receiver];
    entry_values.extend(entry_arguments.iter().copied());
    entry_values.extend(entry_captures.iter().copied());
    if let Some(implementation) = entry_implementation {
        entry_values.push(implementation);
    }
    for (position, left) in entry_values.iter().enumerate() {
        for right in &entry_values[position + 1..] {
            interference[left.0 as usize].insert(*right);
            interference[right.0 as usize].insert(*left);
        }
    }

    let mut colors = vec![None; value_count];
    colors[entry_receiver.0 as usize] = Some(0);
    for (index, value) in entry_arguments.iter().enumerate() {
        colors[value.0 as usize] =
            Some(u32::try_from(index + 1).map_err(|_| LayoutError::QuantityOverflow)?);
    }
    if let Some(implementation) = entry_implementation {
        colors[implementation.0 as usize] = Some(
            program
                .argument_count
                .checked_add(1)
                .ok_or(LayoutError::QuantityOverflow)?,
        );
    }
    let first_capture_slot = program
        .argument_count
        .checked_add(1)
        .and_then(|slot| slot.checked_add(u32::from(entry_implementation.is_some())))
        .ok_or(LayoutError::QuantityOverflow)?;
    for (index, value) in entry_captures.iter().enumerate() {
        colors[value.0 as usize] = Some(
            first_capture_slot
                .checked_add(u32::try_from(index).map_err(|_| LayoutError::QuantityOverflow)?)
                .ok_or(LayoutError::QuantityOverflow)?,
        );
    }
    for value in 0..value_count {
        if colors[value].is_some() {
            continue;
        }
        let mut color = 0u32;
        loop {
            let unavailable = interference[value].iter().any(|neighbor| {
                colors[neighbor.0 as usize].is_some_and(|candidate| candidate == color)
            });
            if !unavailable {
                colors[value] = Some(color);
                break;
            }
            color = color.checked_add(1).ok_or(LayoutError::QuantityOverflow)?;
        }
    }
    let value_slots: Vec<u32> = colors
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(LayoutError::InvalidProgram)?;
    let value_slot_count = value_slots
        .iter()
        .copied()
        .max()
        .and_then(|slot| slot.checked_add(1))
        .ok_or(LayoutError::InvalidProgram)?;
    let maximum_call_arity = program
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            Terminator::Call { arguments, .. } | Terminator::TailCall { arguments, .. } => {
                Some(arguments.len())
            }
            _ => None,
        })
        .max();
    let (call_window_start, call_window_argument_capacity, slot_count) = match maximum_call_arity {
        Some(arity) => {
            let arity = u32::try_from(arity).map_err(|_| LayoutError::QuantityOverflow)?;
            let slot_count = value_slot_count
                .checked_add(2)
                .and_then(|count| count.checked_add(arity))
                .ok_or(LayoutError::QuantityOverflow)?;
            (Some(value_slot_count), arity, slot_count)
        }
        None => (None, 0, value_slot_count),
    };
    let words_per_pc = slot_count
        .checked_add(63)
        .ok_or(LayoutError::QuantityOverflow)?
        / 64;
    let root_word_count = usize::try_from(words_per_pc)
        .ok()
        .and_then(|words| words.checked_mul(program.blocks.len()))
        .ok_or(LayoutError::QuantityOverflow)?;
    let mut root_bitmap_words = vec![0u64; root_word_count];
    let mut pc_value_locations = Vec::with_capacity(program.blocks.len());
    for (pc, values) in pc_values.iter().enumerate() {
        let mut locations = Vec::with_capacity(values.len());
        for value in values {
            let slot = value_slots[value.0 as usize];
            set_bitmap_slot(&mut root_bitmap_words, words_per_pc, pc, slot)?;
            locations.push((*value, slot));
        }
        if let Some(window) = call_window_start
            && matches!(
                program.blocks[pc].terminator,
                Terminator::Call { .. } | Terminator::TailCall { .. }
            )
        {
            let arity = match &program.blocks[pc].terminator {
                Terminator::Call { arguments, .. } | Terminator::TailCall { arguments, .. } => {
                    arguments.len() as u32
                }
                _ => unreachable!(),
            };
            for slot in window..window + 2 + arity {
                set_bitmap_slot(&mut root_bitmap_words, words_per_pc, pc, slot)?;
            }
        }
        pc_value_locations.push(locations);
    }

    let mut edges = Vec::new();
    for block in &program.blocks {
        let (target, call_result) = match block.terminator {
            Terminator::Call {
                result, successor, ..
            } => (successor, Some(result)),
            Terminator::Continue(successor) => (successor, None),
            Terminator::TailCall { .. } | Terminator::Return(_) => continue,
        };
        let target_index = target.0 as usize;
        let target_live_in: BTreeSet<_> = liveness[target_index].live_in.iter().copied().collect();
        let result_initialization_slot = call_result.map(|result| value_slots[result.0 as usize]);
        let mut preserved_slots = BTreeSet::new();
        for value in target_live_in {
            if call_result == Some(value) {
                continue;
            }
            if pc_values[block.pc.0 as usize].contains(&value) {
                preserved_slots.insert(value_slots[value.0 as usize]);
            }
        }
        let mut clear_bitmap_words = vec![0u64; words_per_pc as usize];
        for slot in 0..slot_count {
            if !preserved_slots.contains(&slot) && result_initialization_slot != Some(slot) {
                clear_bitmap_words[slot as usize / 64] |= 1 << (slot % 64);
            }
        }
        edges.push(ColoredEdge {
            source: block.pc,
            target,
            result_initialization_slot,
            copies: Vec::new(),
            clear_bitmap_words,
        });
    }

    let implementation_slot = entry_implementation.map(|value| value_slots[value.0 as usize]);
    let entry_capture_slots = entry_captures
        .iter()
        .map(|value| value_slots[value.0 as usize])
        .collect();
    Ok(PhysicalLayout {
        value_slots,
        pc_value_locations,
        root_bitmap_words,
        root_words_per_pc: words_per_pc,
        slot_count,
        entry_receiver_slot: 0,
        entry_argument_start: 1,
        entry_argument_count: program.argument_count,
        entry_capture_slots,
        implementation_slot,
        call_window_start,
        call_window_argument_capacity,
        edges,
    })
}

fn set_bitmap_slot(
    words: &mut [u64],
    words_per_pc: u32,
    pc: usize,
    slot: u32,
) -> Result<(), LayoutError> {
    let index = pc
        .checked_mul(words_per_pc as usize)
        .and_then(|base| base.checked_add(slot as usize / 64))
        .ok_or(LayoutError::QuantityOverflow)?;
    let word = words.get_mut(index).ok_or(LayoutError::InvalidProgram)?;
    *word |= 1 << (slot % 64);
    Ok(())
}

pub trait Oracle<'a, V> {
    type Error;

    fn literal(&mut self, literal: &Literal<'a>) -> Result<V, Self::Error>;
    fn global(&mut self, name: &'a [u8]) -> Result<V, Self::Error>;
    fn block(&mut self, block: &'a Block<'a>, captures: Vec<V>) -> Result<V, Self::Error>;
    fn portable_block(&mut self, program: &'a [u8], captures: Vec<V>) -> Result<V, Self::Error>;
    fn send(
        &mut self,
        receiver: V,
        selector: &'a [u8],
        arguments: Vec<V>,
    ) -> Result<V, Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterpretError<E> {
    ArgumentCount,
    InvalidProgram,
    Oracle(E),
}

/// Execute the uncolored logical form. This is intentionally not a runtime;
/// it is a deterministic semantic oracle for lowering and coloring tests.
pub fn interpret<'a, V: Clone, O: Oracle<'a, V>>(
    program: &'a LogicalProgram<'a>,
    receiver: V,
    arguments: &[V],
    oracle: &mut O,
) -> Result<V, InterpretError<O::Error>> {
    interpret_with_captures(program, receiver, arguments, &[], oracle)
}

pub fn interpret_with_captures<'a, V: Clone, O: Oracle<'a, V>>(
    program: &'a LogicalProgram<'a>,
    receiver: V,
    arguments: &[V],
    captures: &[V],
    oracle: &mut O,
) -> Result<V, InterpretError<O::Error>> {
    if arguments.len() != program.argument_count as usize
        || captures.len() != program.capture_count as usize
    {
        return Err(InterpretError::ArgumentCount);
    }
    let mut values: Vec<Option<V>> = vec![None; program.value_count as usize];
    let mut pc = program.entry;
    loop {
        let block = program
            .blocks
            .get(pc.0 as usize)
            .filter(|block| block.pc == pc)
            .ok_or(InterpretError::InvalidProgram)?;
        for operation in &block.operations {
            let value = match &operation.kind {
                OperationKind::EntryReceiver => receiver.clone(),
                OperationKind::EntryImplementation => {
                    return Err(InterpretError::InvalidProgram);
                }
                OperationKind::EntryArgument { index, .. } => arguments
                    .get(*index as usize)
                    .cloned()
                    .ok_or(InterpretError::InvalidProgram)?,
                OperationKind::EntryCapture { index, .. } => captures
                    .get(*index as usize)
                    .cloned()
                    .ok_or(InterpretError::InvalidProgram)?,
                OperationKind::Literal(literal) => {
                    oracle.literal(literal).map_err(InterpretError::Oracle)?
                }
                OperationKind::GlobalLookup(name) => {
                    oracle.global(name).map_err(InterpretError::Oracle)?
                }
                OperationKind::BlockLiteral { block, captures } => {
                    let captures = captures
                        .iter()
                        .map(|capture| value(&values, *capture).cloned())
                        .collect::<Result<Vec<_>, _>>()?;
                    oracle
                        .block(block, captures)
                        .map_err(InterpretError::Oracle)?
                }
                OperationKind::PortableBlockLiteral {
                    program, captures, ..
                } => {
                    let captures = captures
                        .iter()
                        .map(|capture| value(&values, *capture).cloned())
                        .collect::<Result<Vec<_>, _>>()?;
                    oracle
                        .portable_block(program, captures)
                        .map_err(InterpretError::Oracle)?
                }
                OperationKind::Bind { source, .. } => value(&values, *source)?.clone(),
            };
            set_value(&mut values, operation.result, value)?;
        }
        match &block.terminator {
            Terminator::Call {
                receiver,
                selector,
                arguments,
                result,
                successor,
                ..
            } => {
                let receiver = value(&values, *receiver)?.clone();
                let arguments = arguments
                    .iter()
                    .map(|argument| value(&values, *argument).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                let result_value = oracle
                    .send(receiver, selector, arguments)
                    .map_err(InterpretError::Oracle)?;
                set_value(&mut values, *result, result_value)?;
                pc = *successor;
            }
            Terminator::TailCall {
                receiver,
                selector,
                arguments,
                ..
            } => {
                let receiver = value(&values, *receiver)?.clone();
                let arguments = arguments
                    .iter()
                    .map(|argument| value(&values, *argument).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                return oracle
                    .send(receiver, selector, arguments)
                    .map_err(InterpretError::Oracle);
            }
            Terminator::Return(result) => return Ok(value(&values, *result)?.clone()),
            Terminator::Continue(successor) => pc = *successor,
        }
    }
}

/// Execute the same logical program through its colored slot and edge plan.
/// This is a compiler property oracle, not production Runtime code.
pub fn interpret_colored<'a, V: Clone, O: Oracle<'a, V>>(
    program: &'a LogicalProgram<'a>,
    layout: &PhysicalLayout,
    receiver: V,
    arguments: &[V],
    oracle: &mut O,
) -> Result<V, InterpretError<O::Error>> {
    interpret_colored_with_captures(program, layout, receiver, arguments, &[], oracle)
}

pub fn interpret_colored_with_captures<'a, V: Clone, O: Oracle<'a, V>>(
    program: &'a LogicalProgram<'a>,
    layout: &PhysicalLayout,
    receiver: V,
    arguments: &[V],
    captures: &[V],
    oracle: &mut O,
) -> Result<V, InterpretError<O::Error>> {
    if arguments.len() != program.argument_count as usize
        || captures.len() != program.capture_count as usize
        || layout.value_slots.len() != program.value_count as usize
        || layout.pc_value_locations.len() != program.blocks.len()
    {
        return Err(InterpretError::InvalidProgram);
    }
    let mut slots: Vec<Option<V>> = vec![None; layout.slot_count as usize];
    let mut pc = program.entry;
    loop {
        let block = program
            .blocks
            .get(pc.0 as usize)
            .filter(|block| block.pc == pc)
            .ok_or(InterpretError::InvalidProgram)?;
        for operation in &block.operations {
            let value = match &operation.kind {
                OperationKind::EntryReceiver => receiver.clone(),
                OperationKind::EntryImplementation => {
                    return Err(InterpretError::InvalidProgram);
                }
                OperationKind::EntryArgument { index, .. } => arguments
                    .get(*index as usize)
                    .cloned()
                    .ok_or(InterpretError::InvalidProgram)?,
                OperationKind::EntryCapture { index, .. } => captures
                    .get(*index as usize)
                    .cloned()
                    .ok_or(InterpretError::InvalidProgram)?,
                OperationKind::Literal(literal) => {
                    oracle.literal(literal).map_err(InterpretError::Oracle)?
                }
                OperationKind::GlobalLookup(name) => {
                    oracle.global(name).map_err(InterpretError::Oracle)?
                }
                OperationKind::BlockLiteral { block, captures } => {
                    let captures = captures
                        .iter()
                        .map(|capture| colored_value(&slots, layout, *capture).cloned())
                        .collect::<Result<Vec<_>, _>>()?;
                    oracle
                        .block(block, captures)
                        .map_err(InterpretError::Oracle)?
                }
                OperationKind::PortableBlockLiteral {
                    program, captures, ..
                } => {
                    let captures = captures
                        .iter()
                        .map(|capture| colored_value(&slots, layout, *capture).cloned())
                        .collect::<Result<Vec<_>, _>>()?;
                    oracle
                        .portable_block(program, captures)
                        .map_err(InterpretError::Oracle)?
                }
                OperationKind::Bind { source, .. } => {
                    colored_value(&slots, layout, *source)?.clone()
                }
            };
            let slot = *layout
                .value_slots
                .get(operation.result.0 as usize)
                .ok_or(InterpretError::InvalidProgram)? as usize;
            let destination = slots.get_mut(slot).ok_or(InterpretError::InvalidProgram)?;
            if destination.is_some() {
                return Err(InterpretError::InvalidProgram);
            }
            *destination = Some(value);
        }
        match &block.terminator {
            Terminator::Call {
                receiver,
                selector,
                arguments,
                result,
                successor,
                ..
            } => {
                let receiver = colored_value(&slots, layout, *receiver)?.clone();
                let arguments = arguments
                    .iter()
                    .map(|argument| colored_value(&slots, layout, *argument).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                let result_value = oracle
                    .send(receiver, selector, arguments)
                    .map_err(InterpretError::Oracle)?;
                let edge = layout
                    .edges
                    .iter()
                    .find(|edge| edge.source == pc && edge.target == *successor)
                    .ok_or(InterpretError::InvalidProgram)?;
                apply_colored_edge(&mut slots, edge)?;
                let result_slot = *layout
                    .value_slots
                    .get(result.0 as usize)
                    .ok_or(InterpretError::InvalidProgram)?
                    as usize;
                if edge.result_initialization_slot != Some(result_slot as u32) {
                    return Err(InterpretError::InvalidProgram);
                }
                *slots
                    .get_mut(result_slot)
                    .ok_or(InterpretError::InvalidProgram)? = Some(result_value);
                pc = *successor;
            }
            Terminator::TailCall {
                receiver,
                selector,
                arguments,
                ..
            } => {
                let receiver = colored_value(&slots, layout, *receiver)?.clone();
                let arguments = arguments
                    .iter()
                    .map(|argument| colored_value(&slots, layout, *argument).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                return oracle
                    .send(receiver, selector, arguments)
                    .map_err(InterpretError::Oracle);
            }
            Terminator::Return(result) => {
                return Ok(colored_value(&slots, layout, *result)?.clone());
            }
            Terminator::Continue(successor) => {
                let edge = layout
                    .edges
                    .iter()
                    .find(|edge| edge.source == pc && edge.target == *successor)
                    .ok_or(InterpretError::InvalidProgram)?;
                apply_colored_edge(&mut slots, edge)?;
                pc = *successor;
            }
        }
    }
}

fn colored_value<'a, V, E>(
    slots: &'a [Option<V>],
    layout: &PhysicalLayout,
    value: ValueId,
) -> Result<&'a V, InterpretError<E>> {
    layout
        .value_slots
        .get(value.0 as usize)
        .and_then(|slot| slots.get(*slot as usize))
        .and_then(Option::as_ref)
        .ok_or(InterpretError::InvalidProgram)
}

fn apply_colored_edge<V: Clone, E>(
    slots: &mut [Option<V>],
    edge: &ColoredEdge,
) -> Result<(), InterpretError<E>> {
    let copied = edge
        .copies
        .iter()
        .map(|copy| {
            slots
                .get(copy.source as usize)
                .and_then(Option::as_ref)
                .cloned()
                .ok_or(InterpretError::InvalidProgram)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (copy, value) in edge.copies.iter().zip(copied) {
        *slots
            .get_mut(copy.destination as usize)
            .ok_or(InterpretError::InvalidProgram)? = Some(value);
    }
    for (word_index, word) in edge.clear_bitmap_words.iter().copied().enumerate() {
        for bit in 0..64 {
            if word & (1 << bit) != 0 {
                let slot = word_index * 64 + bit;
                if let Some(value) = slots.get_mut(slot) {
                    *value = None;
                }
            }
        }
    }
    if let Some(result) = edge.result_initialization_slot {
        *slots
            .get_mut(result as usize)
            .ok_or(InterpretError::InvalidProgram)? = None;
    }
    Ok(())
}

fn value<V, E>(values: &[Option<V>], id: ValueId) -> Result<&V, InterpretError<E>> {
    values
        .get(id.0 as usize)
        .and_then(Option::as_ref)
        .ok_or(InterpretError::InvalidProgram)
}

fn set_value<V, E>(
    values: &mut [Option<V>],
    id: ValueId,
    value: V,
) -> Result<(), InterpretError<E>> {
    let slot = values
        .get_mut(id.0 as usize)
        .ok_or(InterpretError::InvalidProgram)?;
    if slot.is_some() {
        return Err(InterpretError::InvalidProgram);
    }
    *slot = Some(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use birnam_artifact::portable_program::{
        LetSpec, NodeSpec, ProgramSpec, StatementSpec, encode,
    };

    use super::*;

    fn send<'a>(receiver: Expr<'a>, selector: &'a [u8], args: Vec<Expr<'a>>) -> Expr<'a> {
        Expr::Send {
            receiver: Box::new(receiver),
            selector: Cow::Borrowed(selector),
            args,
            source: selector,
        }
    }

    fn selectors(program: &LogicalProgram<'_>) -> Vec<Vec<u8>> {
        program
            .blocks
            .iter()
            .filter_map(|block| match &block.terminator {
                Terminator::Call { selector, .. } | Terminator::TailCall { selector, .. } => {
                    Some(selector.to_vec())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn portable_lowering_uses_binding_identity_instead_of_repeated_names() {
        let bytes = encode(ProgramSpec {
            parameters: &[b"x"],
            captures: &[],
            lets: &[LetSpec {
                name: b"x",
                defining_statement: 0,
            }],
            statements: &[
                StatementSpec::Let {
                    root_node: 0,
                    let_index: 0,
                },
                StatementSpec::Expression { root_node: 1 },
            ],
            nodes: &[NodeSpec::Integer(9), NodeSpec::LexicalRead(0)],
        })
        .unwrap();

        let program = lower_portable_block(&bytes).unwrap();
        assert_eq!(program.binding_count, 2);
        assert!(matches!(
            program.blocks.last().unwrap().terminator,
            Terminator::Return(ValueId(1))
        ));
    }

    #[test]
    fn portable_lowering_preserves_float_bits() {
        let bits = 0x7ff8_0000_0000_1234;
        let bytes = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::FloatBits(bits)],
        })
        .unwrap();

        let program = lower_portable_block(&bytes).unwrap();
        assert!(matches!(
            program.blocks[0].operations[1].kind,
            OperationKind::Literal(Literal::FloatBits(actual)) if actual == bits
        ));
    }

    #[test]
    fn projected_bindings_are_top_level_blocks_with_canonical_keys_and_captures() {
        let program = Program {
            body: vec![
                Statement::Let {
                    name: b"prior",
                    value: Expr::SmallInt(5),
                },
                Statement::Let {
                    name: b"safe",
                    value: Expr::Block(Box::new(Block {
                        params: Vec::new(),
                        body: vec![Statement::Expr(Expr::SmallInt(99))],
                    })),
                },
                Statement::Let {
                    name: b"captured",
                    value: Expr::Block(Box::new(Block {
                        params: Vec::new(),
                        body: vec![Statement::Expr(Expr::Global(b"prior"))],
                    })),
                },
                Statement::Expr(send(
                    Expr::Global(b"factory"),
                    b"consume:",
                    vec![Expr::Block(Box::new(Block {
                        params: Vec::new(),
                        body: vec![Statement::Expr(Expr::SmallInt(1))],
                    }))],
                )),
            ],
        };

        let bindings = projected_block_bindings(&program);
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].statement_index, 1);
        assert_eq!(bindings[0].name, b"safe");
        assert_eq!(
            bindings[0].key.root_kind,
            InventoryRootKind::NonMethodProgram
        );
        assert_eq!(bindings[0].key.root_index, 0);
        assert_eq!(bindings[0].key.path, [0]);
        assert!(bindings[0].captures.is_empty());
        assert_eq!(bindings[1].statement_index, 2);
        assert_eq!(bindings[1].name, b"captured");
        assert_eq!(bindings[1].key.path, [1]);
        assert_eq!(bindings[1].captures, [b"prior".to_vec()]);
    }

    #[test]
    fn receiver_and_arguments_are_lowered_once_in_source_order() {
        let body = send(
            send(Expr::Global(b"factory"), b"makeReceiver", vec![]),
            b"foo:bar:",
            vec![
                send(Expr::Global(b"factory"), b"makeFirst", vec![]),
                send(Expr::Global(b"factory"), b"makeSecond", vec![]),
            ],
        );
        let block = Block {
            params: vec![],
            body: vec![Statement::Expr(body)],
        };
        let program = lower_block(&block).unwrap();
        assert_eq!(
            selectors(&program),
            [
                b"makeReceiver".to_vec(),
                b"makeFirst".to_vec(),
                b"makeSecond".to_vec(),
                b"foo:bar:".to_vec(),
            ]
        );
        assert!(matches!(
            program.blocks.last().unwrap().terminator,
            Terminator::TailCall { .. }
        ));
        assert_eq!(program.call_site_count, 4);
    }

    #[test]
    fn non_tail_send_resumes_and_final_send_tail_calls() {
        let block = Block {
            params: vec![],
            body: vec![
                Statement::Expr(send(Expr::Global(b"first"), b"run", vec![])),
                Statement::Expr(send(Expr::Global(b"second"), b"finish", vec![])),
            ],
        };
        let program = lower_block(&block).unwrap();
        assert!(matches!(
            program.blocks[0].terminator,
            Terminator::Call {
                successor: ProgramCounter(1),
                ..
            }
        ));
        assert!(matches!(
            program.blocks[1].terminator,
            Terminator::TailCall { .. }
        ));
    }

    #[test]
    fn final_binding_keeps_the_send_non_tail_and_has_a_distinct_value() {
        let block = Block {
            params: vec![],
            body: vec![Statement::Let {
                name: b"answer",
                value: send(Expr::Global(b"worker"), b"answer", vec![]),
            }],
        };
        let program = lower_block(&block).unwrap();
        let Terminator::Call { result, .. } = program.blocks[0].terminator else {
            panic!("send feeding a binding must resume");
        };
        let Operation {
            result: binding_value,
            kind: OperationKind::Bind { source, .. },
        } = &program.blocks[1].operations[0]
        else {
            panic!("successor must establish the lexical binding");
        };
        assert_eq!(*source, result);
        assert_ne!(*binding_value, result);
        assert!(matches!(
            program.blocks[1].terminator,
            Terminator::Return(value) if value == *binding_value
        ));
    }

    #[test]
    fn shadowed_bindings_and_repeated_lowering_are_deterministic() {
        let block = Block {
            params: vec![b"item"],
            body: vec![
                Statement::Let {
                    name: b"item",
                    value: Expr::SmallInt(1),
                },
                Statement::Let {
                    name: b"item",
                    value: Expr::SmallInt(2),
                },
                Statement::Expr(Expr::Global(b"item")),
            ],
        };
        let first = lower_block(&block).unwrap();
        let second = lower_block(&block).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.binding_count, 3);
        let bindings: Vec<_> = first.blocks[0]
            .operations
            .iter()
            .filter_map(|operation| match operation.kind {
                OperationKind::EntryArgument { binding, .. }
                | OperationKind::Bind { binding, .. } => Some(binding),
                _ => None,
            })
            .collect();
        assert_eq!(bindings, [BindingId(0), BindingId(1), BindingId(2)]);
    }

    #[test]
    fn long_non_send_regions_split_with_continue() {
        let block = Block {
            params: vec![],
            body: (0..MAX_OPERATIONS_PER_BLOCK + 1)
                .map(|value| Statement::Expr(Expr::SmallInt(value as i64)))
                .collect(),
        };
        let program = lower_block(&block).unwrap();
        assert_eq!(program.blocks.len(), 2);
        assert_eq!(program.blocks[0].operations.len(), MAX_OPERATIONS_PER_BLOCK);
        assert!(matches!(
            program.blocks[0].terminator,
            Terminator::Continue(ProgramCounter(1))
        ));
        assert!(matches!(
            program.blocks[1].terminator,
            Terminator::Return(_)
        ));
    }

    #[derive(Default)]
    struct TraceOracle {
        events: Vec<String>,
    }

    impl<'a> Oracle<'a, String> for TraceOracle {
        type Error = ();

        fn literal(&mut self, literal: &Literal<'a>) -> Result<String, Self::Error> {
            Ok(format!("{literal:?}"))
        }

        fn global(&mut self, name: &'a [u8]) -> Result<String, Self::Error> {
            let name = String::from_utf8_lossy(name).into_owned();
            self.events.push(format!("global:{name}"));
            Ok(name)
        }

        fn block(
            &mut self,
            _block: &'a Block<'a>,
            captures: Vec<String>,
        ) -> Result<String, Self::Error> {
            self.events.push(format!("block:{}", captures.join(",")));
            Ok("block".into())
        }

        fn portable_block(
            &mut self,
            _program: &'a [u8],
            captures: Vec<String>,
        ) -> Result<String, Self::Error> {
            self.events
                .push(format!("portable-block:{}", captures.join(",")));
            Ok("portable-block".into())
        }

        fn send(
            &mut self,
            receiver: String,
            selector: &'a [u8],
            arguments: Vec<String>,
        ) -> Result<String, Self::Error> {
            let selector = String::from_utf8_lossy(selector);
            self.events.push(format!(
                "send:{receiver}:{selector}:{}",
                arguments.join(",")
            ));
            Ok(selector.into_owned())
        }
    }

    #[test]
    fn uncolored_oracle_observes_the_same_recursive_send_order() {
        let body = send(
            send(Expr::Global(b"factory"), b"receiver", vec![]),
            b"outer:",
            vec![send(Expr::Global(b"factory"), b"argument", vec![])],
        );
        let block = Block {
            params: vec![],
            body: vec![Statement::Expr(body)],
        };
        let program = lower_block(&block).unwrap();
        let mut oracle = TraceOracle::default();
        let result = interpret(&program, "self".into(), &[], &mut oracle).unwrap();
        assert_eq!(result, "outer:");
        assert_eq!(
            oracle.events,
            [
                "global:factory",
                "send:factory:receiver:",
                "global:factory",
                "send:factory:argument:",
                "send:receiver:outer::argument"
            ]
        );
    }

    #[test]
    fn liveness_keeps_a_pre_call_value_used_after_resume() {
        let block = Block {
            params: vec![b"kept"],
            body: vec![
                Statement::Expr(send(Expr::Global(b"worker"), b"run", vec![])),
                Statement::Expr(Expr::Global(b"kept")),
            ],
        };
        let program = lower_block(&block).unwrap();
        let liveness = reference_liveness(&program);
        let kept = program.blocks[0]
            .operations
            .iter()
            .find_map(|operation| match operation.kind {
                OperationKind::EntryArgument { name: b"kept", .. } => Some(operation.result),
                _ => None,
            })
            .unwrap();
        assert!(liveness[0].live_out.contains(&kept));
    }

    #[test]
    fn coloring_is_deterministic_and_reserves_one_call_window() {
        let block = Block {
            params: vec![b"left", b"right"],
            body: vec![
                Statement::Expr(send(
                    Expr::Global(b"left"),
                    b"combine:with:",
                    vec![Expr::Global(b"right"), Expr::SmallInt(1)],
                )),
                Statement::Expr(Expr::SmallInt(2)),
            ],
        };
        let program = lower_block(&block).unwrap();
        let first = color_slots(&program).unwrap();
        let second = color_slots(&program).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.entry_receiver_slot, 0);
        assert_eq!(first.entry_argument_start, 1);
        assert_eq!(first.entry_argument_count, 2);
        assert_eq!(first.call_window_argument_capacity, 2);
        let window = first.call_window_start.unwrap();
        assert_eq!(first.slot_count, window + 4);
        for slot in window..window + 4 {
            assert_ne!(
                first.root_bitmap_words[slot as usize / 64] & (1 << (slot % 64)),
                0
            );
        }
    }

    #[test]
    fn coloring_reuses_dead_entry_slots_and_roots_call_results_on_resume() {
        let block = Block {
            params: vec![b"worker"],
            body: vec![
                Statement::Expr(send(Expr::Global(b"worker"), b"run", vec![])),
                Statement::Expr(Expr::SmallInt(7)),
            ],
        };
        let program = lower_block(&block).unwrap();
        let layout = color_slots(&program).unwrap();
        let Terminator::Call {
            result, successor, ..
        } = program.blocks[0].terminator
        else {
            panic!("first send must resume");
        };
        let successor_value = program.blocks[1].operations[0].result;
        assert_eq!(layout.value_slots[successor_value.0 as usize], 0);
        let result_slot = layout.value_slots[result.0 as usize];
        assert!(layout.pc_value_locations[successor.0 as usize].contains(&(result, result_slot)));
        assert_eq!(
            layout.edges[0].result_initialization_slot,
            Some(result_slot)
        );
        assert_eq!(
            layout.edges[0].clear_bitmap_words[result_slot as usize / 64]
                & (1 << (result_slot % 64)),
            0
        );
    }

    #[test]
    fn colored_execution_matches_the_uncolored_oracle() {
        let block = Block {
            params: vec![b"kept"],
            body: vec![
                Statement::Expr(send(
                    send(Expr::Global(b"factory"), b"receiver", vec![]),
                    b"combine:",
                    vec![Expr::Global(b"kept")],
                )),
                Statement::Expr(send(Expr::Global(b"kept"), b"finish", vec![])),
            ],
        };
        let program = lower_block(&block).unwrap();
        let layout = color_slots(&program).unwrap();
        let mut uncolored = TraceOracle::default();
        let mut colored = TraceOracle::default();
        let expected = interpret(
            &program,
            "self".into(),
            &["argument".into()],
            &mut uncolored,
        )
        .unwrap();
        let actual = interpret_colored(
            &program,
            &layout,
            "self".into(),
            &["argument".into()],
            &mut colored,
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(colored.events, uncolored.events);
    }

    #[test]
    fn implementation_inventory_uses_operation_positions_and_lexical_block_paths() {
        let nested = Expr::Block(Box::new(Block {
            params: vec![],
            body: vec![Statement::Expr(Expr::Block(Box::new(Block {
                params: vec![],
                body: vec![Statement::Expr(Expr::Nil)],
            })))],
        }));
        let define = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![Expr::Symbol(b"run"), Expr::Global(b"Target"), nested],
            source: b"define",
        };
        let remove = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"removeMethod:on:"),
            args: vec![Expr::Symbol(b"old"), Expr::Global(b"Target")],
            source: b"remove",
        };
        let second_define = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![
                Expr::Symbol(b"other"),
                Expr::Global(b"Target"),
                Expr::Global(b"implementation"),
            ],
            source: b"second",
        };
        let program = Program {
            body: vec![
                Statement::Expr(Expr::Block(Box::new(Block {
                    params: vec![],
                    body: vec![Statement::Expr(Expr::Nil)],
                }))),
                Statement::Expr(define),
                Statement::Expr(remove),
                Statement::Expr(second_define),
            ],
        };
        assert_eq!(
            implementation_inventory(&program),
            [
                ImplementationKey {
                    root_kind: InventoryRootKind::NonMethodProgram,
                    root_index: 0,
                    path: vec![0],
                    implementation_kind: InventoryImplementationKind::BlockBody,
                },
                ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 0,
                    path: vec![],
                    implementation_kind: InventoryImplementationKind::NamedAdapter,
                },
                ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 0,
                    path: vec![0],
                    implementation_kind: InventoryImplementationKind::BlockBody,
                },
                ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 0,
                    path: vec![0, 0],
                    implementation_kind: InventoryImplementationKind::BlockBody,
                },
                ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 2,
                    path: vec![],
                    implementation_kind: InventoryImplementationKind::NamedAdapter,
                },
            ]
        );
        for (key, block) in implementation_inventory_with_blocks(&program) {
            match key.implementation_kind {
                InventoryImplementationKind::NamedAdapter => assert!(block.is_none()),
                InventoryImplementationKind::BlockBody => {
                    let resolved = resolve_block_implementation(&program, &key).unwrap();
                    assert!(std::ptr::eq(block.unwrap(), resolved.block));
                }
            }
        }
    }

    #[test]
    fn stable_block_resolution_propagates_enclosing_captures_in_binding_order() {
        let nested = Expr::Block(Box::new(Block {
            params: vec![],
            body: vec![Statement::Expr(send(
                Expr::Global(b"self"),
                b"pair:",
                vec![Expr::Global(b"moduleValue")],
            ))],
        }));
        let define = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![
                Expr::Symbol(b"make"),
                Expr::Global(b"Target"),
                Expr::Block(Box::new(Block {
                    params: vec![b"self"],
                    body: vec![
                        // Skipping this subtree must not consume the following
                        // sibling's ordinal or change its enclosing captures.
                        Statement::Expr(Expr::Block(Box::new(Block {
                            params: vec![b"unrelated"],
                            body: vec![Statement::Expr(Expr::Block(Box::new(Block {
                                params: vec![],
                                body: vec![Statement::Expr(Expr::Global(b"unrelated"))],
                            })))],
                        }))),
                        Statement::Expr(nested),
                    ],
                })),
            ],
            source: b"define",
        };
        let program = Program {
            body: vec![
                Statement::Let {
                    name: b"moduleValue",
                    value: Expr::SmallInt(7),
                },
                Statement::Expr(define),
            ],
        };
        let outer = resolve_block_implementation(
            &program,
            &ImplementationKey {
                root_kind: InventoryRootKind::MethodOperation,
                root_index: 0,
                path: vec![0],
                implementation_kind: InventoryImplementationKind::BlockBody,
            },
        )
        .unwrap();
        assert_eq!(outer.captures, [b"moduleValue".to_vec()]);
        let inner = resolve_block_implementation(
            &program,
            &ImplementationKey {
                root_kind: InventoryRootKind::MethodOperation,
                root_index: 0,
                path: vec![0, 1],
                implementation_kind: InventoryImplementationKind::BlockBody,
            },
        )
        .unwrap();
        assert_eq!(inner.captures, [b"self".to_vec(), b"moduleValue".to_vec()]);
    }
}
