use crate::abi::{Method, Oop, SLOT_RAW};
use crate::heap::Permanence;
use crate::runtime::{Runtime, with_active_if_present};
use core::mem::size_of;
use core::ptr;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const CODE_OBJECT_MAGIC: u64 = 0x4249_524e_434f_424a;
const NODE_MAGIC: u64 = 0x4249_524e_434f_4445;
const STATEMENT_MAGIC: u64 = 0x4249_524e_5354_4d54;
const PROGRAM_MAGIC: u64 = 0x4249_524e_5052_4f47;
const BINDING_MAGIC: u64 = 0x4249_524e_4249_4e44;

#[derive(Clone, Copy)]
enum ConstructorFailure {
    TypeMismatch,
    InvalidPortableProgram,
    PortableProgramLimit,
}

fn constructor_error(runtime: &Runtime, failure: ConstructorFailure, message: &[u8]) -> Oop {
    let roots = runtime.temporary_root_scope();
    let factory = runtime
        .import(b"Error")
        .unwrap_or_else(|_| crate::boundary::fatal_invariant("static Error import contains NUL"));
    if factory.is_null() {
        crate::boundary::fatal_invariant("Portable Code loaded before Error")
    }
    runtime.push_temporary_root(factory);
    let kind = match failure {
        ConstructorFailure::TypeMismatch => b"typeMismatch".as_slice(),
        ConstructorFailure::InvalidPortableProgram => b"invalidPortableProgram".as_slice(),
        ConstructorFailure::PortableProgramLimit => b"portableProgramLimit".as_slice(),
    };
    let kind = runtime.intern(kind).unwrap_or_else(|_| {
        crate::boundary::fatal_invariant("static portable constructor Error kind contains NUL")
    });
    runtime.push_temporary_root(kind);
    let message = runtime.make_string(message);
    runtime.push_temporary_root(message);
    let selector = runtime.intern(b"kind:message:").unwrap_or_else(|_| {
        crate::boundary::fatal_invariant("static Error factory selector contains NUL")
    });
    let error = runtime.send_slice(factory, selector, &[kind, message]);
    if error.is_null() || error == runtime.canonical().nil {
        crate::boundary::fatal_invariant("Error factory rejected a portable constructor failure")
    }
    drop(roots);
    error
}

fn type_mismatch(runtime: &Runtime, message: &[u8]) -> Oop {
    constructor_error(runtime, ConstructorFailure::TypeMismatch, message)
}

fn invalid_program(runtime: &Runtime, message: &[u8]) -> Oop {
    constructor_error(runtime, ConstructorFailure::InvalidPortableProgram, message)
}

fn program_limit(runtime: &Runtime, message: &[u8]) -> Oop {
    constructor_error(runtime, ConstructorFailure::PortableProgramLimit, message)
}

fn construction_failure(runtime: &Runtime, failure: ConstructorFailure, message: &[u8]) -> Oop {
    constructor_error(runtime, failure, message)
}

#[repr(C)]
struct CodeObjectState {
    magic: u64,
    program: Oop,
    provenance: Oop,
}

enum Singleton {
    Nil,
    True,
    False,
}

enum NodeValue {
    Integer(i64),
    FloatBits([u8; 8]),
    String(Vec<u8>),
    Symbol(Vec<u8>),
    Singleton(Singleton),
    LexicalRead(u32),
    GlobalRead(Vec<u8>),
    Send {
        selector: Vec<u8>,
        operands: Vec<Oop>,
    },
    Block {
        program: Oop,
        captures: Vec<u32>,
    },
}

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

fn append_node_tree(
    runtime: &Runtime,
    root: Oop,
    nodes: &mut Vec<OwnedNode>,
) -> Result<u32, ConstructorFailure> {
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
                    return Err(ConstructorFailure::PortableProgramLimit);
                }
                let state =
                    unsafe { node_state(node, runtime) }.ok_or(ConstructorFailure::TypeMismatch)?;
                let owned = match &state.value {
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
                            return Err(ConstructorFailure::InvalidPortableProgram);
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
                        let nested = unsafe { program_state(*program, runtime) }
                            .ok_or(ConstructorFailure::InvalidPortableProgram)?;
                        Some(OwnedNode::Block {
                            program: nested.bytes.to_vec(),
                            captures: captures.clone(),
                        })
                    }
                };
                if let Some(owned) = owned {
                    let index = u32::try_from(nodes.len())
                        .map_err(|_| ConstructorFailure::PortableProgramLimit)?;
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
                    return Err(ConstructorFailure::InvalidPortableProgram);
                }
                let operands = results.split_off(results.len() - operand_count);
                let index = u32::try_from(nodes.len())
                    .map_err(|_| ConstructorFailure::PortableProgramLimit)?;
                nodes.push(OwnedNode::Send { selector, operands });
                results.push(index);
            }
        }
    }
    if results.len() == 1 {
        results
            .pop()
            .ok_or(ConstructorFailure::InvalidPortableProgram)
    } else {
        Err(ConstructorFailure::InvalidPortableProgram)
    }
}

fn copy_names(runtime: &Runtime, values: &[Oop]) -> Result<Vec<Vec<u8>>, ConstructorFailure> {
    let mut names = Vec::with_capacity(values.len());
    for value in values {
        let bytes = runtime
            .string_bytes(*value)
            .ok_or(ConstructorFailure::TypeMismatch)?;
        if bytes.is_empty() || bytes.contains(&0) {
            return Err(ConstructorFailure::InvalidPortableProgram);
        }
        if bytes.len() > 1 << 20 {
            return Err(ConstructorFailure::PortableProgramLimit);
        }
        names.push(bytes.to_vec());
    }
    Ok(names)
}

