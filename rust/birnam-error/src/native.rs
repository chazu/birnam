//! Error's two reference slots use ordinary Object Model storage and tracing.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::ffi::c_void;
use std::collections::HashMap;
struct Error {
    id: Protocol,
    roots: Oop,
    model: Oop,
    string: Oop,
    behavior: Oop,
    factory: Oop,
    truth: Oop,
    falsity: Oop,
    nil: Oop,
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
unsafe fn state<'a>(context: *mut c_void) -> Result<&'a Error, Outcome> {
    let e = unsafe { &*context.cast::<Error>() };
    if e.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(e)
}
impl Error {
    unsafe fn send(
        &self,
        receiver: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { self.id.send(receiver, self.names[name], args) }
    }
    fn argument(&self, v: Operand) -> Result<Oop, Outcome> {
        v.as_object()
            .map(|v| if v.is_null() { self.nil } else { v })
            .ok_or_else(invalid)
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
    unsafe fn make(&self, kind: Oop, message: Oop) -> Result<Operand, Outcome> {
        unsafe {
            self.rooted(&[kind, message], || {
                let value = object(self.send(
                    self.model,
                    b"allocate:bytes:",
                    &[
                        Operand::object(self.behavior),
                        Operand::word(2 * size_of::<Oop>()),
                    ],
                )?)?;
                self.rooted(&[value], || {
                    for (index, edge) in [kind, message].into_iter().enumerate() {
                        self.send(
                            self.model,
                            b"slotAt:in:put:",
                            &[
                                Operand::word(index),
                                Operand::object(value),
                                Operand::object(edge),
                            ],
                        )?;
                    }
                    Ok(Operand::object(value))
                })
            })
        }
    }
}
unsafe extern "C" fn roots(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let e = state(context)?;
        let Some([visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let visitor = visitor
            .as_entry()
            .filter(|e| e.method.is_some())
            .ok_or_else(invalid)?;
        let mut values = vec![
            e.roots, e.model, e.string, e.behavior, e.factory, e.truth, e.falsity, e.nil,
        ];
        values.extend(e.names.values().copied());
        for value in values {
            visitor.method.unwrap()(visitor.context, e.factory, &Operand::object(value), 1)
                .into_result()?;
        }
        Ok(Operand::unit())
    })())
}
unsafe fn execute(
    e: &Error,
    receiver: Oop,
    args: &[Operand],
    operation: usize,
) -> Result<Operand, Outcome> {
    unsafe {
        match operation {
            0 => {
                let [kind, message] = args else {
                    return Err(invalid());
                };
                e.make(e.argument(*kind)?, e.argument(*message)?)
            }
            1 => {
                let [kind] = args else {
                    return Err(invalid());
                };
                e.make(e.argument(*kind)?, e.nil)
            }
            2 => {
                let [message] = args else {
                    return Err(invalid());
                };
                e.make(e.names[b"error".as_slice()], e.argument(*message)?)
            }
            3 | 4 => {
                if !args.is_empty() {
                    return Err(invalid());
                }
                e.rooted(&[receiver], || {
                    let value = e.send(
                        e.model,
                        b"slotAt:in:",
                        &[Operand::word(operation - 3), Operand::object(receiver)],
                    )?;
                    Ok(if value.kind() == native::UNIT {
                        Operand::object(e.nil)
                    } else {
                        value
                    })
                })
            }
            5 | 6 => {
                if !args.is_empty() {
                    return Err(invalid());
                }
                Ok(Operand::object(if operation == 5 {
                    e.truth
                } else {
                    e.falsity
                }))
            }
            7 => {
                if args.is_empty() {
                    return Err(invalid());
                }
                Ok(Operand::object(receiver))
            }
            8 => {
                let Some(selector) = args.first() else {
                    return Err(invalid());
                };
                let selector = object(*selector)?;
                let mut inputs = vec![receiver, selector];
                inputs.extend(
                    args[1..]
                        .iter()
                        .filter_map(|v| v.as_object())
                        .filter(|p| !p.is_null()),
                );
                e.rooted(&inputs, || {
                    let raw = e
                        .send(
                            e.id.receiver,
                            b"symbolBytesOf:",
                            &[Operand::object(selector)],
                        )?
                        .as_bytes()
                        .ok_or_else(invalid)?;
                    let mut text = b"does not understand #".to_vec();
                    text.extend_from_slice(
                        borrowed_bytes(raw.pointer, raw.length).ok_or_else(invalid)?,
                    );
                    let message =
                        object(e.send(e.string, b"fromBytes:", &[Operand::bytes(&text)])?)?;
                    e.rooted(&[message], || {
                        // Faults use the current public factory, including replacements.
                        e.send(
                            e.factory,
                            b"kind:message:",
                            &[
                                Operand::object(e.names[b"doesNotUnderstand".as_slice()]),
                                Operand::object(message),
                            ],
                        )
                    })
                })
            }
            _ => Err(invalid()),
        }
    }
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
                execute(
                    state(context)?,
                    receiver,
                    borrowed_slice(args, count).ok_or_else(invalid)?,
                    $op,
                )
            })())
        }
    };
}
entry!(kind_message, 0);
entry!(kind, 1);
entry!(message, 2);
entry!(read_kind, 3);
entry!(read_message, 4);
entry!(yes, 5);
entry!(no, 6);
entry!(propagate, 7);
entry!(missing, 8);
const NAMES: &[&[u8]] = &[
    b"retain:",
    b"release:",
    b"allocate:bytes:",
    b"slotAt:in:",
    b"slotAt:in:put:",
    b"setLayoutOf:to:rest:",
    b"fromBytes:",
    b"symbolBytesOf:",
    b"kind:message:",
    b"kind:",
    b"message:",
    b"kind",
    b"message",
    b"isError",
    b"doesNotUnderstand:",
    b"doesNotUnderstand",
    b"error",
    b"rootsDo:",
    b"register:",
    b"ownerOf:put:",
    b"addMethod",
    b"at:put:",
    b"Error",
    b"ErrorVt",
    b"ErrorFactoryVt",
];
unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
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
        for name in NAMES {
            names.insert(*name, keep(id.intern_name(name)?)?);
        }
        let behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        let factory_behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        let factory = keep(object(id.send_named(
            factory_behavior,
            b"allocate",
            &[Operand::word(0)],
        )?)?)?;
        let pointer = Box::into_raw(Box::new(Error {
            id,
            roots,
            model,
            string,
            behavior,
            factory,
            truth,
            falsity,
            nil,
            names,
            thread: std::thread::current().id(),
        }));
        let e = &*pointer;
        e.send(
            model,
            b"setLayoutOf:to:rest:",
            &[
                Operand::object(behavior),
                Operand::bytes(&[1, 1]),
                Operand::word(0),
            ],
        )?;
        for (target, name, method) in [
            (
                behavior,
                b"doesNotUnderstand:".as_slice(),
                propagate as Method,
            ),
            (behavior, b"isError", yes),
            (behavior, b"kind", read_kind),
            (behavior, b"message", read_message),
            (factory_behavior, b"kind:message:", kind_message),
            (factory_behavior, b"kind:", kind),
            (factory_behavior, b"message:", message),
            (factory_behavior, b"rootsDo:", self::roots),
            (parent, b"isError", no),
            (parent, b"doesNotUnderstand:", missing),
        ] {
            e.send(
                target,
                b"addMethod",
                &[
                    Operand::object(e.names[name]),
                    Operand::entry(NativeEntry::new(method, pointer.cast())),
                ],
            )?;
        }
        e.send(roots, b"register:", &[Operand::object(factory)])?;
        e.send(
            id.receiver,
            b"ownerOf:put:",
            &[Operand::object(factory_behavior), Operand::object(factory)],
        )?;
        for (name, value) in [
            (b"Error".as_slice(), factory),
            (b"ErrorVt", behavior),
            (b"ErrorFactoryVt", factory_behavior),
        ] {
            e.send(
                namespace,
                b"at:put:",
                &[Operand::object(e.names[name]), Operand::object(value)],
            )?;
        }
        for token in tokens {
            id.send(roots, release, &[Operand::word(token)])?;
        }
        Ok(Operand::object(factory))
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
            &[Operand::object(id.intern_name(b"Error")?)],
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
