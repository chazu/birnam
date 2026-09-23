use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::ffi::c_void;
use std::collections::HashMap;
pub(crate) struct Number {
    id: Protocol,
    roots: Oop,
    gc: Oop,
    model: Oop,
    namespace: Oop,
    string: Oop,
    integer: Oop,
    behavior: Oop,
    prototype: Oop,
    truth: Oop,
    falsity: Oop,
    nil: Oop,
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
    unsafe fn read(&self, value: Oop) -> Result<Option<u64>, Outcome> {
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
            Ok(Some(value.cast::<u64>().read()))
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
            n.integer,
            n.behavior,
            n.prototype,
            n.truth,
            n.falsity,
            n.nil,
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
    b"kind:message:",
    b"overflow",
    b"rootsDo:",
    b"referencesOf:do:",
    b"addMethod",
    b"register:",
    b"trace:with:",
    b"ownerOf:put:",
    b"at:put:",
    b"Word",
    b"WordVt",
    b"fromWord:",
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
        let integer = named(b"Integer")?;
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
            integer,
            behavior,
            prototype,
            truth,
            falsity,
            nil,
            names,
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
        for (name, value) in [(b"Word".as_slice(), prototype), (b"WordVt", behavior)] {
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
            &[Operand::object(id.intern_name(b"Word")?)],
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
unsafe fn execute(n: &Number, r: Oop, args: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
        if op == 0 {
            let [bits] = args else { return Err(invalid()) };
            return n.make(bits.as_word().ok_or_else(invalid)?);
        }
        if args.len() != usize::from(op == 2) {
            return Err(invalid());
        }
        let Some(value) = n.read(r)? else {
            return Ok(Operand::object(n.nil));
        };
        match op {
            1 => Ok(Operand::word(value as usize)),
            2 => Ok(n.boolean(n.read(object(args[0])?)? == Some(value))),
            3 => {
                if value > i64::MAX as u64 {
                    n.error(b"overflow", b"word out of Integer range")
                } else {
                    n.send(n.integer, b"fromWord:", &[Operand::word(value as usize)])
                }
            }
            4 => n.send(
                n.string,
                b"fromBytes:",
                &[Operand::bytes(format!("0x{value:x}").as_bytes())],
            ),
            _ => Err(invalid()),
        }
    }
}
entry!(from_bits, 0);
entry!(bits, 1);
entry!(equal, 2);
entry!(as_integer, 3);
entry!(print, 4);
const METHODS: &[(&[u8], Method)] = &[
    (b"fromBits:", from_bits),
    (b"bits", bits),
    (b"=", equal),
    (b"asInteger", as_integer),
    (b"printString", print),
];
