//! Complete Integer base. No Float, Block, IdApi or Runtime inputs.
use super::storage::{Instance, STORAGE};
use birnam_abi::{
    Oop, base_values,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    lifetime, namespace_owner, root_registry,
    string::StringApiV1,
};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
pub(crate) struct Access {
    pub id: Protocol,
    roots: root_registry::OwnedV1,
    values: base_values::ValuesV1,
    namespace: Oop,
    string: Oop,
    allocation_size: NativeEntry,
    behavior_of: NativeEntry,
    allocate: Oop,
    binding: Oop,
    error_name: Oop,
    error_behavior_name: Oop,
    error_make: Oop,
    string_make: Oop,
}
impl Access {
    pub(crate) fn references(self) -> [Oop; 14] {
        [
            self.id.receiver,
            self.id.intern,
            self.id.load,
            self.namespace,
            self.string,
            self.values.nil,
            self.values.true_object,
            self.values.false_object,
            self.allocate,
            self.binding,
            self.error_name,
            self.error_behavior_name,
            self.error_make,
            self.string_make,
        ]
    }
}
static READY: AtomicBool = AtomicBool::new(false);
pub(crate) fn ready() -> bool {
    READY.load(Ordering::Acquire)
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn send(id: Protocol, receiver: Oop, selector: Oop, args: &[Operand]) -> Result<Operand, Outcome> {
    unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) }
        .into_result()
}
fn intern(id: Protocol, bytes: &[u8]) -> Result<Oop, Outcome> {
    send(id, id.receiver, id.intern, &[Operand::bytes(bytes)])?
        .as_object()
        .ok_or_else(invalid)
}
fn select(id: Protocol, name: &[u8]) -> Result<NativeEntry, Outcome> {
    send(
        id,
        id.receiver,
        intern(id, b"lookup")?,
        &[Operand::object(intern(id, name)?)],
    )?
    .as_entry()
    .ok_or_else(invalid)
}
fn fact(access: Access, entry: NativeEntry, object: Oop) -> Result<Operand, Outcome> {
    let args = [Operand::object(object)];
    unsafe { entry.method.unwrap()(entry.context, access.id.receiver, args.as_ptr(), args.len()) }
        .into_result()
}
struct Root {
    roots: root_registry::OwnedV1,
    token: usize,
}
impl Root {
    fn new(roots: root_registry::OwnedV1, object: Oop) -> Self {
        let token = unsafe { roots.retain.unwrap()(roots.context, object) };
        assert_ne!(token, 0);
        Self { roots, token }
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert_eq!(
            unsafe { self.roots.release.unwrap()(self.roots.context, self.token) },
            1
        );
    }
}
fn answer(result: Result<Oop, Outcome>) -> Outcome {
    match result {
        Ok(object) => Outcome::loaded(Operand::object(object)),
        Err(error) => error,
    }
}
fn known(access: Access, object: Oop) -> bool {
    fact(access, access.allocation_size, object)
        .ok()
        .and_then(|value| value.as_word())
        .is_some()
}
pub(crate) fn exact(instance: &Instance, access: Access, object: Oop) -> bool {
    fact(access, access.allocation_size, object)
        .ok()
        .and_then(|value| value.as_word())
        .is_some_and(|size| size >= STORAGE.payload_size)
        && fact(access, access.behavior_of, object)
            .ok()
            .and_then(|value| value.as_object())
            == Some(instance.identities()[0])
}
pub(crate) unsafe fn make(instance: &Instance, access: Access, value: i64) -> Result<Oop, Outcome> {
    let behavior = instance.identities()[0];
    let object = send(
        access.id,
        behavior,
        access.allocate,
        &[Operand::word(STORAGE.payload_size)],
    )?
    .as_object()
    .filter(|object| !object.is_null())
    .ok_or_else(invalid)?;
    if !exact(instance, access, object) {
        return Err(invalid());
    }
    unsafe { (STORAGE.initialize)(object.cast(), value) };
    Ok(object)
}
unsafe fn integer(instance: &Instance, access: Access, object: Oop) -> Option<i64> {
    let ops = unsafe { *instance.operations() };
    if unsafe { (ops.is_exact)(ops.context, object) } == 0 || !exact(instance, access, object) {
        return None;
    }
    Some(unsafe { (ops.read)(ops.context, object) })
}
fn lookup_binding(access: Access, name: Oop) -> Result<Option<Oop>, Outcome> {
    Ok(send(
        access.id,
        access.namespace,
        access.binding,
        &[Operand::object(name)],
    )?
    .as_object()
    .filter(|object| !object.is_null()))
}
fn is_error(access: Access, object: Oop) -> Result<bool, Outcome> {
    let Some(behavior) = lookup_binding(access, access.error_behavior_name)? else {
        return Ok(false);
    };
    Ok(fact(access, access.behavior_of, object)?.as_object() == Some(behavior))
}
fn error(access: Access, kind: &[u8], message: &[u8]) -> Result<Oop, Outcome> {
    let kind = intern(access.id, kind)?;
    let _kind = Root::new(access.roots, kind);
    let message = send(
        access.id,
        access.string,
        access.string_make,
        &[Operand::bytes(message)],
    )?
    .as_object()
    .ok_or_else(invalid)?;
    let _message = Root::new(access.roots, message);
    let Some(factory) = lookup_binding(access, access.error_name)? else {
        return Ok(access.values.nil);
    };
    let _factory = Root::new(access.roots, factory);
    send(
        access.id,
        factory,
        access.error_make,
        &[Operand::object(kind), Operand::object(message)],
    )?
    .as_object()
    .ok_or_else(invalid)
}
unsafe fn result(instance: &Instance, access: Access, value: i128) -> Result<Oop, Outcome> {
    let Ok(value) = i64::try_from(value) else {
        return error(access, b"overflow", b"integer overflow");
    };
    let ops = unsafe { *instance.operations() };
    Ok(unsafe { (ops.make)(ops.context, value) })
}
fn boolean(access: Access, value: bool) -> Oop {
    if value {
        access.values.true_object
    } else {
        access.values.false_object
    }
}
unsafe fn binary(
    instance: &Instance,
    access: Access,
    receiver: Oop,
    right: Oop,
    operation: u8,
) -> Result<Oop, Outcome> {
    if !known(access, receiver) || !known(access, right) {
        return Err(invalid());
    }
    let _receiver = Root::new(access.roots, receiver);
    let _right = Root::new(access.roots, right);
    let Some(left) = (unsafe { integer(instance, access, receiver) }) else {
        return Ok(access.values.nil);
    };
    // Equality deliberately retains the existing exact-Integer rule. Other
    // operators propagate the current Namespace ErrorVt before reading RHS.
    if operation != 5 && is_error(access, right)? {
        return Ok(right);
    }
    let Some(right) = (unsafe { integer(instance, access, right) }) else {
        if operation == 5 {
            return Ok(access.values.false_object);
        }
        let message = if operation == 4 {
            b"mod: needs an Integer".as_slice()
        } else if operation > 5 {
            b"comparison needs a number".as_slice()
        } else {
            b"Integer arithmetic needs a number".as_slice()
        };
        return error(access, b"typeMismatch", message);
    };
    unsafe { calculate(instance, access, left, right, operation) }
}
unsafe fn calculate(
    instance: &Instance,
    access: Access,
    left: i64,
    right: i64,
    operation: u8,
) -> Result<Oop, Outcome> {
    if (operation == 3 || operation == 4) && right == 0 {
        return error(access, b"zeroDivide", b"division by zero");
    }
    match operation {
        0 => unsafe { result(instance, access, i128::from(left) + i128::from(right)) },
        1 => unsafe { result(instance, access, i128::from(left) - i128::from(right)) },
        2 => unsafe { result(instance, access, i128::from(left) * i128::from(right)) },
        3 => unsafe { result(instance, access, i128::from(left) / i128::from(right)) },
        4 => unsafe { result(instance, access, i128::from(left) % i128::from(right)) },
        5 => Ok(boolean(access, left == right)),
        // Preserve the existing language comparison rounding; removing the
        // Float module dependency does not change the language's results.
        6 => Ok(boolean(access, (left as f64) < (right as f64))),
        7 => Ok(boolean(access, (left as f64) <= (right as f64))),
        8 => Ok(boolean(access, (left as f64) > (right as f64))),
        9 => Ok(boolean(access, (left as f64) >= (right as f64))),
        _ => unreachable!(),
    }
}
macro_rules! binary_entry {
    ($name:ident, $operation:expr) => {
        unsafe extern "C" fn $name(
            context: *mut c_void,
            receiver: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            let Some([right]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
                return invalid();
            };
            let Some(right) = right.as_object() else {
                return invalid();
            };
            let instance = unsafe { &*context.cast::<Instance>() };
            answer(unsafe {
                binary(
                    instance,
                    instance.native_access().unwrap(),
                    receiver,
                    right,
                    $operation,
                )
            })
        }
    };
}
binary_entry!(add, 0);
binary_entry!(subtract, 1);
binary_entry!(multiply, 2);
binary_entry!(divide, 3);
binary_entry!(modulo, 4);
binary_entry!(equals, 5);
binary_entry!(less, 6);
binary_entry!(less_equal, 7);
binary_entry!(greater, 8);
binary_entry!(greater_equal, 9);
unsafe extern "C" fn from_word(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([value]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(value) = value.as_word() else {
        return invalid();
    };
    let instance = unsafe { &*context.cast::<Instance>() };
    answer(unsafe { make(instance, instance.native_access().unwrap(), value as i64) })
}
unsafe extern "C" fn word(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let instance = unsafe { &*context.cast::<Instance>() };
    let access = instance.native_access().unwrap();
    if !known(access, receiver) {
        return invalid();
    }
    let _receiver = Root::new(access.roots, receiver);
    match unsafe { integer(instance, access, receiver) } {
        Some(value) => Outcome::loaded(Operand::word(value as usize)),
        None => invalid(),
    }
}
unsafe extern "C" fn print(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let instance = unsafe { &*context.cast::<Instance>() };
    let access = instance.native_access().unwrap();
    if !known(access, receiver) {
        return invalid();
    }
    let _receiver = Root::new(access.roots, receiver);
    let Some(value) = (unsafe { integer(instance, access, receiver) }) else {
        return Outcome::loaded(Operand::object(access.values.nil));
    };
    let text = value.to_string();
    match send(
        access.id,
        access.string,
        access.string_make,
        &[Operand::bytes(text.as_bytes())],
    ) {
        Ok(value) => Outcome::loaded(value),
        Err(error) => error,
    }
}
const METHODS: &[(&[u8], native::Method)] = &[
    (b"+", add),
    (b"-", subtract),
    (b"*", multiply),
    (b"/", divide),
    (b"mod:", modulo),
    (b"=", equals),
    (b"<", less),
    (b"<=", less_equal),
    (b">", greater),
    (b">=", greater_equal),
    (b"printString", print),
    (b"fromWord:", from_word),
    (b"word", word),
    (b"installFloat:", install_float),
];
pub(crate) unsafe fn extend_binding(instance: &Instance, name: &[u8]) -> bool {
    if matches!(name, b"mod:" | b"=" | b"printString") {
        return false;
    }
    let access = instance.native_access().unwrap();
    let selected = (|| {
        send(
            access.id,
            instance.identities()[0],
            intern(access.id, b"lookup")?,
            &[Operand::object(intern(access.id, name)?)],
        )
    })()
    .unwrap_or_else(|_| super::abort("Integer extension lookup failed"));
    if matches!(name, b"asFloat" | b"to:do:") {
        return selected.kind() == native::UNIT;
    }
    let Some(entry) = selected.as_entry() else {
        return false;
    };
    METHODS.iter().any(|(candidate, method)| {
        *candidate == name
            && entry.method.map(|method| method as usize) == Some(*method as usize)
            && entry.context == ptr::from_ref(instance).cast_mut().cast()
    })
}
fn construct(
    id: Protocol,
    roots: root_registry::OwnedV1,
    lifetime: lifetime::OwnedV1,
    values: base_values::ValuesV1,
    namespace: namespace_owner::OwnedV1,
    string: StringApiV1,
    error_factory: Oop,
) -> Result<*mut birnam_abi::integer::IntegerApiV1, Outcome> {
    let access = Access {
        id,
        roots,
        values,
        namespace: namespace.receiver,
        string: string.prototype,
        allocation_size: select(id, b"allocationSizeOf:")?,
        behavior_of: select(id, b"behaviorOf:")?,
        allocate: intern(id, b"allocate")?,
        binding: intern(id, b"bindingAt:")?,
        error_name: intern(id, b"Error")?,
        error_behavior_name: intern(id, b"ErrorVt")?,
        error_make: intern(id, b"kind:message:")?,
        string_make: intern(id, b"fromBytes:")?,
    };
    if lookup_binding(access, access.error_name)? != Some(error_factory)
        || lookup_binding(access, access.error_behavior_name)?.is_none()
    {
        return Err(invalid());
    }
    let error_behavior = fact(access, access.behavior_of, error_factory)?
        .as_object()
        .ok_or_else(invalid)?;
    if send(
        id,
        error_behavior,
        intern(id, b"lookup")?,
        &[Operand::object(access.error_make)],
    )?
    .as_entry()
    .is_none()
    {
        return Err(invalid());
    }
    let instance = Box::into_raw(Instance::native(access));
    super::INSTANCE.store(instance, Ordering::Release);
    unsafe { root_registry::register(roots.api, super::integer_roots) };
    let object = send(id, id.receiver, intern(id, b"objectPrototype")?, &[])?
        .as_object()
        .ok_or_else(invalid)?;
    let parent = fact(access, access.behavior_of, object)?
        .as_object()
        .ok_or_else(invalid)?;
    let behavior = send(id, parent, intern(id, b"delegated")?, &[])?
        .as_object()
        .ok_or_else(invalid)?;
    unsafe {
        (*instance).set_behavior(behavior);
        birnam_abi::payload_layout::set(
            lifetime.layouts,
            behavior,
            ptr::null(),
            0,
            birnam_abi::SLOT_RAW,
        );
    }
    let prototype = unsafe { make(&*instance, access, 0) }?;
    unsafe { (*instance).set_prototype(prototype) };
    let add_method = intern(id, b"addMethod")?;
    for (name, method) in METHODS {
        send(
            id,
            behavior,
            add_method,
            &[
                Operand::object(intern(id, name)?),
                Operand::entry(NativeEntry::new(*method, instance.cast())),
            ],
        )?;
    }
    send(
        id,
        id.receiver,
        intern(id, b"ownerOf:put:")?,
        &[Operand::object(behavior), Operand::object(prototype)],
    )?;
    let publish = intern(id, b"bindingAt:put:")?;
    for (name, value) in [
        (b"Integer".as_slice(), prototype),
        (b"IntegerVt".as_slice(), behavior),
    ] {
        send(
            id,
            namespace.receiver,
            publish,
            &[Operand::object(intern(id, name)?), Operand::object(value)],
        )?;
    }
    let ops = unsafe { (*instance).operations() };
    assert_eq!(unsafe { birnam_abi::integer::attach(ops) }, 1);
    READY.store(true, Ordering::Release);
    Ok(ops)
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if id.is_null() {
        return invalid();
    }
    let Some([roots, lifetime, values, namespace, string, error]) =
        (unsafe { birnam_abi::borrowed_slice(args, count) })
    else {
        return invalid();
    };
    let (Some(roots), Some(lifetime), Some(values), Some(namespace), Some(string), Some(error)) = (
        roots.as_word(),
        lifetime.as_word(),
        values.as_word(),
        namespace.as_word(),
        string.as_word(),
        error.as_object(),
    ) else {
        return invalid();
    };
    let (roots, lifetime, values, namespace, string) = (
        roots as *const root_registry::OwnedV1,
        lifetime as *const lifetime::OwnedV1,
        values as *const base_values::ValuesV1,
        namespace as *const namespace_owner::OwnedV1,
        string as *const StringApiV1,
    );
    if !unsafe {
        root_registry::valid_owned(roots)
            && lifetime::valid(lifetime)
            && base_values::valid(values)
            && namespace_owner::valid(namespace)
            && birnam_abi::string::valid(string)
    } || !super::INSTANCE.load(Ordering::Acquire).is_null()
    {
        return invalid();
    }
    match construct(
        unsafe { *id },
        unsafe { *roots },
        unsafe { *lifetime },
        unsafe { *values },
        unsafe { *namespace },
        unsafe { *string },
        error,
    ) {
        Ok(ops) => Outcome::loaded(Operand::word(ops as usize)),
        Err(error) => error,
    }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};

// A separate, complete owner created only after Float is constructed. The
// Integer base has no Float slot waiting to be filled.
struct FloatExtension {
    integer: *const Instance,
    float: *mut birnam_abi::float::FloatApiV1,
}
static FLOAT_EXTENSION: std::sync::atomic::AtomicPtr<FloatExtension> =
    std::sync::atomic::AtomicPtr::new(ptr::null_mut());

fn admitted_instance(ops: &birnam_abi::integer::IntegerApiV1) -> Option<&'static Instance> {
    if !ready() { return None; }
    let instance = super::INSTANCE.load(Ordering::Acquire);
    if instance.is_null() { return None; }
    let instance = unsafe { &*instance };
    let original = instance.original_operations();
    (ops.same_contract(&original) && unsafe { *instance.operations() }.same_contract(&original))
        .then_some(instance)
}

