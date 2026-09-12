use crate::abi::{Method, Oop, SLOT_RAW};
use crate::heap::Permanence;
use crate::runtime::{Runtime, with_active_if_present};
use birnam_abi::{
    PORTABLE_CODE_API_V1_VERSION, PORTABLE_CODE_API_V2_VERSION, PORTABLE_CODE_API_V3_VERSION,
    PORTABLE_PROGRAM_VIEW_API_V1_VERSION, PortableCodeApiV1, PortableCodeApiV2, PortableCodeApiV3,
    PortableProgramViewApiV1, SLOT_OOP,
};
use core::mem::size_of;
use core::ptr;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const CODE_OBJECT_MAGIC: u64 = 0x4249_524e_434f_424a;
const NODE_MAGIC: u64 = 0x4249_524e_434f_4445;
const STATEMENT_MAGIC: u64 = 0x4249_524e_5354_4d54;
const PROGRAM_MAGIC: u64 = 0x4249_524e_5052_4f47;
const BINDING_MAGIC: u64 = 0x4249_524e_4249_4e44;
const NATIVE_API_NAME: &[u8] = b"_BirnamPortableCodeApiV1";
const NATIVE_API_VTABLE_NAME: &[u8] = b"_BirnamPortableCodeApiV1Vt";
const NATIVE_API_V2_NAME: &[u8] = b"_BirnamPortableCodeApiV2";
const NATIVE_API_V2_VTABLE_NAME: &[u8] = b"_BirnamPortableCodeApiV2Vt";
const NATIVE_API_V3_NAME: &[u8] = b"_BirnamPortableCodeApiV3";
const NATIVE_API_V3_VTABLE_NAME: &[u8] = b"_BirnamPortableCodeApiV3Vt";
const PROGRAM_VIEW_API_NAME: &[u8] = b"_BirnamPortableProgramViewApiV1";
const PROGRAM_VIEW_API_VTABLE_NAME: &[u8] = b"_BirnamPortableProgramViewApiV1Vt";
const BLOCK_CARRIER_NAME: &[u8] = b"_BirnamPortableBlockCarrier";
const BLOCK_CARRIER_VTABLE_NAME: &[u8] = b"_BirnamPortableBlockCarrierVt";

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

fn inspected_code_object(
    runtime: &Runtime,
    object: Oop,
) -> Option<(
    &'static CodeObjectState,
    &'static ProgramState,
    birnam_artifact::portable_program::Program<'static>,
)> {
    let state = unsafe { code_object_state(object, runtime) }?;
    let program_state = unsafe { program_state(state.program, runtime) }?;
    let program = program_state.bytes.view();
    Some((state, program_state, program))
}