fn encode_program(
    runtime: &Runtime,
    parameter_values: &[Oop],
    capture_values: &[Oop],
    statement_values: &[Oop],
) -> Result<Vec<u8>, ConstructorFailure> {
    use birnam_artifact::portable_program::{LetSpec, ProgramSpec, StatementSpec};

    let parameters = copy_names(runtime, parameter_values)?;
    let captures = copy_names(runtime, capture_values)?;
    let mut owned_lets = Vec::<(Vec<u8>, u32)>::new();
    let mut statements = Vec::<StatementSpec>::with_capacity(statement_values.len());
    let mut nodes = Vec::<OwnedNode>::new();
    for (statement_index, value) in statement_values.iter().enumerate() {
        let state =
            unsafe { statement_state(*value, runtime) }.ok_or(ConstructorFailure::TypeMismatch)?;
        let (expression, let_name) = match &state.value {
            StatementValue::Expression(expression) => (*expression, None),
            StatementValue::Let { name, expression } => (*expression, Some(name.clone())),
        };
        let root_node = append_node_tree(runtime, expression, &mut nodes)?;
        if let Some(name) = let_name {
            let let_index = u32::try_from(owned_lets.len())
                .map_err(|_| ConstructorFailure::PortableProgramLimit)?;
            owned_lets.push((
                name,
                u32::try_from(statement_index)
                    .map_err(|_| ConstructorFailure::PortableProgramLimit)?,
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
        birnam_artifact::portable_program::Error::Quantity => {
            ConstructorFailure::PortableProgramLimit
        }
        birnam_artifact::portable_program::Error::Format
        | birnam_artifact::portable_program::Error::Name => {
            ConstructorFailure::InvalidPortableProgram
        }
    })
}

unsafe extern "C" fn node_global_read_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return type_mismatch(runtime, b"globalRead: needs one Symbol");
        }
        // SAFETY: count is exactly one and the Method ABI supplies that entry.
        let value = unsafe { args.read() };
        let Some(bytes) = runtime.symbol_bytes(value) else {
            return type_mismatch(runtime, b"globalRead: needs a Symbol");
        };
        if bytes.is_empty() {
            return invalid_program(runtime, b"global name must not be empty");
        }
        if bytes.len() > 1 << 20 {
            return program_limit(runtime, b"global name exceeds the PortableProgram limit");
        }
        let bytes = bytes.to_vec();
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        unsafe { allocate_node(runtime, vtable, NodeValue::GlobalRead(bytes)) }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_lexical_read_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return type_mismatch(runtime, b"lexicalReadAt: needs one Integer");
        }
        // SAFETY: count is exactly one and the Method ABI supplies that entry.
        let value = unsafe { args.read() };
        if !runtime.is_integer(value) {
            return type_mismatch(runtime, b"lexicalReadAt: needs an Integer");
        }
        let Ok(index) = u32::try_from(runtime.integer_value(value)) else {
            return invalid_program(runtime, b"lexical binding index is outside u32");
        };
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        unsafe { allocate_node(runtime, vtable, NodeValue::LexicalRead(index)) }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe fn singleton_new(receiver: Oop, count: usize, value: Singleton) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return type_mismatch(runtime, b"literal constructor takes no arguments");
        }
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        unsafe { allocate_node(runtime, vtable, NodeValue::Singleton(value)) }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_nil_new(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    unsafe { singleton_new(receiver, count, Singleton::Nil) }
}

unsafe extern "C" fn node_true_new(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    unsafe { singleton_new(receiver, count, Singleton::True) }
}

unsafe extern "C" fn node_false_new(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    unsafe { singleton_new(receiver, count, Singleton::False) }
}

unsafe extern "C" fn node_symbol_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return type_mismatch(runtime, b"symbol: needs one Symbol");
        }
        // SAFETY: count is exactly one and the Method ABI supplies that entry.
        let value = unsafe { args.read() };
        let Some(bytes) = runtime.symbol_bytes(value) else {
            return type_mismatch(runtime, b"symbol: needs a Symbol");
        };
        if bytes.is_empty() {
            return invalid_program(runtime, b"Symbol literal name must not be empty");
        }
        if bytes.len() > 1 << 20 {
            return program_limit(runtime, b"Symbol literal exceeds the PortableProgram limit");
        }
        let bytes = bytes.to_vec();
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        unsafe { allocate_node(runtime, vtable, NodeValue::Symbol(bytes)) }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_string_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return type_mismatch(runtime, b"string: needs one String");
        }
        // SAFETY: count is exactly one and the Method ABI supplies that entry.
        let value = unsafe { args.read() };
        let Some(bytes) = runtime.string_bytes(value) else {
            return type_mismatch(runtime, b"string: needs a String");
        };
        if bytes.len() > 16 << 20 {
            return program_limit(runtime, b"String literal exceeds the PortableProgram limit");
        }
        let bytes = bytes.to_vec();
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        unsafe { allocate_node(runtime, vtable, NodeValue::String(bytes)) }
    })
    .unwrap_or(core::ptr::null_mut())
}

#[repr(C)]
struct NodeState {
    magic: u64,
    value: NodeValue,
}

enum StatementValue {
    Expression(Oop),
    Let { name: Vec<u8>, expression: Oop },
}

#[repr(C)]
struct StatementState {
    magic: u64,
    value: StatementValue,
}

#[repr(C)]
struct ProgramState {
    magic: u64,
    bytes: birnam_artifact::portable_program::ValidatedProgram,
}

#[repr(C)]
struct BindingState {
    magic: u64,
    program: Oop,
    index: u32,
}

unsafe fn binding_state(receiver: Oop, runtime: &Runtime) -> Option<&'static BindingState> {
    if runtime.payload_size(receiver) != Some(size_of::<BindingState>())
        || unsafe { receiver.cast::<u64>().read() } != BINDING_MAGIC
    {
        return None;
    }
    Some(unsafe { &*receiver.cast::<BindingState>() })
}

unsafe extern "C" fn trace_binding(object: Oop, mark: crate::abi::MarkFn) {
    let _ = with_active_if_present(|runtime| {
        let Some(state) = (unsafe { binding_state(object, runtime) }) else {
            return;
        };
        unsafe { mark(state.program) };
    });
}

unsafe fn allocate_binding(runtime: &Runtime, program: Oop, index: u32) -> Option<Oop> {
    let vtable = runtime.import(b"CodeLexicalBindingVt").ok()?;
    if vtable.is_null() {
        return None;
    }
    let binding = unsafe { runtime.allocate_instance_active(vtable, size_of::<BindingState>()) };
    unsafe {
        binding.cast::<BindingState>().write(BindingState {
            magic: BINDING_MAGIC,
            program,
            index,
        })
    };
    Some(binding)
}

fn inspected_binding(
    runtime: &Runtime,
    receiver: Oop,
) -> Option<(
    &'static BindingState,
    birnam_artifact::portable_program::Binding<'static>,
)> {
    let state = unsafe { binding_state(receiver, runtime) }?;
    let program_state: &'static ProgramState = unsafe { program_state(state.program, runtime) }?;
    let program: birnam_artifact::portable_program::Program<'static> = program_state.bytes.view();
    let binding = program.binding(state.index as usize)?;
    Some((state, binding))
}

