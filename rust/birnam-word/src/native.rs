use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::ffi::c_void;
use std::{cell::RefCell, collections::HashMap};
pub(crate) struct Number {
    pub(super) id: Protocol,
    pub(super) binding_guard: birnam_abi::binding_guard::BindingGuard,
    roots: Oop,
    gc: Oop,
    pub(super) model: Oop,
    namespace: Oop,
    pub(super) behavior: Oop,
    pub(super) prototype: Oop,
    truth: Oop,
    falsity: Oop,
    nil: Oop,
    pub(super) names: HashMap<&'static [u8], Oop>,
    thread: std::thread::ThreadId,
    scoped: RefCell<Vec<Oop>>,
    pub(super) expression_meanings: core::cell::Cell<(usize, u32)>,
}
// Owner-local roots are reported through the registered rootsDo: protocol.
// Nested calls append; dropping a scope restores only its own suffix.
struct Scope<'a> { values: &'a RefCell<Vec<Oop>>, start: usize }
impl Drop for Scope<'_> {
    fn drop(&mut self) { self.values.borrow_mut().truncate(self.start); }
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
    pub(super) unsafe fn send(
        &self,
        r: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { self.id.send(r, self.names[name], args) }
    }
    // Only newly allocated outputs need this local protection. Incoming
    // receiver/arguments are covered by the native Id invocation contract.
    pub(super) unsafe fn rooted(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<Operand, Outcome>,
    ) -> Result<Operand, Outcome> {
        if values.iter().any(|value| value.is_null()) { return Err(invalid()); }
        let start = self.scoped.borrow().len();
        self.scoped.borrow_mut().extend(values.iter().copied());
        let _scope = Scope { values: &self.scoped, start };
        f()
    }

    unsafe fn matches_representation(&self, value: Oop) -> Result<bool, Outcome> {
        unsafe {
            match self.send(self.id.receiver, b"tryRepresentationIdentityOf:behavior:bytes:",
                &[Operand::object(value), Operand::object(self.behavior), Operand::word(8)]) {
                Ok(result) if result.kind() == native::UNIT => {},
                Err(error) if error.status == native::UNBOUND => {},
                Ok(result) => return result.as_word().map(|identity| identity != 0).ok_or_else(invalid),
                Err(error) => return Err(error),
            }
            if self.send(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?.as_object() != Some(self.behavior) {
                return Ok(false);
            }
            Ok(self.send(self.id.receiver, b"allocationSizeOf:", &[Operand::object(value)])?.as_word() == Some(8))
        }
    }
    // Canonical expression inputs have no intervening callback between their
    // shape facts and these owner-local reads. Decline retains ordered reads.
    pub(super) unsafe fn read_inputs(&self, inputs: &[Operand], values: &mut [u64]) -> Result<bool, Outcome> {
        if inputs.len() > 32 || inputs.len() > values.len() { return Err(invalid()); }
        if inputs.len() >= 2 {
            let mut addresses = [0usize; 32];
            for (address, input) in addresses.iter_mut().zip(inputs) { *address = object(*input)?.addr(); }
            let addresses = &addresses[..inputs.len()];
            let bytes = unsafe { core::slice::from_raw_parts(addresses.as_ptr().cast::<u8>(), size_of_val(addresses)) };
            match unsafe { self.send(self.id.receiver, b"tryRepresentationsOf:behavior:bytes:",
                &[Operand::bytes(bytes), Operand::object(self.behavior), Operand::word(8)]) } {
                Ok(reply) if reply.kind() == native::UNIT => {},
                Err(error) if error.status == native::UNBOUND => {},
                Ok(reply) => match reply.as_word() {
                    Some(0) => return Ok(false),
                    Some(1) => {
                        for (input, value) in inputs.iter().zip(values) {
                            *value = unsafe { object(*input)?.cast::<u64>().read() };
                        }
                        return Ok(true);
                    },
                    _ => return Err(invalid()),
                },
                Err(error) => return Err(error),
            }
        }
        for (input, value) in inputs.iter().zip(values) {
            let Some(bits) = (unsafe { self.read(object(*input)?) })? else { return Ok(false); };
            *value = bits;
        }
        Ok(true)
    }
    pub(super) unsafe fn read(&self, value: Oop) -> Result<Option<u64>, Outcome> {
        unsafe {
            if !self.matches_representation(value)?
            {
                return Ok(None);
            }
            Ok(Some(value.cast::<u64>().read()))
        }
    }
    pub(super) unsafe fn make(&self, bits: usize) -> Result<Operand, Outcome> {
        unsafe {
            let value = object(self.send(
                self.model,
                b"allocate:bytes:",
                &[Operand::object(self.behavior), Operand::word(8)],
            )?)?;
            self.rooted(&[value], || {
                if !self.matches_representation(value)?
                {
                    return Err(invalid());
                }
                value.cast::<usize>().write(bits);
                Ok(Operand::object(value))
            })
        }
    }
    pub(super) fn boolean(&self, value: bool) -> Operand {
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
        values.extend(n.scoped.borrow().iter().copied());
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
        pub(super) unsafe extern "C" fn $name(
            context: *mut c_void,
            receiver: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            done((|| unsafe {
                let n = state(context)?;
                let args = borrowed_slice(args, count).ok_or_else(invalid)?;
                // Id callers keep receiver and Object arguments live through
                // this complete call, including dependency callbacks. Fresh
                // values created inside execute still need their own roots.
                execute(n, receiver, args, $op)
            })())
        }
    };
}

