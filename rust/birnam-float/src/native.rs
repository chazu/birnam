use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::ffi::c_void;
use std::collections::HashMap;
struct Number {
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
    integer: Oop,
    names: HashMap<&'static [u8], Oop>,
    thread: std::thread::ThreadId,
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|p| !p.is_null()).ok_or_else(invalid)
}
fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
unsafe fn state<'a>(context: *mut c_void) -> Result<&'a Number, Outcome> {
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
    unsafe fn read(&self, value: Oop) -> Result<Option<f64>, Outcome> {
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
            Ok(Some(f64::from_bits(value.cast::<u64>().read())))
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
        let visitor = visitor
            .as_entry()
            .filter(|e| e.method.is_some())
            .ok_or_else(invalid)?;
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
            n.integer,
        ];
        values.extend(n.names.values().copied());
        for value in values {
            visitor.method.unwrap()(visitor.context, n.prototype, &Operand::object(value), 1)
                .into_result()?;
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
        state(context)?;
        let Some([value, visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        object(*value)?;
        visitor
            .as_entry()
            .filter(|e| e.method.is_some())
            .ok_or_else(invalid)?;
        // This family's raw numeric bits never represent object edges, including
        // instances with private behaviors. No foreign representation is read.
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

const NAMES: &[&[u8]] = &[
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
    b"Float",
    b"FloatVt",
    b"fromWord:",
    b"wordOf:",
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
        let integer = named(b"Integer")?;
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
            integer,
            thread: std::thread::current().id(),
        }));
        let n = &*pointer;
        for (name, method) in METHODS.iter().copied().chain([
            (b"rootsDo:".as_slice(), self::roots as Method),
            (b"referencesOf:do:", references),
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
        for (name, value) in [(b"Float".as_slice(), prototype), (b"FloatVt", behavior)] {
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
            &[Operand::object(id.intern_name(b"Float")?)],
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
    unsafe fn floating(&self, value: f64) -> Result<Operand, Outcome> {
        unsafe {
            self.send(
                self.prototype,
                b"fromBits:",
                &[Operand::word(value.to_bits() as usize)],
            )
        }
    }
    unsafe fn integer(&self, value: f64) -> Result<Operand, Outcome> {
        unsafe {
            let value = value.trunc();
            if !value.is_finite()
                || value < (i64::MIN as f64)
                || value >= 9_223_372_036_854_775_808.0
            {
                return self.error(b"overflow", b"float out of Integer range");
            }
            self.send(
                self.integer,
                b"fromWord:",
                &[Operand::word(value as i64 as usize)],
            )
        }
    }
    unsafe fn number(&self, value: Oop) -> Result<Option<f64>, Outcome> {
        unsafe {
            let word = self.send(self.integer, b"wordOf:", &[Operand::object(value)])?;
            if word.kind() != native::UNIT {
                return Ok(Some(word.as_word().ok_or_else(invalid)? as i64 as f64));
            }
            self.read(value)
        }
    }
}
unsafe fn execute(n: &Number, r: Oop, args: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
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
                .map(|v| Operand::word(v.to_bits() as usize))
                .unwrap_or_else(Operand::unit));
        }
        let Some(value) = n.read(r)? else {
            return if op == 12 {
                Err(invalid())
            } else {
                Ok(Operand::object(n.nil))
            };
        };
        if op <= 9 {
            let [right] = args else {
                return Err(invalid());
            };
            let right = object(*right)?;
            if op != 5 && n.is_error(right)? {
                return Ok(Operand::object(right));
            }
            let right = if op == 5 {
                n.read(right)?
            } else {
                n.number(right)?
            };
            let Some(right) = right else {
                return if op == 5 {
                    Ok(n.boolean(false))
                } else {
                    n.error(
                        b"typeMismatch",
                        if op > 5 {
                            b"comparison needs a number"
                        } else {
                            b"Float arithmetic needs a number"
                        },
                    )
                };
            };
            if op == 3 && right == 0.0 {
                return n.error(b"zeroDivide", b"division by zero");
            }
            return Ok(match op {
                0 => return n.floating(value + right),
                1 => return n.floating(value - right),
                2 => return n.floating(value * right),
                3 => return n.floating(value / right),
                5 => n.boolean(value == right),
                6 => n.boolean(value < right),
                7 => n.boolean(value <= right),
                8 => n.boolean(value > right),
                9 => n.boolean(value >= right),
                _ => return Err(invalid()),
            });
        }
        if !args.is_empty() {
            return Err(invalid());
        }
        match op {
            12 => Ok(Operand::word(value.to_bits() as usize)),
            13 => {
                let mut text = value.to_string();
                if !text.contains(['.', 'e', 'E']) {
                    text.push_str(".0");
                }
                n.send(n.string, b"fromBytes:", &[Operand::bytes(text.as_bytes())])
            }
            14 => n.floating(-value),
            15 => n.integer(value),
            16 => n.floating(value.abs()),
            17 => n.integer(value.floor()),
            18 => n.integer(value.ceil()),
            19 => n.integer(value.round()),
            20 if value < 0.0 => n.error(b"domain", b"square root of a negative number"),
            20 => n.floating(value.sqrt()),
            _ => Err(invalid()),
        }
    }
}
entry!(add, 0);
entry!(sub, 1);
entry!(mul, 2);
entry!(div, 3);
entry!(equal, 5);
entry!(lt, 6);
entry!(le, 7);
entry!(gt, 8);
entry!(ge, 9);
entry!(from_bits, 10);
entry!(bits_of, 11);
entry!(bits, 12);
entry!(print, 13);
entry!(negated, 14);
entry!(as_integer, 15);
entry!(abs, 16);
entry!(floor, 17);
entry!(ceiling, 18);
entry!(rounded, 19);
entry!(sqrt, 20);
const METHODS: &[(&[u8], Method)] = &[
    (b"+", add),
    (b"-", sub),
    (b"*", mul),
    (b"/", div),
    (b"=", equal),
    (b"<", lt),
    (b"<=", le),
    (b">", gt),
    (b">=", ge),
    (b"fromBits:", from_bits),
    (b"bitsOf:", bits_of),
    (b"bits", bits),
    (b"printString", print),
    (b"negated", negated),
    (b"asInteger", as_integer),
    (b"abs", abs),
    (b"floor", floor),
    (b"ceiling", ceiling),
    (b"rounded", rounded),
    (b"sqrt", sqrt),
];