unsafe extern "C" fn binding_kind(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some((_, binding)) = inspected_binding(runtime, receiver) else {
            return runtime.canonical().nil;
        };
        let kind = match binding.kind {
            birnam_artifact::portable_program::BindingKind::Parameter => b"parameter".as_slice(),
            birnam_artifact::portable_program::BindingKind::Capture => b"capture".as_slice(),
            birnam_artifact::portable_program::BindingKind::Let => b"let".as_slice(),
        };
        runtime.intern(kind).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static CodeLexicalBinding kind contains NUL")
        })
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn binding_index(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { binding_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        runtime.make_integer(i64::from(state.index))
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn binding_ordinal(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some((_, binding)) = inspected_binding(runtime, receiver) else {
            return runtime.canonical().nil;
        };
        runtime.make_integer(i64::from(binding.ordinal))
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn binding_name(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some((_, binding)) = inspected_binding(runtime, receiver) else {
            return runtime.canonical().nil;
        };
        let name = binding.name.to_vec();
        unsafe {
            runtime.make_string_with_finalizer_active(&name, crate::public_api::string_finalizer)
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn binding_defining_statement(
    receiver: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some((_, binding)) = inspected_binding(runtime, receiver) else {
            return runtime.canonical().nil;
        };
        binding
            .defining_statement
            .map(|index| runtime.make_integer(i64::from(index)))
            .unwrap_or(runtime.canonical().nil)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe fn code_object_state(receiver: Oop, runtime: &Runtime) -> Option<&'static CodeObjectState> {
    if runtime.payload_size(receiver) != Some(size_of::<CodeObjectState>())
        || unsafe { receiver.cast::<u64>().read() } != CODE_OBJECT_MAGIC
    {
        return None;
    }
    Some(unsafe { &*receiver.cast::<CodeObjectState>() })
}

unsafe fn allocate_code_object(
    runtime: &Runtime,
    vtable: Oop,
    program: Oop,
    provenance: Oop,
) -> Oop {
    let object = unsafe { runtime.allocate_instance_active(vtable, size_of::<CodeObjectState>()) };
    unsafe {
        object.cast::<CodeObjectState>().write(CodeObjectState {
            magic: CODE_OBJECT_MAGIC,
            program,
            provenance,
        })
    };
    object
}

unsafe extern "C" fn trace_code_object(object: Oop, mark: crate::abi::MarkFn) {
    let _ = with_active_if_present(|runtime| {
        let Some(state) = (unsafe { code_object_state(object, runtime) }) else {
            return;
        };
        unsafe {
            mark(state.program);
            mark(state.provenance);
        }
    });
}

fn construct_code_object(runtime: &Runtime, receiver: Oop, program: Oop, provenance: Oop) -> Oop {
    if unsafe { program_state(program, runtime) }.is_none() {
        return type_mismatch(runtime, b"CodeObject program must be a CodeProgram");
    }
    let roots = runtime.temporary_root_scope();
    runtime.push_temporary_root(program);
    runtime.push_temporary_root(provenance);
    let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
    let object = unsafe { allocate_code_object(runtime, vtable, program, provenance) };
    drop(roots);
    object
}

unsafe extern "C" fn code_object_from_program(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return type_mismatch(runtime, b"fromProgram: needs one CodeProgram");
        }
        construct_code_object(
            runtime,
            receiver,
            unsafe { args.read() },
            runtime.canonical().nil,
        )
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_from_program_provenance(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    with_active_if_present(|runtime| {
        if count != 2 || args.is_null() {
            return type_mismatch(runtime, b"fromProgram:provenance: needs two arguments");
        }
        let arguments = unsafe { core::slice::from_raw_parts(args, count) };
        construct_code_object(runtime, receiver, arguments[0], arguments[1])
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_format(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 || unsafe { code_object_state(receiver, runtime) }.is_none() {
            return runtime.canonical().nil;
        }
        runtime.intern(b"portableProgramV1").unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static CodeObject format contains NUL")
        })
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_arity(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { code_object_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let Some(program) = (unsafe { program_state(state.program, runtime) }) else {
            crate::boundary::fatal_invariant("CodeObject lost its admitted CodeProgram")
        };
        let program = program.bytes.view();
        runtime.make_integer(i64::from(program.parameter_count))
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_program(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        unsafe { code_object_state(receiver, runtime) }
            .map(|state| state.program)
            .unwrap_or(runtime.canonical().nil)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_capture_schema(
    receiver: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let roots = runtime.temporary_root_scope();
        runtime.push_temporary_root(receiver);
        let Some(state) = (unsafe { code_object_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let Some(program_state) = (unsafe { program_state(state.program, runtime) }) else {
            crate::boundary::fatal_invariant("CodeObject lost its admitted CodeProgram")
        };
        let program = program_state.bytes.view();
        let Ok(vector_prototype) = runtime.import(b"Vector") else {
            return runtime.canonical().nil;
        };
        if vector_prototype.is_null() {
            return runtime.canonical().nil;
        }
        let new = runtime.intern(b"new").unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static Vector new selector contains NUL")
        });
        let add = runtime.intern(b"add:").unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static Vector add selector contains NUL")
        });
        let mut vector = runtime.send_slice(vector_prototype, new, &[]);
        if vector.is_null() || vector == runtime.canonical().nil {
            return runtime.canonical().nil;
        }
        runtime.push_temporary_root(vector);
        for ordinal in 0..program.capture_count {
            let Some(index) = program.parameter_count.checked_add(ordinal) else {
                crate::boundary::fatal_invariant("admitted capture binding index overflowed")
            };
            let Some(binding) = (unsafe { allocate_binding(runtime, state.program, index) }) else {
                return runtime.canonical().nil;
            };
            runtime.push_temporary_root(binding);
            vector = runtime.send_slice(vector, add, &[binding]);
            if vector.is_null() || vector == runtime.canonical().nil {
                return runtime.canonical().nil;
            }
            runtime.push_temporary_root(vector);
        }
        drop(roots);
        vector
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_digest(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { code_object_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let Some(program) = (unsafe { program_state(state.program, runtime) }) else {
            crate::boundary::fatal_invariant("CodeObject lost its admitted CodeProgram")
        };
        let digest: [u8; 32] = Sha256::digest(&program.bytes).into();
        unsafe {
            runtime.make_string_with_finalizer_active(&digest, crate::public_api::string_finalizer)
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_source_provenance(
    receiver: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        unsafe { code_object_state(receiver, runtime) }
            .map(|state| state.provenance)
            .unwrap_or(runtime.canonical().nil)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_with_program(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return type_mismatch(runtime, b"withProgram: needs one CodeProgram");
        }
        let Some(state) = (unsafe { code_object_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        construct_code_object(runtime, receiver, unsafe { args.read() }, state.provenance)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn code_object_without_source_provenance(
    receiver: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { code_object_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        construct_code_object(runtime, receiver, state.program, runtime.canonical().nil)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe fn program_state(receiver: Oop, runtime: &Runtime) -> Option<&'static ProgramState> {
    if runtime.payload_size(receiver) != Some(size_of::<ProgramState>())
        || unsafe { receiver.cast::<u64>().read() } != PROGRAM_MAGIC
    {
        return None;
    }
    Some(unsafe { &*receiver.cast::<ProgramState>() })
}

unsafe fn allocate_program(
    runtime: &Runtime,
    vtable: Oop,
    bytes: birnam_artifact::portable_program::ValidatedProgram,
) -> Oop {
    let program = unsafe { runtime.allocate_instance_active(vtable, size_of::<ProgramState>()) };
    unsafe {
        program.cast::<ProgramState>().write(ProgramState {
            magic: PROGRAM_MAGIC,
            bytes,
        })
    };
    runtime.set_finalizer(program, finalize_program);
    program
}

unsafe extern "C" fn finalize_program(object: Oop) {
    if !object.is_null() && unsafe { object.cast::<u64>().read() } == PROGRAM_MAGIC {
        unsafe { ptr::drop_in_place(object.cast::<ProgramState>()) };
    }
}

unsafe extern "C" fn program_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 3 || args.is_null() {
            return type_mismatch(
                runtime,
                b"CodeProgram construction needs parameter, capture, and statement collections",
            );
        }
        let roots = runtime.temporary_root_scope();
        let arguments = unsafe { core::slice::from_raw_parts(args, count) };
        for argument in arguments {
            runtime.push_temporary_root(*argument);
        }
        let parameters = match read_collection(runtime, arguments[0], 4096) {
            Ok(values) => values,
            Err(CollectionFailure::TypeMismatch) => {
                return type_mismatch(runtime, b"parameters must be a finite collection");
            }
            Err(CollectionFailure::Limit) => {
                return program_limit(
                    runtime,
                    b"parameter count exceeds the PortableProgram limit",
                );
            }
        };
        let captures = match read_collection(runtime, arguments[1], 4096) {
            Ok(values) => values,
            Err(CollectionFailure::TypeMismatch) => {
                return type_mismatch(runtime, b"captures must be a finite collection");
            }
            Err(CollectionFailure::Limit) => {
                return program_limit(runtime, b"capture count exceeds the PortableProgram limit");
            }
        };
        let statements = match read_collection(runtime, arguments[2], 262_144) {
            Ok(values) => values,
            Err(CollectionFailure::TypeMismatch) => {
                return type_mismatch(runtime, b"statements must be a finite collection");
            }
            Err(CollectionFailure::Limit) => {
                return program_limit(
                    runtime,
                    b"statement count exceeds the PortableProgram limit",
                );
            }
        };
        let bytes = match encode_program(runtime, &parameters, &captures, &statements) {
            Ok(bytes) => bytes,
            Err(failure) => {
                return construction_failure(
                    runtime,
                    failure,
                    b"CodeProgram semantic validation failed",
                );
            }
        };
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        let bytes =
            birnam_artifact::portable_program::ValidatedProgram::new(bytes).unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("encoded CodeProgram bytes became invalid")
            });
        let program = unsafe { allocate_program(runtime, vtable, bytes) };
        drop(roots);
        program
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn program_format(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 || unsafe { program_state(receiver, runtime) }.is_none() {
            return runtime.canonical().nil;
        }
        runtime.intern(b"portableProgramV1").unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static CodeProgram format contains NUL")
        })
    })
    .unwrap_or(core::ptr::null_mut())
}

fn program_count(runtime: &Runtime, receiver: Oop, field: usize) -> Oop {
    let Some(state) = (unsafe { program_state(receiver, runtime) }) else {
        return runtime.canonical().nil;
    };
    let program = state.bytes.view();
    let value = match field {
        0 | 1 => program.parameter_count,
        2 => program.capture_count,
        3 => program
            .binding_count
            .checked_sub(program.parameter_count)
            .and_then(|count| count.checked_sub(program.capture_count))
            .unwrap_or_else(|| {
                crate::boundary::fatal_invariant("admitted CodeProgram binding partitions drifted")
            }),
        4 => program.binding_count,
        5 => program.statement_count,
        _ => crate::boundary::fatal_invariant("invalid CodeProgram count field"),
    };
    runtime.make_integer(i64::from(value))
}

macro_rules! program_count_method {
    ($name:ident, $field:expr) => {
        unsafe extern "C" fn $name(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
            with_active_if_present(|runtime| {
                if count != 0 {
                    runtime.canonical().nil
                } else {
                    program_count(runtime, receiver, $field)
                }
            })
            .unwrap_or(core::ptr::null_mut())
        }
    };
}

program_count_method!(program_arity, 0);
program_count_method!(program_parameter_count, 1);
program_count_method!(program_capture_count, 2);
program_count_method!(program_let_count, 3);
program_count_method!(program_binding_count, 4);
program_count_method!(program_statement_count, 5);

fn decoded_subtree_start(
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

fn decode_node_tree(
    runtime: &Runtime,
    program: &birnam_artifact::portable_program::Program<'_>,
    root: u32,
) -> Option<Oop> {
    let root = root as usize;
    let start = decoded_subtree_start(program, root)?;
    let vtable = runtime.import(b"CodeNodeVt").ok()?;
    if vtable.is_null() {
        return None;
    }
    let mut stack = Vec::<Oop>::new();
    for index in start..=root {
        let node = program.node(index)?;
        let value = match node.kind {
            1 => NodeValue::Integer(node.payload as i64),
            2 => NodeValue::FloatBits(node.payload.to_le_bytes()),
            3 => NodeValue::String(node.data.to_vec()),
            4 => NodeValue::Symbol(node.data.to_vec()),
            5 => NodeValue::Singleton(Singleton::Nil),
            6 => NodeValue::Singleton(Singleton::True),
            7 => NodeValue::Singleton(Singleton::False),
            8 => NodeValue::LexicalRead(node.auxiliary_0),
            9 => NodeValue::GlobalRead(node.data.to_vec()),
            10 => {
                let operand_count = node.operand_count as usize;
                if stack.len() < operand_count {
                    return None;
                }
                let operands = stack.split_off(stack.len() - operand_count);
                NodeValue::Send {
                    selector: node.data.to_vec(),
                    operands,
                }
            }
            11 => {
                let program_vtable = runtime.import(b"CodeProgramVt").ok()?;
                if program_vtable.is_null() {
                    return None;
                }
                let bytes =
                    birnam_artifact::portable_program::ValidatedProgram::copy_from_slice(node.data)
                        .unwrap_or_else(|_| {
                            crate::boundary::fatal_invariant(
                                "nested CodeProgram bytes became invalid",
                            )
                        });
                let nested = unsafe { allocate_program(runtime, program_vtable, bytes) };
                runtime.push_temporary_root(nested);
                let captures = (0..node.operand_count as usize)
                    .map(|operand| program.node_operand(index, operand))
                    .collect::<Option<Vec<_>>>()?;
                NodeValue::Block {
                    program: nested,
                    captures,
                }
            }
            _ => return None,
        };
        let decoded = unsafe { allocate_node(runtime, vtable, value) };
        runtime.push_temporary_root(decoded);
        stack.push(decoded);
    }
    (stack.len() == 1).then(|| stack[0])
}

unsafe extern "C" fn program_statement_at(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        let index_value = unsafe { args.read() };
        if !runtime.is_integer(index_value) {
            return runtime.canonical().nil;
        }
        let Ok(index) = usize::try_from(runtime.integer_value(index_value)) else {
            return runtime.canonical().nil;
        };
        let roots = runtime.temporary_root_scope();
        runtime.push_temporary_root(receiver);
        let Some(state) = (unsafe { program_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let program = state.bytes.view();
        let Some(statement) = program.statement(index) else {
            return runtime.canonical().nil;
        };
        let let_name = statement
            .binding_index
            .and_then(|binding| program.binding(binding as usize))
            .map(|binding| binding.name.to_vec());
        let Some(expression) = decode_node_tree(runtime, &program, statement.root_node) else {
            return runtime.canonical().nil;
        };
        let statement_vtable = runtime.import(b"CodeStatementVt").unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static CodeStatementVt import contains NUL")
        });
        let value = match statement.kind {
            birnam_artifact::portable_program::StatementKind::Expression => {
                StatementValue::Expression(expression)
            }
            birnam_artifact::portable_program::StatementKind::Let => StatementValue::Let {
                name: let_name.unwrap_or_else(|| {
                    crate::boundary::fatal_invariant("admitted Let statement lost its binding")
                }),
                expression,
            },
        };
        let decoded = unsafe { allocate_statement(runtime, statement_vtable, value) };
        drop(roots);
        decoded
    })
    .unwrap_or(core::ptr::null_mut())
}

fn program_binding_at(runtime: &Runtime, receiver: Oop, index: usize, partition: usize) -> Oop {
    let roots = runtime.temporary_root_scope();
    runtime.push_temporary_root(receiver);
    let Some(state) = (unsafe { program_state(receiver, runtime) }) else {
        return runtime.canonical().nil;
    };
    let program = state.bytes.view();
    let let_count = program
        .binding_count
        .checked_sub(program.parameter_count)
        .and_then(|count| count.checked_sub(program.capture_count))
        .unwrap_or_else(|| {
            crate::boundary::fatal_invariant("admitted CodeProgram binding partitions drifted")
        });
    let global = match partition {
        0 if index < program.binding_count as usize => index,
        1 if index < program.parameter_count as usize => index,
        2 if index < program.capture_count as usize => program.parameter_count as usize + index,
        3 if index < let_count as usize => {
            program.parameter_count as usize + program.capture_count as usize + index
        }
        _ => return runtime.canonical().nil,
    };
    let Some(binding) = (unsafe { allocate_binding(runtime, receiver, global as u32) }) else {
        return runtime.canonical().nil;
    };
    drop(roots);
    binding
}

macro_rules! program_binding_at_method {
    ($name:ident, $partition:expr) => {
        unsafe extern "C" fn $name(receiver: Oop, args: *const Oop, count: usize) -> Oop {
            with_active_if_present(|runtime| {
                if count != 1 || args.is_null() {
                    return runtime.canonical().nil;
                }
                let index = unsafe { args.read() };
                if !runtime.is_integer(index) {
                    return runtime.canonical().nil;
                }
                let Ok(index) = usize::try_from(runtime.integer_value(index)) else {
                    return runtime.canonical().nil;
                };
                program_binding_at(runtime, receiver, index, $partition)
            })
            .unwrap_or(core::ptr::null_mut())
        }
    };
}

program_binding_at_method!(program_any_binding_at, 0);
program_binding_at_method!(program_parameter_at, 1);
program_binding_at_method!(program_capture_at, 2);
program_binding_at_method!(program_let_at, 3);

unsafe fn statement_state(receiver: Oop, runtime: &Runtime) -> Option<&'static StatementState> {
    if runtime.payload_size(receiver) != Some(size_of::<StatementState>())
        || unsafe { receiver.cast::<u64>().read() } != STATEMENT_MAGIC
    {
        return None;
    }
    Some(unsafe { &*receiver.cast::<StatementState>() })
}

unsafe fn allocate_statement(runtime: &Runtime, vtable: Oop, value: StatementValue) -> Oop {
    let statement =
        unsafe { runtime.allocate_instance_active(vtable, size_of::<StatementState>()) };
    unsafe {
        statement.cast::<StatementState>().write(StatementState {
            magic: STATEMENT_MAGIC,
            value,
        })
    };
    runtime.set_finalizer(statement, finalize_statement);
    statement
}

unsafe extern "C" fn finalize_statement(object: Oop) {
    if !object.is_null() && unsafe { object.cast::<u64>().read() } == STATEMENT_MAGIC {
        unsafe { ptr::drop_in_place(object.cast::<StatementState>()) };
    }
}

unsafe extern "C" fn trace_statement(object: Oop, mark: crate::abi::MarkFn) {
    let _ = with_active_if_present(|runtime| {
        let Some(state) = (unsafe { statement_state(object, runtime) }) else {
            return;
        };
        let expression = match &state.value {
            StatementValue::Expression(expression) | StatementValue::Let { expression, .. } => {
                *expression
            }
        };
        unsafe { mark(expression) };
    });
}

unsafe extern "C" fn statement_expression_new(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        let expression = unsafe { args.read() };
        if unsafe { node_state(expression, runtime) }.is_none() {
            return type_mismatch(runtime, b"expression: needs a CodeNode");
        }
        let roots = runtime.temporary_root_scope();
        runtime.push_temporary_root(expression);
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        let statement =
            unsafe { allocate_statement(runtime, vtable, StatementValue::Expression(expression)) };
        drop(roots);
        statement
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn statement_let_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 2 || args.is_null() {
            return type_mismatch(runtime, b"letNamed:expression: needs a String and CodeNode");
        }
        let arguments = unsafe { core::slice::from_raw_parts(args, count) };
        let name_value = arguments[0];
        let expression = arguments[1];
        let Some(name) = runtime.string_bytes(name_value) else {
            return type_mismatch(runtime, b"Let binding name must be a String");
        };
        if name.is_empty() || name.contains(&0) {
            return invalid_program(runtime, b"Let binding name must be nonempty and NUL-free");
        }
        if name.len() > 1 << 20 {
            return program_limit(
                runtime,
                b"Let binding name exceeds the PortableProgram limit",
            );
        }
        if unsafe { node_state(expression, runtime) }.is_none() {
            return type_mismatch(runtime, b"Let expression must be a CodeNode");
        }
        let name = name.to_vec();
        let roots = runtime.temporary_root_scope();
        runtime.push_temporary_root(expression);
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        let statement = unsafe {
            allocate_statement(runtime, vtable, StatementValue::Let { name, expression })
        };
        drop(roots);
        statement
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn statement_kind(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { statement_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let kind = match state.value {
            StatementValue::Expression(_) => b"expression".as_slice(),
            StatementValue::Let { .. } => b"let".as_slice(),
        };
        runtime.intern(kind).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static CodeStatement kind contains NUL")
        })
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn statement_expression(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { statement_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        match &state.value {
            StatementValue::Expression(expression) | StatementValue::Let { expression, .. } => {
                *expression
            }
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn statement_binding_name(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { statement_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let StatementValue::Let { name, .. } = &state.value else {
            return runtime.canonical().nil;
        };
        unsafe {
            runtime.make_string_with_finalizer_active(name, crate::public_api::string_finalizer)
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe fn node_state(receiver: Oop, runtime: &Runtime) -> Option<&'static NodeState> {
    if runtime.payload_size(receiver) != Some(size_of::<NodeState>()) {
        return None;
    }
    // Read the initialized scalar prefix before treating the remaining bytes
    // as a Rust enum. Raw vtable allocation leaves this word zero.
    if unsafe { receiver.cast::<u64>().read() } != NODE_MAGIC {
        return None;
    }
    // SAFETY: only the constructors below publish NODE_MAGIC, and they write a
    // complete NodeState before returning the object.
    Some(unsafe { &*receiver.cast::<NodeState>() })
}

unsafe fn allocate_node(runtime: &Runtime, vtable: Oop, value: NodeValue) -> Oop {
    let node = unsafe { runtime.allocate_instance_active(vtable, size_of::<NodeState>()) };
    // SAFETY: the fresh payload has the exact NodeState size and is not
    // observable until the complete state and finalizer are installed.
    unsafe {
        node.cast::<NodeState>().write(NodeState {
            magic: NODE_MAGIC,
            value,
        })
    };
    runtime.set_finalizer(node, finalize_node);
    node
}

unsafe extern "C" fn finalize_node(object: Oop) {
    if !object.is_null() && unsafe { object.cast::<u64>().read() } == NODE_MAGIC {
        // SAFETY: the magic is written only with a complete NodeState, and the
        // heap invokes an installed finalizer at most once.
        unsafe { ptr::drop_in_place(object.cast::<NodeState>()) };
    }
}

unsafe extern "C" fn trace_node(object: Oop, mark: crate::abi::MarkFn) {
    let _ = with_active_if_present(|runtime| {
        let Some(state) = (unsafe { node_state(object, runtime) }) else {
            return;
        };
        match &state.value {
            NodeValue::Send { operands, .. } => {
                for operand in operands.iter().copied() {
                    unsafe { mark(operand) };
                }
            }
            NodeValue::Block { program, .. } => unsafe { mark(*program) },
            _ => {}
        }
    });
}

enum CollectionFailure {
    TypeMismatch,
    Limit,
}

fn read_collection(
    runtime: &Runtime,
    collection: Oop,
    limit: usize,
) -> Result<Vec<Oop>, CollectionFailure> {
    let size = runtime
        .intern(b"size")
        .map_err(|_| CollectionFailure::TypeMismatch)?;
    let at = runtime
        .intern(b"at:")
        .map_err(|_| CollectionFailure::TypeMismatch)?;
    let count = runtime.send_slice(collection, size, &[]);
    if !runtime.is_integer(count) {
        return Err(CollectionFailure::TypeMismatch);
    }
    let count = usize::try_from(runtime.integer_value(count))
        .map_err(|_| CollectionFailure::TypeMismatch)?;
    if count > limit {
        return Err(CollectionFailure::Limit);
    }
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let index = runtime.make_integer(index as i64);
        let value = runtime.send_slice(collection, at, &[index]);
        runtime.push_temporary_root(value);
        values.push(value);
    }
    Ok(values)
}

unsafe extern "C" fn node_send_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 2 || args.is_null() {
            return type_mismatch(runtime, b"send:operands: needs a Symbol and collection");
        }
        let roots = runtime.temporary_root_scope();
        // SAFETY: count is exactly two and the Method ABI supplies both entries.
        let arguments = unsafe { core::slice::from_raw_parts(args, count) };
        let selector = arguments[0];
        let collection = arguments[1];
        runtime.push_temporary_root(selector);
        runtime.push_temporary_root(collection);
        let Some(selector_bytes) = runtime.symbol_bytes(selector) else {
            return type_mismatch(runtime, b"send selector must be a Symbol");
        };
        let selector_bytes = selector_bytes.to_vec();
        let Ok(arity) = birnam_artifact::portable_program::selector_arity(&selector_bytes) else {
            return invalid_program(runtime, b"send selector has invalid arity spelling");
        };
        let operands = match read_collection(runtime, collection, 4096) {
            Ok(values) => values,
            Err(CollectionFailure::TypeMismatch) => {
                return type_mismatch(runtime, b"send operands must be a finite collection");
            }
            Err(CollectionFailure::Limit) => {
                return program_limit(
                    runtime,
                    b"send operand count exceeds the PortableProgram limit",
                );
            }
        };
        if operands.len() != arity as usize + 1 {
            return invalid_program(runtime, b"send operand count does not match selector arity");
        }
        if operands
            .iter()
            .any(|operand| unsafe { node_state(*operand, runtime) }.is_none())
        {
            return type_mismatch(runtime, b"send operands must contain only CodeNodes");
        }
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        let node = unsafe {
            allocate_node(
                runtime,
                vtable,
                NodeValue::Send {
                    selector: selector_bytes,
                    operands,
                },
            )
        };
        drop(roots);
        node
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_block_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 2 || args.is_null() {
            return type_mismatch(
                runtime,
                b"block:captureBindings: needs a CodeProgram and collection",
            );
        }
        let roots = runtime.temporary_root_scope();
        let arguments = unsafe { core::slice::from_raw_parts(args, count) };
        let program = arguments[0];
        let collection = arguments[1];
        runtime.push_temporary_root(program);
        runtime.push_temporary_root(collection);
        let Some(program_state) = (unsafe { program_state(program, runtime) }) else {
            return type_mismatch(runtime, b"block program must be a CodeProgram");
        };
        let inspected = program_state.bytes.view();
        let values = match read_collection(runtime, collection, 4096) {
            Ok(values) => values,
            Err(CollectionFailure::TypeMismatch) => {
                return type_mismatch(runtime, b"capture bindings must be a finite collection");
            }
            Err(CollectionFailure::Limit) => {
                return program_limit(
                    runtime,
                    b"capture binding count exceeds the PortableProgram limit",
                );
            }
        };
        if values.len() != inspected.capture_count as usize {
            return invalid_program(
                runtime,
                b"capture binding count does not match nested program",
            );
        }
        let mut captures = Vec::with_capacity(values.len());
        for value in values {
            if !runtime.is_integer(value) {
                return type_mismatch(runtime, b"capture binding indices must be Integers");
            }
            let Ok(index) = u32::try_from(runtime.integer_value(value)) else {
                return invalid_program(runtime, b"capture binding index is outside u32");
            };
            captures.push(index);
        }
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        let node =
            unsafe { allocate_node(runtime, vtable, NodeValue::Block { program, captures }) };
        drop(roots);
        node
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_integer_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return type_mismatch(runtime, b"integer: needs one Integer");
        }
        // SAFETY: count is exactly one and the Method ABI supplies that entry.
        let value = unsafe { args.read() };
        if !runtime.is_integer(value) {
            return type_mismatch(runtime, b"integer: needs an Integer");
        }
        // The factory receiver owns the exact admitted CodeNode vtable.
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        unsafe {
            allocate_node(
                runtime,
                vtable,
                NodeValue::Integer(runtime.integer_value(value)),
            )
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_float_bits_new(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return type_mismatch(runtime, b"floatBits: needs one eight-byte String");
        }
        // SAFETY: count is exactly one and the Method ABI supplies that entry.
        let value = unsafe { args.read() };
        let Some(bytes) = runtime.string_bytes(value) else {
            return type_mismatch(runtime, b"floatBits: needs a String");
        };
        let Ok(bits) = <[u8; 8]>::try_from(bytes) else {
            return invalid_program(runtime, b"floatBits: needs exactly eight bytes");
        };
        let vtable = unsafe { runtime.checked_vtable_of_active(receiver) };
        unsafe { allocate_node(runtime, vtable, NodeValue::FloatBits(bits)) }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_kind(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let kind = match state.value {
            NodeValue::Integer(_) => b"integer".as_slice(),
            NodeValue::FloatBits(_) => b"float".as_slice(),
            NodeValue::String(_) => b"string".as_slice(),
            NodeValue::Symbol(_) => b"symbol".as_slice(),
            NodeValue::Singleton(Singleton::Nil) => b"nil".as_slice(),
            NodeValue::Singleton(Singleton::True) => b"true".as_slice(),
            NodeValue::Singleton(Singleton::False) => b"false".as_slice(),
            NodeValue::LexicalRead(_) => b"lexicalRead".as_slice(),
            NodeValue::GlobalRead(_) => b"globalRead".as_slice(),
            NodeValue::Send { .. } => b"send".as_slice(),
            NodeValue::Block { .. } => b"block".as_slice(),
        };
        runtime.intern(kind).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static CodeNode kind contains NUL")
        })
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_integer(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        match state.value {
            NodeValue::Integer(value) => runtime.make_integer(value),
            NodeValue::FloatBits(_) => runtime.canonical().nil,
            NodeValue::String(_) => runtime.canonical().nil,
            NodeValue::Symbol(_) => runtime.canonical().nil,
            NodeValue::Singleton(_) => runtime.canonical().nil,
            NodeValue::LexicalRead(_) => runtime.canonical().nil,
            NodeValue::GlobalRead(_) => runtime.canonical().nil,
            NodeValue::Send { .. } => runtime.canonical().nil,
            NodeValue::Block { .. } => runtime.canonical().nil,
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_program(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::Block { program, .. } = state.value else {
            return runtime.canonical().nil;
        };
        program
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_capture_count(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let count = match &state.value {
            NodeValue::Block { captures, .. } => captures.len(),
            _ => 0,
        };
        runtime.make_integer(count as i64)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_capture_binding_at(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        let index = unsafe { args.read() };
        if !runtime.is_integer(index) {
            return runtime.canonical().nil;
        }
        let Ok(index) = usize::try_from(runtime.integer_value(index)) else {
            return runtime.canonical().nil;
        };
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::Block { captures, .. } = &state.value else {
            return runtime.canonical().nil;
        };
        captures
            .get(index)
            .map(|binding| runtime.make_integer(i64::from(*binding)))
            .unwrap_or(runtime.canonical().nil)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_selector(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::Send { selector, .. } = &state.value else {
            return runtime.canonical().nil;
        };
        runtime.intern(selector).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("admitted CodeNode selector contains NUL")
        })
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_operand_count(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let count = match &state.value {
            NodeValue::Send { operands, .. } => operands.len(),
            _ => 0,
        };
        runtime.make_integer(count as i64)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_operand_at(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        // SAFETY: count is exactly one and the Method ABI supplies that entry.
        let index = unsafe { args.read() };
        if !runtime.is_integer(index) {
            return runtime.canonical().nil;
        }
        let Ok(index) = usize::try_from(runtime.integer_value(index)) else {
            return runtime.canonical().nil;
        };
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::Send { operands, .. } = &state.value else {
            return runtime.canonical().nil;
        };
        operands
            .get(index)
            .copied()
            .unwrap_or(runtime.canonical().nil)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_global(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::GlobalRead(bytes) = &state.value else {
            return runtime.canonical().nil;
        };
        runtime.intern(bytes).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("admitted CodeNode global contains NUL")
        })
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_binding_index(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::LexicalRead(index) = state.value else {
            return runtime.canonical().nil;
        };
        runtime.make_integer(i64::from(index))
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_symbol(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::Symbol(bytes) = &state.value else {
            return runtime.canonical().nil;
        };
        runtime.intern(bytes).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("admitted CodeNode Symbol contains NUL")
        })
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_string(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::String(bytes) = &state.value else {
            return runtime.canonical().nil;
        };
        unsafe {
            runtime.make_string_with_finalizer_active(bytes, crate::public_api::string_finalizer)
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn node_float_bits(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        let Some(state) = (unsafe { node_state(receiver, runtime) }) else {
            return runtime.canonical().nil;
        };
        let NodeValue::FloatBits(bits) = state.value else {
            return runtime.canonical().nil;
        };
        unsafe {
            runtime.make_string_with_finalizer_active(&bits, crate::public_api::string_finalizer)
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn immutable_clone(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count == 0 {
            receiver
        } else {
            runtime.canonical().nil
        }
    })
    .unwrap_or(core::ptr::null_mut())
}

fn family(runtime: &Runtime, name: &[u8], vtable_name: &[u8], parent: Oop) -> (Oop, Oop) {
    let prototype = runtime
        .import(name)
        .unwrap_or_else(|_| crate::boundary::fatal_invariant("static family name contains NUL"));
    if prototype.is_null() {
        let vtable = runtime.create_vtable(parent);
        runtime.push_temporary_root(vtable);
        let prototype = runtime.allocate_empty(vtable, Permanence::Permanent);
        runtime.push_temporary_root(prototype);
        runtime.set_vtable_owner(vtable, prototype);
        return (vtable, prototype);
    }
    let vtable = runtime.import(vtable_name).unwrap_or_else(|_| {
        crate::boundary::fatal_invariant("static family vtable name contains NUL")
    });
    if vtable.is_null() {
        crate::boundary::fatal_invariant("Portable Code reload found an incomplete family")
    }
    runtime.set_vtable_owner(vtable, prototype);
    (vtable, prototype)
}

impl Runtime {
    pub(crate) fn install_portable_code_objects(&self) {
        let _roots = self.temporary_root_scope();
        let globals = self.globals();
        let (vtable, prototype) = family(self, b"CodeNode", b"CodeNodeVt", globals.object_vt);
        self.set_layout(vtable, core::ptr::null(), 0, SLOT_RAW);
        self.set_trace_hook(vtable, trace_node);

        for (selector, method) in [
            (b"integer:".as_slice(), node_integer_new as Method),
            (b"floatBits:".as_slice(), node_float_bits_new as Method),
            (b"string:".as_slice(), node_string_new as Method),
            (b"symbol:".as_slice(), node_symbol_new as Method),
            (b"nilLiteral".as_slice(), node_nil_new as Method),
            (b"trueLiteral".as_slice(), node_true_new as Method),
            (b"falseLiteral".as_slice(), node_false_new as Method),
            (
                b"lexicalReadAt:".as_slice(),
                node_lexical_read_new as Method,
            ),
            (b"globalRead:".as_slice(), node_global_read_new as Method),
            (b"send:operands:".as_slice(), node_send_new as Method),
            (
                b"block:captureBindings:".as_slice(),
                node_block_new as Method,
            ),
            (b"kind".as_slice(), node_kind as Method),
            (b"integer".as_slice(), node_integer as Method),
            (b"floatBits".as_slice(), node_float_bits as Method),
            (b"string".as_slice(), node_string as Method),
            (b"symbol".as_slice(), node_symbol as Method),
            (b"bindingIndex".as_slice(), node_binding_index as Method),
            (b"global".as_slice(), node_global as Method),
            (b"selector".as_slice(), node_selector as Method),
            (b"operandCount".as_slice(), node_operand_count as Method),
            (b"operandAt:".as_slice(), node_operand_at as Method),
            (b"program".as_slice(), node_program as Method),
            (b"captureCount".as_slice(), node_capture_count as Method),
            (
                b"captureBindingAt:".as_slice(),
                node_capture_binding_at as Method,
            ),
            (b"clone".as_slice(), immutable_clone as Method),
        ] {
            let selector = self.intern(selector).unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeNode selector contains NUL")
            });
            let _ = self.add_method(vtable, selector, method);
        }
        self.export(b"CodeNode", prototype).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static CodeNode export contains NUL")
        });
        self.export(b"CodeNodeVt", vtable).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("static CodeNodeVt export contains NUL")
        });

        let (statement_vtable, statement_prototype) = family(
            self,
            b"CodeStatement",
            b"CodeStatementVt",
            globals.object_vt,
        );
        self.set_layout(statement_vtable, core::ptr::null(), 0, SLOT_RAW);
        self.set_trace_hook(statement_vtable, trace_statement);
        for (selector, method) in [
            (
                b"expression:".as_slice(),
                statement_expression_new as Method,
            ),
            (
                b"letNamed:expression:".as_slice(),
                statement_let_new as Method,
            ),
            (b"kind".as_slice(), statement_kind as Method),
            (b"expression".as_slice(), statement_expression as Method),
            (b"bindingName".as_slice(), statement_binding_name as Method),
            (b"clone".as_slice(), immutable_clone as Method),
        ] {
            let selector = self.intern(selector).unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeStatement selector contains NUL")
            });
            let _ = self.add_method(statement_vtable, selector, method);
        }
        self.export(b"CodeStatement", statement_prototype)
            .unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeStatement export contains NUL")
            });
        self.export(b"CodeStatementVt", statement_vtable)
            .unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeStatementVt export contains NUL")
            });

        let (program_vtable, program_prototype) =
            family(self, b"CodeProgram", b"CodeProgramVt", globals.object_vt);
        self.set_layout(program_vtable, core::ptr::null(), 0, SLOT_RAW);
        for (selector, method) in [
            (
                b"parameters:captures:statements:".as_slice(),
                program_new as Method,
            ),
            (b"format".as_slice(), program_format as Method),
            (b"arity".as_slice(), program_arity as Method),
            (
                b"parameterCount".as_slice(),
                program_parameter_count as Method,
            ),
            (b"captureCount".as_slice(), program_capture_count as Method),
            (b"letCount".as_slice(), program_let_count as Method),
            (b"bindingCount".as_slice(), program_binding_count as Method),
            (
                b"statementCount".as_slice(),
                program_statement_count as Method,
            ),
            (b"statementAt:".as_slice(), program_statement_at as Method),
            (b"bindingAt:".as_slice(), program_any_binding_at as Method),
            (b"parameterAt:".as_slice(), program_parameter_at as Method),
            (b"captureAt:".as_slice(), program_capture_at as Method),
            (b"letAt:".as_slice(), program_let_at as Method),
            (b"clone".as_slice(), immutable_clone as Method),
        ] {
            let selector = self.intern(selector).unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeProgram selector contains NUL")
            });
            let _ = self.add_method(program_vtable, selector, method);
        }
        self.export(b"CodeProgram", program_prototype)
            .unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeProgram export contains NUL")
            });
        self.export(b"CodeProgramVt", program_vtable)
            .unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeProgramVt export contains NUL")
            });

        let (binding_vtable, binding_prototype) = family(
            self,
            b"CodeLexicalBinding",
            b"CodeLexicalBindingVt",
            globals.object_vt,
        );
        self.set_layout(binding_vtable, core::ptr::null(), 0, SLOT_RAW);
        self.set_trace_hook(binding_vtable, trace_binding);
        for (selector, method) in [
            (b"kind".as_slice(), binding_kind as Method),
            (b"index".as_slice(), binding_index as Method),
            (b"ordinal".as_slice(), binding_ordinal as Method),
            (b"name".as_slice(), binding_name as Method),
            (
                b"definingStatementIndex".as_slice(),
                binding_defining_statement as Method,
            ),
            (b"clone".as_slice(), immutable_clone as Method),
        ] {
            let selector = self.intern(selector).unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeLexicalBinding selector contains NUL")
            });
            let _ = self.add_method(binding_vtable, selector, method);
        }
        self.export(b"CodeLexicalBinding", binding_prototype)
            .unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeLexicalBinding export contains NUL")
            });
        self.export(b"CodeLexicalBindingVt", binding_vtable)
            .unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeLexicalBindingVt export contains NUL")
            });

        let (code_object_vtable, code_object_prototype) =
            family(self, b"CodeObject", b"CodeObjectVt", globals.object_vt);
        self.set_layout(code_object_vtable, core::ptr::null(), 0, SLOT_RAW);
        self.set_trace_hook(code_object_vtable, trace_code_object);
        for (selector, method) in [
            (
                b"fromProgram:".as_slice(),
                code_object_from_program as Method,
            ),
            (
                b"fromProgram:provenance:".as_slice(),
                code_object_from_program_provenance as Method,
            ),
            (b"format".as_slice(), code_object_format as Method),
            (b"arity".as_slice(), code_object_arity as Method),
            (
                b"captureSchema".as_slice(),
                code_object_capture_schema as Method,
            ),
            (b"program".as_slice(), code_object_program as Method),
            (b"digest".as_slice(), code_object_digest as Method),
            (
                b"sourceProvenance".as_slice(),
                code_object_source_provenance as Method,
            ),
            (
                b"withProgram:".as_slice(),
                code_object_with_program as Method,
            ),
            (
                b"withoutSourceProvenance".as_slice(),
                code_object_without_source_provenance as Method,
            ),
            (b"clone".as_slice(), immutable_clone as Method),
        ] {
            let selector = self.intern(selector).unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeObject selector contains NUL")
            });
            let _ = self.add_method(code_object_vtable, selector, method);
        }
        self.export(b"CodeObject", code_object_prototype)
            .unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeObject export contains NUL")
            });
        self.export(b"CodeObjectVt", code_object_vtable)
            .unwrap_or_else(|_| {
                crate::boundary::fatal_invariant("static CodeObjectVt export contains NUL")
            });

    }
}

#[cfg(test)]
mod byte_rule_contract_tests {
    #[test]
    fn unchecked_arity_preserves_all_byte_classes() {
        for (name, expected) in [
            (&b""[..], 0), (&b"_private"[..], 0), (&b"value"[..], 0),
            (&b"at:put:"[..], 2), (&b"+"[..], 1), (&b"\xff"[..], 1),
            (&b"\0"[..], 1), (&b"_x:\0:"[..], 2),
        ] {
            assert_eq!(birnam_artifact::selector::arity(name), expected, "{name:?}");
        }
    }
}
