use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void};
use std::collections::HashMap;
pub(crate) struct Number {
    id: Protocol,
    roots: Oop,
    gc: Oop,
    model: Oop,
    namespace: Oop,
    string: Oop,
    behavior: Oop,
    prototype: Oop,
    truth: Oop,
    falsity: Oop,
    nil: Oop,
    extended: Cell<bool>,
    mixed: Cell<*mut mixed::Mixed>,
    names: HashMap<&'static [u8], Oop>,
    thread: std::thread::ThreadId,
}
pub(crate) fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
pub(crate) fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|p| !p.is_null()).ok_or_else(invalid)
}
pub(crate) fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
pub(crate) unsafe fn state<'a>(context: *mut c_void) -> Result<&'a Number, Outcome> {
    let n = unsafe { &*context.cast::<Number>() };
    if n.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(n)
}
impl Number {
    unsafe fn send(
        &self,
        r: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { self.id.send(r, self.names[name], args) }
    }
    unsafe fn rooted(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<Operand, Outcome>,
    ) -> Result<Operand, Outcome> {
        let mut tokens = Vec::new();
        let result = (|| unsafe {
            for value in values {
                tokens.push(
                    self.send(self.roots, b"retain:", &[Operand::object(*value)])?
                        .as_word()
                        .filter(|n| *n != 0)
                        .ok_or_else(invalid)?,
                );
            }
            f()
        })();
        for token in tokens {
            if unsafe { self.send(self.roots, b"release:", &[Operand::word(token)]) }?.as_word()
                != Some(1)
            {
                return Err(invalid());
            }
        }
        result
    }
    unsafe fn read(&self, value: Oop) -> Result<Option<i64>, Outcome> {
        unsafe {
            if self
                .send(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                .as_object()
                != Some(self.behavior)
                || self
                    .send(
                        self.id.receiver,
                        b"allocationSizeOf:",
                        &[Operand::object(value)],
                    )?
                    .as_word()
                    != Some(8)
            {
                return Ok(None);
            }
            Ok(Some(value.cast::<i64>().read()))
        }
    }
    unsafe fn make(&self, bits: usize) -> Result<Operand, Outcome> {
        unsafe {
            let value = object(self.send(
                self.model,
                b"allocate:bytes:",
                &[Operand::object(self.behavior), Operand::word(8)],
            )?)?;
            self.rooted(&[value], || {
                if self
                    .send(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                    .as_object()
                    != Some(self.behavior)
                    || self
                        .send(
                            self.id.receiver,
                            b"allocationSizeOf:",
                            &[Operand::object(value)],
                        )?
                        .as_word()
                        != Some(8)
                {
                    return Err(invalid());
                }
                value.cast::<usize>().write(bits);
                Ok(Operand::object(value))
            })
        }
    }
    unsafe fn binding(&self, name: &'static [u8]) -> Result<Operand, Outcome> {
        unsafe { self.send(self.namespace, b"at:", &[Operand::object(self.names[name])]) }
    }
    unsafe fn is_error(&self, value: Oop) -> Result<bool, Outcome> {
        // Preserve the existing live Namespace ErrorVt rule, including rebinding.
        unsafe {
            Ok(self.binding(b"ErrorVt")?.as_object()
                == self
                    .send(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                    .as_object())
        }
    }
    unsafe fn error(&self, kind: &'static [u8], message: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            let message =
                object(self.send(self.string, b"fromBytes:", &[Operand::bytes(message)])?)?;
            self.rooted(&[message], || {
                let Some(factory) = self.binding(b"Error")?.as_object() else {
                    return Ok(Operand::object(self.nil));
                };
                self.rooted(&[factory], || {
                    self.send(
                        factory,
                        b"kind:message:",
                        &[Operand::object(self.names[kind]), Operand::object(message)],
                    )
                })
            })
        }
    }
    fn boolean(&self, value: bool) -> Operand {
        Operand::object(if value { self.truth } else { self.falsity })
    }
}
unsafe extern "C" fn roots(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let n = state(context)?;
        let Some([visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        let mut values = vec![
            n.roots,
            n.gc,
            n.model,
            n.namespace,
            n.string,
            n.behavior,
            n.prototype,
            n.truth,
            n.falsity,
            n.nil,
        ];
        values.extend(n.names.values().copied());
        for value in values {
            n.id.send_named(visitor, b"visit:", &[Operand::object(value)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn references(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let n = state(context)?;
        let Some([value, visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let value = object(*value)?;
        let visitor = object(*visitor)?;
        let precise =
            n.id.send_named(n.gc, b"precisePayloads", &[])?
                .as_word()
                .ok_or_else(invalid)?
                != 0;
        if !precise {
            let bytes =
                n.id.send_named(
                    n.id.receiver,
                    b"allocationSizeOf:",
                    &[Operand::object(value)],
                )?
                .as_word()
                .ok_or_else(invalid)?;
            // Native callers may use an empty family object as a factory receiver.
            // Collection scans its actual padded extent; numeric value reads retain
            // their separate exact-size checks.
            for i in 0..bytes.div_ceil(size_of::<usize>()) {
                let bits = value.cast::<usize>().add(i).read();
                let edge =
                    n.id.send_named(n.id.receiver, b"allocationAt:", &[Operand::word(bits)])?;
                if edge.as_object().is_some() {
                    n.id.send_named(visitor, b"visit:", &[edge])?;
                }
            }
        }
        Ok(Operand::unit())
    })())
}
macro_rules! entry {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(
            context: *mut c_void,
            receiver: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            done((|| unsafe {
                let n = state(context)?;
                let args = borrowed_slice(args, count).ok_or_else(invalid)?;
                let mut values = vec![receiver];
                values.extend(
                    args.iter()
                        .filter_map(|v| v.as_object())
                        .filter(|v| !v.is_null()),
                );
                n.rooted(&values, || execute(n, receiver, args, $op))
            })())
        }
    };
}

#[path = "predicate.rs"]
mod predicate;

const NAMES: &[&[u8]] = &[
    b"numericOperation:left:right:",
    b"commitNumericPredicate:",
    b"evaluatePredicate:then:left:right:limit:in:epoch:with:",
    b"usesDefaultAllocation",
    b"canOmitIndexing",
    b"canOmitAllocationFor:",
    b"usesDefaultLookup:",
    b"behaviorEpoch",
    b"bind:to:",
    b"retain:",
    b"release:",
    b"behaviorOf:",
    b"allocationSizeOf:",
    b"allocate:bytes:",
    b"fromBytes:",
    b"at:",
    b"Error",
    b"ErrorVt",
    b"kind:message:",
    b"overflow",
    b"zeroDivide",
    b"typeMismatch",
    b"domain",
    b"rootsDo:",
    b"referencesOf:do:",
    b"addMethod",
    b"register:",
    b"trace:with:",
    b"ownerOf:put:",
    b"at:put:",
    b"Integer",
    b"IntegerVt",
    b"lookup",
    b"fromWord:",
    b"wordOf:",
    b"fromBits:",
    b"bitsOf:",
    b"asFloat",
    b"value:",
    b"evaluate:receiver:argument:with:",
];

unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let model = named(b"ObjectModel")?;
        let string = named(b"String")?;
        let truth = named(b"true")?;
        let falsity = named(b"false")?;
        let nil = named(b"nil")?;
        let parent = named(b"ObjectVt")?;

        let retain = id.intern_name(b"retain:")?;
        let release = id.intern_name(b"release:")?;
        let mut tokens = Vec::new();
        let mut keep = |v: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send(roots, retain, &[Operand::object(v)])?
                    .as_word()
                    .filter(|n| *n != 0)
                    .ok_or_else(invalid)?,
            );
            Ok(v)
        };
        let mut names = HashMap::new();
        for name in NAMES
            .iter()
            .copied()
            .chain(METHODS.iter().map(|(name, _)| *name))
        {
            names.insert(name, keep(id.intern_name(name)?)?);
        }
        let behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        id.send_named(
            model,
            b"setLayoutOf:to:rest:",
            &[
                Operand::object(behavior),
                Operand::bytes(&[]),
                Operand::word(0),
            ],
        )?;
        let prototype = keep(object(id.send_named(
            model,
            b"allocate:bytes:",
            &[Operand::object(behavior), Operand::word(8)],
        )?)?)?;
        // Zero-filled ordinary storage represents the numeric prototype (zero).
        let pointer = Box::into_raw(Box::new(Number {
            id,
            roots,
            gc,
            model,
            namespace,
            string,
            behavior,
            prototype,
            truth,
            falsity,
            nil,
            names,
            extended: Cell::new(false),
            mixed: Cell::new(core::ptr::null_mut()),
            thread: std::thread::current().id(),
        }));
        let n = &*pointer;
        for (name, method) in METHODS.iter().copied().chain([
            (b"rootsDo:".as_slice(), self::roots as Method),
            (b"referencesOf:do:", references),
            (b"evaluate:receiver:argument:with:", evaluate_numeric),
        ]) {
            n.send(
                behavior,
                b"addMethod",
                &[
                    Operand::object(n.names[name]),
                    Operand::entry(NativeEntry::new(method, pointer.cast())),
                ],
            )?;
        }
        n.send(roots, b"register:", &[Operand::object(prototype)])?;
        n.send(
            gc,
            b"trace:with:",
            &[Operand::object(behavior), Operand::object(prototype)],
        )?;
        n.send(
            id.receiver,
            b"ownerOf:put:",
            &[Operand::object(behavior), Operand::object(prototype)],
        )?;
        for (name, value) in [(b"Integer".as_slice(), prototype), (b"IntegerVt", behavior)] {
            n.send(
                namespace,
                b"at:put:",
                &[Operand::object(n.names[name]), Operand::object(value)],
            )?;
        }
        for token in tokens {
            id.send(roots, release, &[Operand::word(token)])?;
        }
        Ok(Operand::object(prototype))
    }
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([namespace]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let namespace = object(*namespace)?;
        let id = *id;
        let current = id.send_named(
            namespace,
            b"at:",
            &[Operand::object(id.intern_name(b"Integer")?)],
        )?;
        if current.kind() == native::OBJECT {
            return Ok(current);
        }
        construct(id, namespace)
    })())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
impl Number {
    unsafe fn integer(&self, value: i128) -> Result<Operand, Outcome> {
        unsafe {
            match i64::try_from(value) {
                Ok(v) => self.send(self.prototype, b"fromWord:", &[Operand::word(v as usize)]),
                Err(_) => self.error(b"overflow", b"integer overflow"),
            }
        }
    }
    unsafe fn calculate(&self, left: i64, right: i64, op: usize) -> Result<Operand, Outcome> {
        unsafe {
            if matches!(op, 3 | 4) && right == 0 {
                return self.error(b"zeroDivide", b"division by zero");
            }
            if op <= 4 {
                return self.integer(arithmetic(left, right, op).ok_or_else(invalid)?);
            }
            Ok(self.boolean(comparison(left, right, op).ok_or_else(invalid)?))
        }
    }
}
// Shared owner-local semantics for ordinary evaluation and the predicate path.
fn arithmetic(left: i64, right: i64, op: usize) -> Option<i128> {
    let (a, b) = (i128::from(left), i128::from(right));
    Some(match op {
        0 => a + b,
        1 => a - b,
        2 => a * b,
        3 if b != 0 => a / b,
        4 if b != 0 => a % b,
        _ => return None,
    })
}
fn comparison(left: i64, right: i64, op: usize) -> Option<bool> {
    Some(match op {
        5 => left == right,
        6 => (left as f64) < (right as f64),
        7 => (left as f64) <= (right as f64),
        8 => (left as f64) > (right as f64),
        9 => (left as f64) >= (right as f64),
        _ => return None,
    })
}

unsafe fn execute(n: &Number, r: Oop, args: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
        if op == 31 {
            let [other] = args else { return Err(invalid()) };
            let other = object(*other)?;
            let equal =
                r == other || matches!((n.read(r)?, n.read(other)?), (Some(a), Some(b)) if a == b);
            return Ok(n.boolean(equal));
        }

        if op == 30 {
            if !args.is_empty() {
                return Err(invalid());
            }
            let bits = n
                .read(r)?
                .map(|value| value as u64)
                .unwrap_or(r.addr() as u64);
            return Ok(Operand::word(mix(bits)));
        }
        if op == 10 {
            let [v] = args else {
                return Err(invalid());
            };
            return n.make(v.as_word().ok_or_else(invalid)?);
        }
        if op == 11 {
            let [v] = args else {
                return Err(invalid());
            };
            return Ok(n
                .read(object(*v)?)?
                .map(|v| Operand::word(v as usize))
                .unwrap_or_else(Operand::unit));
        }
        if op <= 9 {
            let [right] = args else {
                return Err(invalid());
            };
            let right = object(*right)?;
            let Some(left) = n.read(r)? else {
                return Ok(Operand::object(n.nil));
            };
            if op != 5 && n.is_error(right)? {
                return Ok(Operand::object(right));
            }
            let Some(right) = n.read(right)? else {
                return if op == 5 {
                    Ok(n.boolean(false))
                } else {
                    n.error(
                        b"typeMismatch",
                        if op == 4 {
                            b"mod: needs an Integer"
                        } else if op > 5 {
                            b"comparison needs a number"
                        } else {
                            b"Integer arithmetic needs a number"
                        },
                    )
                };
            };
            return n.calculate(left, right, op);
        }
        if op == 14 {
            let [end, block] = args else {
                return Err(invalid());
            };
            let (end, block) = (object(*end)?, object(*block)?);
            let (Some(mut value), Some(end)) = (n.read(r)?, n.read(end)?) else {
                return Ok(Operand::object(n.nil));
            };
            while value <= end {
                let arg = object(n.integer(i128::from(value))?)?;
                let answer =
                    n.rooted(&[arg], || n.send(block, b"value:", &[Operand::object(arg)]))?;
                let answer = object(answer)?;
                let mut fault = false;
                n.rooted(&[answer], || {
                    fault = n.is_error(answer)?;
                    Ok(Operand::unit())
                })?;
                if fault {
                    return Ok(Operand::object(answer));
                }
                if value == i64::MAX {
                    break;
                }
                value += 1;
            }
            return Ok(Operand::object(r));
        }
        if !args.is_empty() {
            return Err(invalid());
        }
        let Some(value) = n.read(r)? else {
            return if op == 13 {
                Ok(Operand::object(n.nil))
            } else {
                Err(invalid())
            };
        };
        match op {
            12 => Ok(Operand::word(value as usize)),
            13 => n.send(
                n.string,
                b"fromBytes:",
                &[Operand::bytes(value.to_string().as_bytes())],
            ),
            _ => Err(invalid()),
        }
    }
}
entry!(add, 0);
entry!(sub, 1);
entry!(mul, 2);
entry!(div, 3);
entry!(modulo, 4);
entry!(equal, 5);
entry!(lt, 6);
entry!(le, 7);
entry!(gt, 8);
entry!(ge, 9);
entry!(from_word, 10);
entry!(word_of, 11);
entry!(word, 12);
entry!(print, 13);
entry!(to_do, 14);
const METHODS: &[(&[u8], Method)] = &[
    (b"collectionEquals:", collection_equal),
    (b"collectionHash", collection_hash),
    (b"+", add),
    (b"-", sub),
    (b"*", mul),
    (b"/", div),
    (b"mod:", modulo),
    (b"=", equal),
    (b"<", lt),
    (b"<=", le),
    (b">", gt),
    (b">=", ge),
    (b"fromWord:", from_word),
    (b"canOmitIndexing", can_omit_indexing),
    (
        b"evaluatePredicate:then:left:right:limit:in:epoch:with:",
        predicate::evaluate,
    ),
    (b"wordOf:", word_of),
    (b"word", word),
    (b"printString", print),
    (b"to:do:", to_do),
    (b"installFloat:", mixed::install),
];