unsafe extern "C" fn native_admit_program(bytes: *const u8, length: usize) -> Oop {
    with_active_if_present(|runtime| {
        if bytes.is_null() && length != 0 {
            return invalid_program(runtime, b"PortableProgram bytes pointer was null");
        }
        let bytes = if length == 0 {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(bytes, length) }
        };
        let bytes =
            match birnam_artifact::portable_program::ValidatedProgram::copy_from_slice(bytes) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return match error {
                        birnam_artifact::portable_program::Error::Quantity => {
                            program_limit(runtime, b"PortableProgram bytes exceed a resource limit")
                        }
                        birnam_artifact::portable_program::Error::Format
                        | birnam_artifact::portable_program::Error::Name => {
                            invalid_program(runtime, b"PortableProgram bytes are not canonical v1")
                        }
                    };
                }
            };
        let program_vtable = runtime
            .import(b"CodeProgramVt")
            .unwrap_or(core::ptr::null_mut());
        let code_object_vtable = runtime
            .import(b"CodeObjectVt")
            .unwrap_or(core::ptr::null_mut());
        if program_vtable.is_null() || code_object_vtable.is_null() {
            crate::boundary::fatal_invariant("Portable Code families disappeared after activation")
        }
        let roots = runtime.temporary_root_scope();
        let program = unsafe { allocate_program(runtime, program_vtable, bytes) };
        runtime.push_temporary_root(program);
        let object = unsafe {
            allocate_code_object(
                runtime,
                code_object_vtable,
                program,
                runtime.canonical().nil,
            )
        };
        drop(roots);
        object
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn native_admit_program_with_provenance(
    bytes: *const u8,
    length: usize,
    provenance: Oop,
) -> Oop {
    with_active_if_present(|runtime| {
        if bytes.is_null() && length != 0 {
            return invalid_program(runtime, b"PortableProgram bytes pointer was null");
        }
        let bytes = if length == 0 {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(bytes, length) }
        };
        let Ok(bytes) = birnam_artifact::portable_program::ValidatedProgram::copy_from_slice(bytes)
        else {
            return invalid_program(runtime, b"PortableProgram bytes are not canonical v1");
        };
        let program_vtable = runtime
            .import(b"CodeProgramVt")
            .unwrap_or(core::ptr::null_mut());
        let code_object_vtable = runtime
            .import(b"CodeObjectVt")
            .unwrap_or(core::ptr::null_mut());
        if program_vtable.is_null() || code_object_vtable.is_null() {
            crate::boundary::fatal_invariant("Portable Code families disappeared after activation")
        }
        let roots = runtime.temporary_root_scope();
        runtime.push_temporary_root(provenance);
        let program = unsafe { allocate_program(runtime, program_vtable, bytes) };
        runtime.push_temporary_root(program);
        let object =
            unsafe { allocate_code_object(runtime, code_object_vtable, program, provenance) };
        drop(roots);
        object
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn native_copy_program(
    code_object: Oop,
    destination: *mut u8,
    capacity: usize,
) -> usize {
    with_active_if_present(|runtime| {
        let Some((_, program, _)) = inspected_code_object(runtime, code_object) else {
            return 0;
        };
        let length = program.bytes.len();
        if !destination.is_null() && capacity >= length {
            // SAFETY: the caller promises `capacity` writable bytes and the
            // branch proves the complete immutable program fits. Source and
            // destination cannot overlap across the native object boundary.
            unsafe { ptr::copy_nonoverlapping(program.bytes.as_ptr(), destination, length) };
        }
        length
    })
    .unwrap_or(0)
}

fn selector_arity(selector: &[u8]) -> usize {
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        return colons;
    }
    match selector.first() {
        None => 0,
        Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => 0,
        Some(_) => 1,
    }
}

fn vtable_has_value_arity(runtime: &Runtime, vtable: Oop, arity: usize) -> bool {
    let mut index = 0usize;
    loop {
        let selector = runtime.vtable_selector_at(vtable, index);
        if selector.is_null() {
            return false;
        }
        if let Some(name) = runtime.symbol_bytes(selector) {
            let value_family = if arity == 0 {
                name == b"value"
            } else {
                name.len() == arity.saturating_mul(b"value:".len())
                    && name
                        .chunks_exact(b"value:".len())
                        .all(|part| part == b"value:")
            };
            if value_family && selector_arity(name) == arity {
                return true;
            }
        }
        index = index.saturating_add(1);
    }
}

fn vtable_inherits_from(runtime: &Runtime, mut vtable: Oop, ancestor: Oop) -> bool {
    for _ in 0..4096 {
        if vtable == ancestor {
            return true;
        }
        if vtable.is_null() {
            return false;
        }
        vtable = runtime.vtable_parent(vtable);
    }
    false
}

fn portable_block_parts(
    runtime: &Runtime,
    receiver: Oop,
) -> Option<(
    Oop,
    &'static ProgramState,
    birnam_artifact::portable_program::Program<'static>,
)> {
    let vtable = runtime.vtable_of(receiver);
    if vtable.is_null() {
        return None;
    }
    let owner = runtime.vtable_owner(vtable);
    let carrier_vtable = runtime.import(BLOCK_CARRIER_VTABLE_NAME).ok()?;
    if owner.is_null()
        || runtime.vtable_of(owner) != carrier_vtable
        || runtime.payload_size(owner) != Some(2 * size_of::<Oop>())
    {
        return None;
    }
    let carrier_code_object = unsafe { owner.cast::<Oop>().read() };
    let (_, carrier_program_state, carrier_program) =
        inspected_code_object(runtime, carrier_code_object)?;
    let code_object = unsafe { receiver.cast::<Oop>().read() };
    let (_, program_state, program) = inspected_code_object(runtime, code_object)?;
    let payload_words = usize::try_from(program.capture_count)
        .ok()?
        .checked_add(1)?;
    if runtime.payload_size(receiver) != Some(payload_words.checked_mul(size_of::<Oop>())?)
        || carrier_program_state.bytes != program_state.bytes
        || carrier_program.parameter_count != program.parameter_count
        || carrier_program.capture_count != program.capture_count
    {
        return None;
    }
    Some((code_object, program_state, program))
}

unsafe extern "C" fn block_code_object(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        portable_block_parts(runtime, receiver)
            .map(|parts| parts.0)
            .unwrap_or(runtime.canonical().nil)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn block_capture_count(receiver: Oop, _args: *const Oop, count: usize) -> Oop {
    with_active_if_present(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        portable_block_parts(runtime, receiver)
            .map(|parts| runtime.make_integer(i64::from(parts.2.capture_count)))
            .unwrap_or(runtime.canonical().nil)
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn block_capture_at(receiver: Oop, args: *const Oop, count: usize) -> Oop {
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
        let Some((_, _, program)) = portable_block_parts(runtime, receiver) else {
            return runtime.canonical().nil;
        };
        if index >= program.capture_count as usize {
            return runtime.canonical().nil;
        }
        unsafe { receiver.cast::<Oop>().add(index + 1).read() }
    })
    .unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn native_new_block(
    block_vtable: Oop,
    code_object: Oop,
    captures: *const Oop,
    capture_count: usize,
) -> Oop {
    unsafe {
        native_new_materialized_block(
            block_vtable,
            code_object,
            captures,
            capture_count,
            core::ptr::null_mut(),
        )
    }
}

/// Optional, pure declaration of the exact warm constructor operation.
/// See abi/native-block-construction-v1.md. This never constructs an object,
/// installs a carrier, calls an Id Method, or grants invocation authority.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_native_block_construction_v1(
    constructor: *const core::ffi::c_void,
    nil: Oop,
    context: *const usize,
    block_vtable: Oop,
    code_object: Oop,
    capture_count: usize,
    materialization_owner: Oop,
    expected_program: *const u8,
    expected_length: usize,
) -> u32 {
    if context.is_null() || expected_program.is_null() || expected_length > 65_536 {
        return 0;
    }
    let mode = if constructor == native_new_block as *const core::ffi::c_void {
        if !materialization_owner.is_null() {
            return 0;
        }
        1
    } else if constructor == native_new_materialized_block as *const core::ffi::c_void {
        2
    } else {
        return 0;
    };
    with_active_if_present(|runtime| {
        let api = runtime.api();
        // Check the current table before calling any entry. Copied tables work;
        // instrumentation or a different floor must withdraw this declaration.
        if nil != api.nilObj
            || unsafe { core::slice::from_raw_parts(context, 10) }
                != [
                    api.alloc as usize,
                    api.import as usize,
                    api.objectPayloadSize as usize,
                    api.vtableOf as usize,
                    api.vtableParent as usize,
                    api.vtableSelectorAt as usize,
                    api.symbolName as usize,
                    api.vtableOwner as usize,
                    api.symbol_vt as usize,
                    api.exportedNameOf as usize,
                ]
        {
            return 0;
        }
        let Some((_, program_state, program)) = inspected_code_object(runtime, code_object) else {
            return 0;
        };
        if capture_count != program.capture_count as usize
            || program_state.bytes.as_ref()
                != unsafe { core::slice::from_raw_parts(expected_program, expected_length) }
        {
            return 0;
        }
        // Reverse export inspection never interns a missing name. An object
        // exported under several names may conservatively decline when the
        // floor returns another alias first; it must not manufacture authority.
        let named = |object: Oop, expected: &[u8]| {
            if object.is_null() {
                return false;
            }
            let name = unsafe { (api.exportedNameOf)(object) };
            !name.is_null() && unsafe { core::ffi::CStr::from_ptr(name) }.to_bytes() == expected
        };
        let mut ancestor = block_vtable;
        let mut family = false;
        for _ in 0..4096 {
            if ancestor.is_null() {
                break;
            }
            if named(ancestor, b"BlockVt") {
                family = true;
                break;
            }
            ancestor = runtime.vtable_parent(ancestor);
        }
        if !family
            || !vtable_has_value_arity(runtime, block_vtable, program.parameter_count as usize)
        {
            return 0;
        }
        let owner = runtime.vtable_owner(block_vtable);
        if owner.is_null()
            || runtime.payload_size(owner) != Some(2 * size_of::<Oop>())
            || !named(runtime.vtable_of(owner), BLOCK_CARRIER_VTABLE_NAME)
            || unsafe { owner.cast::<Oop>().add(1).read() } != materialization_owner
        {
            return 0;
        }
        let carrier_code = unsafe { owner.cast::<Oop>().read() };
        if !inspected_code_object(runtime, carrier_code)
            .is_some_and(|(_, state, _)| state.bytes == program_state.bytes)
        {
            return 0;
        }
        mode
    })
    .unwrap_or(0)
}

unsafe extern "C" fn native_new_materialized_block(
    block_vtable: Oop,
    code_object: Oop,
    captures: *const Oop,
    capture_count: usize,
    materialization_owner: Oop,
) -> Oop {
    with_active_if_present(|runtime| {
        let Some((_, program_state, program)) = inspected_code_object(runtime, code_object) else {
            return type_mismatch(runtime, b"portable Block needs an admitted CodeObject");
        };
        if capture_count != program.capture_count as usize
            || (captures.is_null() && capture_count != 0)
        {
            return invalid_program(
                runtime,
                b"portable Block capture count disagrees with CodeObject",
            );
        }
        let block_family = runtime.import(b"BlockVt").unwrap_or(core::ptr::null_mut());
        if block_vtable.is_null()
            || block_family.is_null()
            || !vtable_inherits_from(runtime, block_vtable, block_family)
            || !vtable_has_value_arity(runtime, block_vtable, program.parameter_count as usize)
        {
            return invalid_program(
                runtime,
                b"portable Block vtable has incompatible invocation arity",
            );
        }

        let roots = runtime.temporary_root_scope();
        runtime.push_temporary_root(code_object);
        if !materialization_owner.is_null() {
            runtime.push_temporary_root(materialization_owner);
        }
        let captures = if capture_count == 0 {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(captures, capture_count) }
        };
        for capture in captures.iter().copied() {
            runtime.push_temporary_root(capture);
        }

        let carrier_vtable = runtime
            .import(BLOCK_CARRIER_VTABLE_NAME)
            .unwrap_or(core::ptr::null_mut());
        if carrier_vtable.is_null() {
            crate::boundary::fatal_invariant("Portable Block carrier disappeared")
        }
        let owner = runtime.vtable_owner(block_vtable);
        if owner.is_null() {
            let carrier =
                unsafe { runtime.allocate_instance_active(carrier_vtable, 2 * size_of::<Oop>()) };
            unsafe {
                carrier.cast::<Oop>().write(code_object);
                carrier.cast::<Oop>().add(1).write(materialization_owner);
            };
            runtime.set_vtable_owner(block_vtable, carrier);
            runtime.set_layout(block_vtable, core::ptr::null(), 0, SLOT_OOP);
            for (selector, method) in [
                (b"codeObject".as_slice(), block_code_object as Method),
                (b"captureCount".as_slice(), block_capture_count as Method),
                (b"captureAt:".as_slice(), block_capture_at as Method),
            ] {
                let selector = runtime.intern(selector).unwrap_or_else(|_| {
                    crate::boundary::fatal_invariant("static portable Block selector contains NUL")
                });
                let _ = runtime.add_method(block_vtable, selector, method);
            }
        } else {
            let carrier_code_object = unsafe { owner.cast::<Oop>().read() };
            let same_program = inspected_code_object(runtime, carrier_code_object).is_some_and(
                |(_, carrier_program, _)| carrier_program.bytes == program_state.bytes,
            );
            if runtime.vtable_of(owner) != carrier_vtable
                || runtime.payload_size(owner) != Some(2 * size_of::<Oop>())
                || !same_program
                || unsafe { owner.cast::<Oop>().add(1).read() } != materialization_owner
            {
                return invalid_program(
                    runtime,
                    b"portable Block vtable was reused for different meaning",
                );
            }
        }

        let Some(payload_words) = capture_count.checked_add(1) else {
            return program_limit(runtime, b"portable Block payload size overflowed");
        };
        let Some(payload_bytes) = payload_words.checked_mul(size_of::<Oop>()) else {
            return program_limit(runtime, b"portable Block payload size overflowed");
        };
        let block = unsafe { runtime.allocate_instance_active(block_vtable, payload_bytes) };
        unsafe { block.cast::<Oop>().write(code_object) };
        for (index, capture) in captures.iter().copied().enumerate() {
            unsafe { block.cast::<Oop>().add(index + 1).write(capture) };
        }
        drop(roots);
        block
    })
    .unwrap_or(core::ptr::null_mut())
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
        let prototype = runtime.allocate_empty(vtable, Permanence::Permanent);
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

fn private_payload_object(
    runtime: &Runtime,
    name: &[u8],
    vtable_name: &[u8],
    parent: Oop,
    payload_bytes: usize,
) -> (Oop, Oop) {
    let object = runtime
        .import(name)
        .unwrap_or_else(|_| crate::boundary::fatal_invariant("private export name contains NUL"));
    if object.is_null() {
        let vtable = runtime.create_vtable(parent);
        let object = unsafe { runtime.allocate_instance_active(vtable, payload_bytes) };
        runtime.set_vtable_owner(vtable, object);
        runtime.export(name, object).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("private object export name contains NUL")
        });
        runtime.export(vtable_name, vtable).unwrap_or_else(|_| {
            crate::boundary::fatal_invariant("private vtable export name contains NUL")
        });
        return (vtable, object);
    }
    let vtable = runtime.import(vtable_name).unwrap_or_else(|_| {
        crate::boundary::fatal_invariant("private vtable import name contains NUL")
    });
    if vtable.is_null() || runtime.payload_size(object) != Some(payload_bytes) {
        crate::boundary::fatal_invariant(
            "Portable Code reload found an incompatible private object",
        )
    }
    runtime.set_vtable_owner(vtable, object);
    (vtable, object)
}