const NAMES: &[&[u8]] = &[
    b"behaviorEpoch",
    b"usesDefaultAllocation",
    b"canOmitAllocationFor:",
    b"usesDefaultLookup:",
    b"bind:to:",
    b"nativeBindingsMatch:",
    b"nativeBindingQuerySemantics:for:",
    b"retain:",
    b"release:",
    b"behaviorOf:",
    b"allocationSizeOf:",
    b"tryRepresentationIdentityOf:behavior:bytes:",
    b"tryRepresentationsOf:behavior:bytes:",
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
        let binding_guard = birnam_abi::binding_guard::BindingGuard::discover(id,
            birnam_abi::binding_guard::SERVICES.map(|name| names[name]),
            names[b"nativeBindingQuerySemantics:for:".as_slice()]);
        let pointer = Box::into_raw(Box::new(Number {
            id,
            binding_guard,
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
            scoped: RefCell::new(Vec::new()),
            expression_meanings: core::cell::Cell::new((usize::MAX, 0)),
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
        if op == 17 {
            let [left, right] = args else {
                return Err(invalid());
            };
            return n.make(usize::from(object(*left)? == object(*right)?));
        }
        if op == 18 {
            let [zero, nonzero] = args else {
                return Err(invalid());
            };
            let value = n.read(r)?.ok_or_else(invalid)?;
            let (zero, nonzero) = (object(*zero)?, object(*nonzero)?);
            return Ok(Operand::object(if value == 0 { zero } else { nonzero }));
        }
        // Only well-shaped binary requests use the optional batch. Malformed
        // arity/carriers retain the original left-read-before-rejection order.
        if (6..=16).contains(&op) && !r.is_null() && args.len() == 1
            && args[0].as_object().is_some_and(|v| !v.is_null())
        {
            let mut values = [0u64; 2];
            if !n.read_inputs(&[Operand::object(r), args[0]], &mut values)? { return Err(invalid()); }
            if op == 16 { return Ok(n.boolean(values[0] < values[1])); }
            let value = crate::operations::calculate(values[0], values[1], op).ok_or_else(invalid)?;
            return n.make(value as usize);
        }
        if op >= 5 {
            let value = n.read(r)?.ok_or_else(invalid)?;
            if op == 5 {
                if !args.is_empty() {
                    return Err(invalid());
                }
                return n.make(!value as usize);
            }
            let [right] = args else {
                return Err(invalid());
            };
            let right = n.read(object(*right)?)?.ok_or_else(invalid)?;
            if op == 16 {
                return Ok(n.boolean(value < right));
            }
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
entry!(same_object, 17);
entry!(select_object, 18);
// Pure data selection: native callers keep Object operands live throughout the
// synchronous Id call. No allocation, payload read, dependency send or callback
// occurs here, so no additional root registrations or allocator checks are needed.
unsafe extern "C" fn select_same_object(
    context: *mut c_void, _: Oop, args: *const Operand, count: usize,
) -> Outcome {
    done((|| unsafe {
        state(context)?;
        let [left, right, equal, different] = borrowed_slice(args, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let (left, right, equal, different) =
            (object(*left)?, object(*right)?, object(*equal)?, object(*different)?);
        Ok(Operand::object(if left == right { equal } else { different }))
    })())
}

const METHODS: &[(&[u8], Method)] = &[
    (b"selectIfSameObject:with:ifEqual:ifDifferent:", select_same_object),
    (b"tryPrimitiveExpression:", crate::expression::evaluate),
    (
        b"tryPrimitiveExpressionAtEpoch:",
        crate::expression::evaluate_at_epoch,
    ),
    (b"sameObject:with:", same_object),
    (b"selectIfZero:ifNonzero:", select_object),
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
