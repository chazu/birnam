//! Text methods installed after the completed numeric families exist.
use super::{
    native::{self as base, Access, Root},
    storage::Instance,
};
use birnam_abi::{
    Oop, base_values,
    float::FloatApiV1,
    id_protocol::{self as native, NativeEntry, Operand, Outcome},
    integer::IntegerApiV1,
    root_registry,
};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
struct Text {
    instance: *const Instance,
    access: Access,
    values: base_values::ValuesV1,
    integer: *mut IntegerApiV1,
    float: *mut FloatApiV1,
    print_string: Oop,
}
static TEXT: AtomicPtr<Text> = AtomicPtr::new(ptr::null_mut());
static READY: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn roots(mark: birnam_abi::MarkFn) {
    let text = TEXT.load(Ordering::Acquire);
    if text.is_null() {
        return;
    }
    let text = unsafe { &*text };
    let integer = unsafe { *text.integer };
    let float = unsafe { *text.float };
    let objects = [
        text.print_string,
        text.values.nil,
        text.values.true_object,
        text.values.false_object,
        integer.behavior,
        integer.prototype,
        float.behavior,
        float.prototype,
    ];
    for object in objects {
        unsafe { mark(object) };
    }
}
impl Text {
    fn known(&self, object: Oop) -> bool {
        base::fact(self.access, self.access.allocation_size, object)
            .ok()
            .and_then(|value| value.as_word())
            .is_some()
    }
    fn behavior(&self, object: Oop) -> Option<Oop> {
        base::fact(self.access, self.access.behavior_of, object)
            .ok()?
            .as_object()
    }
    unsafe fn copy(&self, object: Oop) -> Option<Vec<u8>> {
        let ops = unsafe { *(*self.instance).operations() };
        if self.behavior(object)? != ops.behavior {
            return None;
        }
        let mut length = 0;
        let bytes = unsafe { (ops.bytes)(ops.context, object, &mut length) };
        if length == 0 {
            return Some(Vec::new());
        }
        if bytes.is_null() {
            return None;
        }
        Some(unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec())
    }
    unsafe fn make_string(&self, bytes: &[u8]) -> Oop {
        let ops = unsafe { *(*self.instance).operations() };
        unsafe { (ops.make)(ops.context, bytes.as_ptr(), bytes.len()) }
    }
    unsafe fn read_integer(&self, object: Oop) -> Option<i64> {
        let ops = unsafe { *self.integer };
        if self.behavior(object)? != ops.behavior
            || base::fact(self.access, self.access.allocation_size, object)
                .ok()?
                .as_word()?
                < 8
        {
            return None;
        }
        Some(unsafe { (ops.read)(ops.context, object) })
    }
    unsafe fn make_integer(&self, value: i64) -> Oop {
        let ops = unsafe { *self.integer };
        unsafe { (ops.make)(ops.context, value) }
    }
    unsafe fn make_float(&self, value: f64) -> Oop {
        let ops = unsafe { *self.float };
        unsafe { (ops.make)(ops.context, value) }
    }
    unsafe fn execute(
        &self,
        receiver: Oop,
        args: &[Oop],
        operation: usize,
    ) -> Result<Oop, Outcome> {
        let nil = self.values.nil;
        if operation == 0 {
            return Ok(receiver);
        }
        if operation == 1 {
            let mut other = args[0];
            let behavior = unsafe { (*self.instance).identities()[0] };
            if self.behavior(other) != Some(behavior) {
                other = base::send(self.access.id, other, self.print_string, &[])?
                    .as_object()
                    .ok_or_else(base::invalid)?;
            }
            if !self.known(other) {
                return Err(base::invalid());
            }
            let _other = Root::new(self.access.roots, other);
            let Some(mut left) = (unsafe { self.copy(receiver) }) else {
                return Ok(nil);
            };
            if let Some(right) = unsafe { self.copy(other) } {
                left.extend_from_slice(&right);
            }
            return Ok(unsafe { self.make_string(&left) });
        }
        if operation == 2 {
            let equal = unsafe { self.copy(receiver) }
                .zip(unsafe { self.copy(args[0]) })
                .is_some_and(|(a, b)| a == b);
            return Ok(if equal {
                self.values.true_object
            } else {
                self.values.false_object
            });
        }
        // The callbacks for index recognition/reading retain their original order.
        let indices = if matches!(operation, 4 | 5 | 6) {
            if args.iter().any(|value| {
                let ops = unsafe { *self.integer };
                (unsafe { (ops.is_exact)(ops.context, *value) }) == 0
            }) {
                return Ok(nil);
            }
            let mut values = Vec::with_capacity(args.len());
            for value in args {
                let Some(value) = (unsafe { self.read_integer(*value) }) else {
                    return Ok(nil);
                };
                values.push(value);
            }
            values
        } else {
            Vec::new()
        };
        let Some(bytes) = (unsafe { self.copy(receiver) }) else {
            return Ok(nil);
        };
        Ok(match operation {
            3 => unsafe { self.make_integer(bytes.len() as i64) },
            4 | 5 => {
                let index = indices[0];
                if index < 0 || index as usize >= bytes.len() {
                    return Ok(nil);
                }
                let byte = bytes[index as usize];
                if operation == 4 {
                    unsafe { self.make_string(&[byte]) }
                } else {
                    unsafe { self.make_integer(i64::from(byte)) }
                }
            }
            6 => {
                let (from, to) = (indices[0], indices[1]);
                if from < 0 || to < from || to as usize > bytes.len() {
                    return Ok(nil);
                }
                unsafe { self.make_string(&bytes[from as usize..to as usize]) }
            }
            7 => unsafe { self.make_string(trimmed_bytes(&bytes)) },
            8 => {
                let Some(needle) = (unsafe { self.copy(args[0]) }) else {
                    return Ok(nil);
                };
                let found = if needle.is_empty() {
                    Some(0)
                } else {
                    bytes
                        .windows(needle.len())
                        .position(|window| window == needle)
                };
                match found {
                    Some(index) => unsafe { self.make_integer(index as i64) },
                    None => nil,
                }
            }
            9 => unsafe { self.make_integer(bracket_depth(&bytes)) },
            10 => {
                let Some(needle) = (unsafe { self.copy(args[0]) }) else {
                    return Ok(nil);
                };
                let Some(replacement) = (unsafe { self.copy(args[1]) }) else {
                    return Ok(nil);
                };
                unsafe { self.make_string(&replace(&bytes, &needle, &replacement)) }
            }
            11 => {
                // The legacy selector is C-string based, so preserve its NUL boundary.
                let end = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(bytes.len());
                base::intern(self.access.id, &bytes[..end])?
            }
            12 | 13 => {
                let Ok(text) = core::str::from_utf8(super::trim_ascii(&bytes)) else {
                    return Ok(nil);
                };
                if operation == 12 || !text.contains(['.', 'e', 'E']) {
                    if let Ok(value) = text.parse::<i64>() {
                        return Ok(unsafe { self.make_integer(value) });
                    }
                    if operation == 12 {
                        return Ok(nil);
                    }
                }
                match text.parse::<f64>() {
                    Ok(value) => unsafe { self.make_float(value) },
                    Err(_) => nil,
                }
            }
            _ => unreachable!(),
        })
    }
}
macro_rules! entry {
    ($name:ident, $operation:expr, $arity:expr) => {
        unsafe extern "C" fn $name(
            context: *mut c_void,
            receiver: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            if count != $arity {
                return base::invalid();
            }
            let Some(args) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
                return base::invalid();
            };
            let Some(args) = args
                .iter()
                .map(|arg| arg.as_object())
                .collect::<Option<Vec<_>>>()
            else {
                return base::invalid();
            };
            let text = unsafe { &*context.cast::<Text>() };
            if !text.known(receiver) || args.iter().any(|object| !text.known(*object)) {
                return base::invalid();
            }
            let _roots: Vec<_> = core::iter::once(receiver)
                .chain(args.iter().copied())
                .map(|object| Root::new(text.access.roots, object))
                .collect();
            match unsafe { text.execute(receiver, &args, $operation) } {
                Ok(object) => Outcome::loaded(Operand::object(object)),
                Err(error) => error,
            }
        }
    };
}
entry!(print, 0, 0);
entry!(comma, 1, 1);
entry!(equals, 2, 1);
entry!(size, 3, 0);
entry!(at, 4, 1);
entry!(byte_at, 5, 1);
entry!(copy, 6, 2);
entry!(trimmed, 7, 0);
entry!(index_of, 8, 1);
entry!(depth, 9, 0);
entry!(replace_with, 10, 2);
entry!(as_symbol, 11, 0);
entry!(as_integer, 12, 0);
entry!(as_number, 13, 0);
pub(crate) unsafe extern "C" fn install(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let instance = unsafe { &*context.cast::<Instance>() };
    if !TEXT.load(Ordering::Acquire).is_null() || receiver != instance.identities()[1] {
        return base::invalid();
    }
    let Some([values, integer, float]) = (unsafe { birnam_abi::borrowed_slice(args, count) })
    else {
        return base::invalid();
    };
    let (Some(values), Some(integer), Some(float)) =
        (values.as_word(), integer.as_word(), float.as_word())
    else {
        return base::invalid();
    };
    let (values, integer, float) = (
        values as *const base_values::ValuesV1,
        integer as *mut IntegerApiV1,
        float as *mut FloatApiV1,
    );
    if !unsafe {
        base_values::valid(values)
            && birnam_abi::integer::valid(integer)
            && birnam_abi::float::valid(float)
    } {
        return base::invalid();
    }
    let access = instance.native_access().unwrap();
    let result = (|| {
        let text = Box::into_raw(Box::new(Text {
            instance,
            access,
            values: unsafe { *values },
            integer,
            float,
            print_string: base::intern(access.id, b"printString")?,
        }));
        TEXT.store(text, Ordering::Release);
        unsafe { root_registry::register(access.roots.api, roots) };
        let bind = base::intern(access.id, b"addMethod")?;
        let lookup = base::intern(access.id, b"lookup")?;
        let methods: &[(&[u8], native::Method)] = &[
            (b"printString", print),
            (b",", comma),
            (b"=", equals),
            (b"size", size),
            (b"at:", at),
            (b"byteAt:", byte_at),
            (b"copyFrom:to:", copy),
            (b"trimmed", trimmed),
            (b"indexOf:", index_of),
            (b"bracketDepth", depth),
            (b"replace:with:", replace_with),
            (b"asSymbol", as_symbol),
            (b"asInteger", as_integer),
            (b"asNumber", as_number),
        ];
        for (name, method) in methods {
            let selector = base::intern(access.id, name)?;
            if base::send(
                access.id,
                instance.identities()[0],
                lookup,
                &[Operand::object(selector)],
            )?
            .kind()
                != native::UNIT
            {
                continue;
            }
            base::send(
                access.id,
                instance.identities()[0],
                bind,
                &[
                    Operand::object(selector),
                    Operand::entry(NativeEntry::new(*method, text.cast())),
                ],
            )?;
        }
        READY.store(true, Ordering::Release);
        Ok(Operand::unit())
    })();
    match result {
        Ok(value) => Outcome::loaded(value),
        Err(error) => error,
    }
}

pub(crate) fn bracket_depth(bytes: &[u8]) -> i64 {
    let mut depth = 0i64;
    let mut in_string = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if byte == b'\\' {
                index = index.saturating_add(2);
                continue;
            }
            if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    depth
}
pub(crate) fn replace(bytes: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    if needle.is_empty() {
        return bytes.to_vec();
    }
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if index + needle.len() <= bytes.len() && bytes[index..index + needle.len()] == needle[..] {
            output.extend_from_slice(replacement);
            index += needle.len();
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    output
}

pub(crate) fn trimmed_bytes(bytes: &[u8]) -> &[u8] {
    let space = |byte: &u8| matches!(*byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c);
    let start = bytes
        .iter()
        .position(|byte| !space(byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !space(byte))
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}