fn install_native_api(runtime: &Runtime) {
    let globals = runtime.globals();
    let (carrier_vtable, carrier_prototype) = private_payload_object(
        runtime,
        BLOCK_CARRIER_NAME,
        BLOCK_CARRIER_VTABLE_NAME,
        globals.object_vt,
        2 * size_of::<Oop>(),
    );
    let carrier_layout = [SLOT_OOP, SLOT_OOP];
    runtime.set_layout(
        carrier_vtable,
        carrier_layout.as_ptr(),
        carrier_layout.len(),
        SLOT_RAW,
    );
    unsafe {
        carrier_prototype
            .cast::<Oop>()
            .write(runtime.canonical().nil);
        carrier_prototype
            .cast::<Oop>()
            .add(1)
            .write(runtime.canonical().nil);
    };

    let (api_v3_vtable, api_v3_object) = private_payload_object(
        runtime,
        NATIVE_API_V3_NAME,
        NATIVE_API_V3_VTABLE_NAME,
        globals.object_vt,
        size_of::<PortableCodeApiV3>(),
    );
    runtime.set_layout(api_v3_vtable, core::ptr::null(), 0, SLOT_RAW);
    unsafe {
        api_v3_object
            .cast::<PortableCodeApiV3>()
            .write(PortableCodeApiV3 {
                abi_version: PORTABLE_CODE_API_V3_VERSION,
                struct_size: size_of::<PortableCodeApiV3>() as u64,
                admit_program_with_provenance: native_admit_program_with_provenance,
            })
    };

    let (api_v2_vtable, api_v2_object) = private_payload_object(
        runtime,
        NATIVE_API_V2_NAME,
        NATIVE_API_V2_VTABLE_NAME,
        globals.object_vt,
        size_of::<PortableCodeApiV2>(),
    );
    runtime.set_layout(api_v2_vtable, core::ptr::null(), 0, SLOT_RAW);
    unsafe {
        api_v2_object
            .cast::<PortableCodeApiV2>()
            .write(PortableCodeApiV2 {
                abi_version: PORTABLE_CODE_API_V2_VERSION,
                struct_size: size_of::<PortableCodeApiV2>() as u64,
                new_materialized_block: native_new_materialized_block,
            })
    };

    let (api_vtable, api_object) = private_payload_object(
        runtime,
        NATIVE_API_NAME,
        NATIVE_API_VTABLE_NAME,
        globals.object_vt,
        size_of::<PortableCodeApiV1>(),
    );
    runtime.set_layout(api_vtable, core::ptr::null(), 0, SLOT_RAW);
    unsafe {
        api_object
            .cast::<PortableCodeApiV1>()
            .write(PortableCodeApiV1 {
                abi_version: PORTABLE_CODE_API_V1_VERSION,
                struct_size: size_of::<PortableCodeApiV1>() as u64,
                admit_program: native_admit_program,
                new_block: native_new_block,
            })
    };

    let (view_vtable, view_object) = private_payload_object(
        runtime,
        PROGRAM_VIEW_API_NAME,
        PROGRAM_VIEW_API_VTABLE_NAME,
        globals.object_vt,
        size_of::<PortableProgramViewApiV1>(),
    );
    runtime.set_layout(view_vtable, core::ptr::null(), 0, SLOT_RAW);
    unsafe {
        view_object
            .cast::<PortableProgramViewApiV1>()
            .write(PortableProgramViewApiV1 {
                abi_version: PORTABLE_PROGRAM_VIEW_API_V1_VERSION,
                struct_size: size_of::<PortableProgramViewApiV1>() as u64,
                copy_program: native_copy_program,
            })
    };
}

impl Runtime {
    pub(crate) fn install_portable_code_objects(&self) {
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

        install_native_api(self);
    }
}