#[path = "mixed.rs"]
mod mixed;
fn mix(mut value: u64) -> usize {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u32 as usize
}
entry!(collection_hash, 30);

entry!(collection_equal, 31);

// Execution supplies an Object receiving a synchronous numeric evaluation send.
// Integer alone recognizes its exact executable/context and reads its payload.
unsafe extern "C" fn evaluate_numeric(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let n = state(c)?;
        let Some([selected, left, right, visitor]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        let selected = selected.as_entry().ok_or_else(invalid)?;
        if let Some(result) = mixed::evaluate(n, selected, *left, *right, *visitor) {
            return result;
        }
        let Some(op) = [add as Method, sub, mul, div, modulo]
            .iter()
            .position(|method| selected == NativeEntry::new(*method, c))
        else {
            return Ok(Operand::unit());
        };
        let left = object(*left)?;
        let right = object(*right)?;
        let visitor = object(*visitor)?;
        n.rooted(&[left, right, visitor], || {
            let Some(left) = n.read(left)? else {
                return Ok(Operand::object(n.nil));
            };
            if n.is_error(right)? {
                return Ok(Operand::object(right));
            }
            let Some(right) = n.read(right)? else {
                return n.error(
                    b"typeMismatch",
                    if op == 4 {
                        b"mod: needs an Integer"
                    } else {
                        b"Integer arithmetic needs a number"
                    },
                );
            };
            n.calculate_with(left, right, op, Some(visitor))
        })
    })())
}

