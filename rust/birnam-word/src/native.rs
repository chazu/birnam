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

const NAMES: &[&[u8]] = &[
    b"retain:",
    b"release:",
    b"behaviorOf:",
    b"allocationSizeOf:",
    b"allocate:bytes:",
    b"at:",
    b"rootsDo:",
    b"referencesOf:do:",
    b"addMethod",
    b"register:",
    b"trace:with:",
    b"ownerOf:put:",
    b"at:put:",
    b"Word",
    b"WordVt",
];

unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let model = named(b"ObjectModel")?;
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
        if op >= 5 {
            let value = n.read(r)?.ok_or_else(invalid)?;
            if op == 5 {
                if !args.is_empty() { return Err(invalid()); }
                return n.make(!value as usize);
            }
            let [right] = args else { return Err(invalid()); };
            let right = n.read(object(*right)?)?.ok_or_else(invalid)?;
            if op == 16 { return Ok(n.boolean(value < right)); }
            let result = crate::operations::calculate(value, right, op).ok_or_else(invalid)?;
            return n.make(result as usize);
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
            _ => Err(invalid()),
        }
    }
}
entry!(from_bits, 0);
entry!(bits, 1);
entry!(equal, 2);
entry!(bit_not, 5);
entry!(wrapping_add, 6);
entry!(wrapping_sub, 7);
entry!(wrapping_mul, 8);
entry!(unsigned_div, 9);
entry!(unsigned_rem, 10);
entry!(bit_and, 11);
entry!(bit_or, 12);
entry!(bit_xor, 13);
entry!(shift_left, 14);
entry!(shift_right, 15);
entry!(unsigned_less, 16);
const METHODS: &[(&[u8], Method)] = &[
    (b"bitNot", bit_not),
    (b"wrappingAdd:", wrapping_add),
    (b"wrappingSubtract:", wrapping_sub),
    (b"wrappingMultiply:", wrapping_mul),
    (b"unsignedDivide:", unsigned_div),
    (b"unsignedRemainder:", unsigned_rem),
    (b"bitAnd:", bit_and),
    (b"bitOr:", bit_or),
    (b"bitXor:", bit_xor),
    (b"shiftLeft:", shift_left),
    (b"shiftRight:", shift_right),
    (b"unsignedLessThan:", unsigned_less),
    (b"fromBits:", from_bits),
    (b"bits", bits),
    (b"=", equal),
];