/// Pure snapshot of the immutable dependencies of the actual constructor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_integer_construction_v1(
    ops: *const birnam_abi::integer::IntegerApiV1,
    out: *mut birnam_abi::integer_native::ConstructionV1,
) -> i32 {
    if ops.is_null() || out.is_null() { return 0; }
    let Some(instance) = admitted_instance(unsafe { &*ops }) else { return 0; };
    let Some(access) = instance.native_access() else { return 0; };
    unsafe { out.write(birnam_abi::integer_native::ConstructionV1 {
        id: access.id, allocation_size: access.allocation_size, behavior_of: access.behavior_of,
        allocate: access.allocate, namespace: access.namespace, binding: access.binding,
        error_behavior_name: access.error_behavior_name, roots: access.roots,
    }) };
    1
}

/// Successful exact-Integer cases do not consult Float, including mixed entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_native_numeric_semantics_v5(
    entry: NativeEntry, ops: *const birnam_abi::integer::IntegerApiV1,
) -> u32 {
    if ops.is_null() { return 0; }
    let Some(instance) = admitted_instance(unsafe { &*ops }) else { return 0; };
    let context = ptr::from_ref(instance).cast_mut().cast();
    for (method, code) in [(add as native::Method, 1), (subtract, 2), (multiply, 3), (divide, 4), (modulo, 5)] {
        if entry == NativeEntry::new(method, context) { return code; }
    }
    let extension = FLOAT_EXTENSION.load(Ordering::Acquire);
    if !extension.is_null() && unsafe { (*extension).integer } == ptr::from_ref(instance) {
        for (method, code) in [(mixed_add as native::Method, 1), (mixed_sub, 2), (mixed_mul, 3), (mixed_div, 4)] {
            if entry == NativeEntry::new(method, extension.cast()) { return code; }
        }
    }
    0
}
unsafe fn floating_value(extension: &FloatExtension, access: Access, object: Oop) -> Option<f64> {
    let float = unsafe { *extension.float };
    if unsafe { (float.is_exact)(float.context, object) } == 0
        || fact(access, access.allocation_size, object)
            .ok()?
            .as_word()?
            < 8
        || fact(access, access.behavior_of, object).ok()?.as_object()? != float.behavior
    {
        return None;
    }
    Some(unsafe { (float.read)(float.context, object) })
}
unsafe fn mixed(
    extension: &FloatExtension,
    receiver: Oop,
    right: Option<Oop>,
    operation: u8,
) -> Result<Oop, Outcome> {
    let instance = unsafe { &*extension.integer };
    let access = instance.native_access().unwrap();
    if !known(access, receiver) || right.is_some_and(|value| !known(access, value)) {
        return Err(invalid());
    }
    let _receiver = Root::new(access.roots, receiver);
    let _right = right.map(|value| Root::new(access.roots, value));
    let left_integer = unsafe { integer(instance, access, receiver) };
    let Some(right) = right else {
        let Some(left) = left_integer else {
            return Ok(access.values.nil);
        };
        let float = unsafe { *extension.float };
        return Ok(unsafe { (float.make)(float.context, left as f64) });
    };
    // Existing mixed methods also work when copied to a Float receiver.
    let left = match left_integer {
        Some(value) => value as f64,
        None => match unsafe { floating_value(extension, access, receiver) } {
            Some(value) => value,
            None => return Ok(access.values.nil),
        },
    };
    if is_error(access, right)? {
        return Ok(right);
    }
    let right_integer = unsafe { integer(instance, access, right) };
    if let (Some(left), Some(right)) = (left_integer, right_integer) {
        return unsafe { calculate(instance, access, left, right, operation) };
    }
    let right = match right_integer {
        Some(value) => value as f64,
        None => match unsafe { floating_value(extension, access, right) } {
            Some(value) => value,
            None => {
                return error(
                    access,
                    b"typeMismatch",
                    if operation > 5 {
                        b"comparison needs a number"
                    } else {
                        b"Integer arithmetic needs a number"
                    },
                );
            }
        },
    };
    if operation == 3 && right == 0.0 {
        return error(access, b"zeroDivide", b"division by zero");
    }
    let float = unsafe { *extension.float };
    Ok(match operation {
        0 => unsafe { (float.make)(float.context, left + right) },
        1 => unsafe { (float.make)(float.context, left - right) },
        2 => unsafe { (float.make)(float.context, left * right) },
        3 => unsafe { (float.make)(float.context, left / right) },
        6 => boolean(access, left < right),
        7 => boolean(access, left <= right),
        8 => boolean(access, left > right),
        9 => boolean(access, left >= right),
        _ => unreachable!(),
    })
}
macro_rules! mixed_entry {
    ($name:ident, $operation:expr) => {
        unsafe extern "C" fn $name(
            context: *mut c_void,
            receiver: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            let Some(args) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
                return invalid();
            };
            let right = if $operation == 10 {
                if !args.is_empty() {
                    return invalid();
                }
                None
            } else {
                let [arg] = args else {
                    return invalid();
                };
                let Some(object) = arg.as_object() else {
                    return invalid();
                };
                Some(object)
            };
            answer(unsafe {
                mixed(
                    &*context.cast::<FloatExtension>(),
                    receiver,
                    right,
                    $operation,
                )
            })
        }
    };
}
mixed_entry!(mixed_add, 0);
mixed_entry!(mixed_sub, 1);
mixed_entry!(mixed_mul, 2);
mixed_entry!(mixed_div, 3);
mixed_entry!(mixed_lt, 6);
mixed_entry!(mixed_le, 7);
mixed_entry!(mixed_gt, 8);
mixed_entry!(mixed_ge, 9);
mixed_entry!(as_float, 10);
unsafe extern "C" fn install_float(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let instance = unsafe { &*context.cast::<Instance>() };
    if receiver != instance.identities()[1] || !FLOAT_EXTENSION.load(Ordering::Acquire).is_null() {
        return invalid();
    }
    let Some([arg]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(word) = arg.as_word() else {
        return invalid();
    };
    let float = word as *mut birnam_abi::float::FloatApiV1;
    if !unsafe { birnam_abi::float::valid(float) } {
        return invalid();
    }
    let access = instance.native_access().unwrap();
    let table = unsafe { *float };
    // The supplied descriptor must name objects in this Id, not another floor.
    if !known(access, table.behavior)
        || !known(access, table.prototype)
        || fact(access, access.behavior_of, table.prototype)
            .ok()
            .and_then(|value| value.as_object())
            != Some(table.behavior)
    {
        return invalid();
    }
    if unsafe { birnam_abi::float::attach(float) } != 1 {
        return invalid();
    }
    let extension = Box::into_raw(Box::new(FloatExtension {
        integer: instance,
        float,
    }));
    FLOAT_EXTENSION.store(extension, Ordering::Release);
    let result = (|| {
        let add_method = intern(access.id, b"addMethod")?;
        let methods: &[(&[u8], native::Method)] = &[
            (b"+", mixed_add),
            (b"-", mixed_sub),
            (b"*", mixed_mul),
            (b"/", mixed_div),
            (b"<", mixed_lt),
            (b"<=", mixed_le),
            (b">", mixed_gt),
            (b">=", mixed_ge),
            (b"asFloat", as_float),
        ];
        for (name, method) in methods {
            if !unsafe { extend_binding(instance, name) } {
                continue;
            }
            send(
                access.id,
                instance.identities()[0],
                add_method,
                &[
                    Operand::object(intern(access.id, name)?),
                    Operand::entry(NativeEntry::new(*method, extension.cast())),
                ],
            )?;
        }
        Ok(Operand::unit())
    })();
    match result {
        Ok(value) => Outcome::loaded(value),
        Err(error) => error,
    }
}