impl Number {
    unsafe fn calculate_with(
        &self,
        left: i64,
        right: i64,
        op: usize,
        visitor: Option<Oop>,
    ) -> Result<Operand, Outcome> {
        unsafe {
            let Some(visitor) = visitor else {
                return self.calculate(left, right, op);
            };
            let args = [
                Operand::word(op + 1),
                Operand::word(left as usize),
                Operand::word(right as usize),
            ];
            let result = self.send(visitor, b"numericOperation:left:right:", &args)?;
            if result.kind() == native::UNIT {
                return self.calculate(left, right, op);
            }
            self.send(
                self.prototype,
                b"fromWord:",
                &[Operand::word(result.as_word().ok_or_else(invalid)?)],
            )
        }
    }
}

// Generated frame-slot indices and PC tokens use these two owner operations.
// A replacement must run normally; it is not classified by selector spelling.
unsafe extern "C" fn can_omit_indexing(
    c: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        if count != 0 {
            return Err(invalid());
        }
        let n = state(c)?;
        let epoch = n
            .send(n.id.receiver, b"behaviorEpoch", &[])?
            .as_word()
            .ok_or_else(invalid)?;
        if epoch == usize::MAX
            || n.send(
                n.id.receiver,
                b"usesDefaultLookup:",
                &[Operand::object(receiver)],
            )?
            .as_word()
                != Some(1)
        {
            return Ok(Operand::word(0));
        }
        for (name, method) in [
            (b"fromWord:".as_slice(), from_word as Method),
            (b"wordOf:".as_slice(), word_of as Method),
        ] {
            if n.send(
                n.id.receiver,
                b"bind:to:",
                &[Operand::object(n.names[name]), Operand::object(receiver)],
            )?
            .as_entry()
                != Some(NativeEntry::new(method, c))
            {
                return Ok(Operand::word(0));
            }
        }
        let permitted = n
            .send(
                n.model,
                b"canOmitAllocationFor:",
                &[Operand::object(n.behavior)],
            )?
            .as_word()
            == Some(1);
        Ok(Operand::word(usize::from(
            permitted && n.send(n.id.receiver, b"behaviorEpoch", &[])?.as_word() == Some(epoch),
        )))
    })())
}
