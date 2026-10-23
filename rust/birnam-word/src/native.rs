//! Unsigned Word values own their eight-byte payload; all message values are Oops.
use birnam_abi::{
    self as abi, Oop,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicPtr, Ordering},
};
pub(crate) struct Number {
    pub(crate) id: Protocol,
    namespace: Oop,
    roots: Oop,
    gc: Oop,
    pub(crate) model: Oop,
    pub(crate) behavior: Oop,
    pub(crate) prototype: Oop,
    truth: Oop,
    falsity: Oop,
    nil: Oop,
    pub(crate) names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    pub(crate) witnesses: [Cell<Oop>; 19],
    pub(crate) expression_meanings: Cell<(usize, u32)>,
    ready: Cell<bool>,
}
struct Scope<'a> {
    values: &'a RefCell<Vec<Oop>>,
    start: usize,
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.values.borrow_mut().truncate(self.start);
    }
}
static INSTANCE: AtomicPtr<Number> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
pub(crate) unsafe fn state<'a>(c: Oop) -> Result<&'a Number, Oop> {
    let raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        let p = API.load(Ordering::Acquire);
        if p.is_null() {
            std::process::abort();
        }
        return Err(unsafe { (*p).invalid });
    }
    let n = unsafe { &*raw };
    if c == n.prototype {
        Ok(n)
    } else {
        Err(n.invalid())
    }
}
impl Number {
    pub(crate) fn invalid(&self) -> Oop {
        abi::object_reference(self.id.invalid)
    }
    pub(crate) unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[name], a) }
    }
    pub(crate) fn boolean(&self, v: bool) -> Oop {
        if v { self.truth } else { self.falsity }
    }
    fn scope(&self) -> Scope<'_> {
        Scope {
            values: &self.scoped,
            start: self.scoped.borrow().len(),
        }
    }
    fn keep(&self, v: Oop) -> Oop {
        self.scoped.borrow_mut().push(v);
        v
    }
    unsafe fn matches_representation(&self, value: Oop) -> Result<bool, Oop> {
        unsafe {
            let query = self.send(
                self.id.root,
                b"tryRepresentationIdentityOf:behavior:bytes:",
                &[value, self.behavior, abi::size(8).unwrap()],
            )?;
            if !query.is_null() {
                return abi::as_size(query).map(|n| n != 0).ok_or(self.invalid());
            }
            if self.send(self.id.root, b"behaviorOf:", &[value])? != self.behavior {
                return Ok(false);
            }
            Ok(self.send(self.id.root, b"allocationSizeOf:", &[value])? == abi::size(8).unwrap())
        }
    }
    pub(crate) unsafe fn read(&self, value: Oop) -> Result<Option<u64>, Oop> {
        if let Some(size) = abi::as_size(value) {
            return Ok(Some(size as u64));
        }
        if !unsafe { self.matches_representation(value) }? {
            return Ok(None);
        }
        Ok(Some(unsafe { value.cast::<u64>().read() }))
    }
    pub(crate) unsafe fn read_inputs(
        &self,
        inputs: &[Oop],
        values: &mut [u64],
    ) -> Result<bool, Oop> {
        if inputs.len() > 32 || inputs.len() > values.len() {
            return Err(self.invalid());
        }
        unsafe {
            if inputs.len() >= 2 && inputs.iter().all(|&v| abi::as_size(v).is_none()) {
                let mut request = [ptr::null_mut(); 34];
                request[0] = self.behavior;
                request[1] = abi::size(8).unwrap();
                request[2..2 + inputs.len()].copy_from_slice(inputs);
                let reply = self.send(
                    self.id.root,
                    b"representationsMatch",
                    &request[..2 + inputs.len()],
                )?;
                if !reply.is_null() {
                    match abi::as_size(reply) {
                        Some(0) => return Ok(false),
                        Some(1) => {
                            for (&input, value) in inputs.iter().zip(values.iter_mut()) {
                                *value = input.cast::<u64>().read();
                            }
                            return Ok(true);
                        }
                        _ => return Err(self.invalid()),
                    }
                }
            }
            for (&input, value) in inputs.iter().zip(values.iter_mut()) {
                let Some(bits) = self.read(input)? else {
                    return Ok(false);
                };
                *value = bits;
            }
            Ok(true)
        }
    }
    pub(crate) unsafe fn make(&self, bits: u64) -> Result<Oop, Oop> {
        unsafe {
            let _scope = self.scope();
            let value = self.keep(self.send(
                self.model,
                b"allocate:bytes:",
                &[self.behavior, abi::size(8).unwrap()],
            )?);
            if !self.matches_representation(value)? {
                return Err(self.invalid());
            }
            value.cast::<u64>().write(bits);
            Ok(value)
        }
    }
    unsafe fn number_input(&self, value: Oop) -> Result<u64, Oop> {
        if let Some(bits) = unsafe { self.read(value) }? {
            return Ok(bits);
        }
        unsafe {
            let low = abi::as_size(self.send(value, b"low32", &[])?)
                .filter(|n| *n <= u32::MAX as usize)
                .ok_or(self.invalid())?;
            let high = abi::as_size(self.send(value, b"high32", &[])?)
                .filter(|n| *n <= u32::MAX as usize)
                .ok_or(self.invalid())?;
            Ok(low as u64 | ((high as u64) << 32))
        }
    }
    unsafe fn execute(&self, r: Oop, a: &[Oop], op: usize) -> Result<Oop, Oop> {
        let invalid = self.invalid();
        unsafe {
            if op == 0 {
                let [v] = a else {
                    return Err(invalid);
                };
                return self.make(self.number_input(*v)?);
            }
            if op == 19 {
                let [lo, hi] = a else {
                    return Err(invalid);
                };
                let lo = abi::as_size(*lo)
                    .filter(|v| *v <= u32::MAX as usize)
                    .ok_or(invalid)?;
                let hi = abi::as_size(*hi)
                    .filter(|v| *v <= u32::MAX as usize)
                    .ok_or(invalid)?;
                return self.make(lo as u64 | ((hi as u64) << 32));
            }
            if op == 17 {
                let [left, right] = a else {
                    return Err(invalid);
                };
                if left.is_null() || right.is_null() {
                    return Err(invalid);
                }
                return self.make(u64::from(left == right));
            }
            if op == 18 {
                let [zero, nonzero] = a else {
                    return Err(invalid);
                };
                if zero.is_null() || nonzero.is_null() {
                    return Err(invalid);
                }
                let value = self.read(r)?.ok_or(invalid)?;
                return Ok(if value == 0 { *zero } else { *nonzero });
            }
            if op == 22 {
                let [left, right, yes, no] = a else {
                    return Err(invalid);
                };
                if a.iter().any(|v| v.is_null()) {
                    return Err(invalid);
                }
                return Ok(if left == right { *yes } else { *no });
            }
            if (6..=16).contains(&op) && !r.is_null() && a.len() == 1 && !a[0].is_null() {
                let mut values = [0; 2];
                if !self.read_inputs(&[r, a[0]], &mut values)? {
                    return Err(invalid);
                }
                if op == 16 {
                    return Ok(self.boolean(values[0] < values[1]));
                }
                return self
                    .make(crate::operations::calculate(values[0], values[1], op).ok_or(invalid)?);
            }
            if (5..=16).contains(&op) {
                let value = self.read(r)?.ok_or(invalid)?;
                if op == 5 {
                    if !a.is_empty() {
                        return Err(invalid);
                    }
                    return self.make(!value);
                }
                let [right] = a else {
                    return Err(invalid);
                };
                let right = self.read(*right)?.ok_or(invalid)?;
                if op == 16 {
                    return Ok(self.boolean(value < right));
                }
                return self.make(crate::operations::calculate(value, right, op).ok_or(invalid)?);
            }
            if a.len() != usize::from(op == 2) {
                return Err(invalid);
            }
            let Some(value) = self.read(r)? else {
                return Ok(self.nil);
            };
            match op {
                // A Word is already an object-valued bit pattern. Small boundary
                // numbers use the size immediate; all 64 bits remain available.
                1 => Ok(abi::size(value as usize).unwrap_or(r)),
                2 => Ok(self.boolean(self.read(a[0])? == Some(value))),
                20 => Ok(abi::size((value & u32::MAX as u64) as usize).unwrap()),
                21 => Ok(abi::size((value >> 32) as usize).unwrap()),
                _ => Err(invalid),
            }
        }
    }
}
unsafe extern "C" fn roots(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([visitor]) = abi::borrowed_slice(a, n) else {
            return Err(s.invalid());
        };
        let mut values = vec![
            s.roots,
            s.gc,
            s.model,
            s.namespace,
            s.behavior,
            s.prototype,
            s.truth,
            s.falsity,
            s.nil,
        ];
        values.extend(s.names.values().copied());
        values.extend(s.scoped.borrow().iter().copied());
        values.extend(s.witnesses.iter().map(Cell::get));
        for value in values {
            s.send(*visitor, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn references(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([value, visitor]) = abi::borrowed_slice(a, n) else {
            return Err(s.invalid());
        };
        let precise = abi::as_size(s.send(s.gc, b"precisePayloads", &[])?).ok_or(s.invalid())? != 0;
        if !precise {
            let bytes = abi::as_size(s.send(s.id.root, b"allocationSizeOf:", &[*value])?)
                .ok_or(s.invalid())?;
            for i in 0..bytes.div_ceil(size_of::<usize>()) {
                let bits = value.cast::<usize>().add(i).read();
                if bits & 1 != 0 {
                    continue;
                }
                let Some(address) = abi::size(bits) else {
                    continue;
                };
                let edge = s.send(s.id.root, b"allocationAt:", &[address])?;
                if !edge.is_null() {
                    s.send(*visitor, b"visit:", &[edge])?;
                }
            }
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! operations{($(($name:literal,$entry:ident,$op:expr)),*$(,)?)=>{
    $(pub(crate) unsafe extern "C" fn $entry(_id:Oop,c:Oop,r:Oop,a:*const Oop,n:usize)->Oop{
        (||unsafe{let s=state(c)?;let a=abi::borrowed_slice(a,n).ok_or(s.invalid())?;s.execute(r,a,$op)})().unwrap_or_else(abi::fault_reference)
    })*
    static METHODS:&[MethodSpec]=&[$(MethodSpec{name:index($name),entry:Some($entry)}),*,
        MethodSpec{name:index(b"rootsDo:"),entry:Some(roots)},MethodSpec{name:index(b"referencesOf:do:"),entry:Some(references)},
        MethodSpec{name:index(b"tryPrimitiveExpression:"),entry:Some(crate::expression::evaluate)},
        MethodSpec{name:index(b"tryPrimitiveExpressionAtEpoch:"),entry:Some(crate::expression::evaluate_at_epoch)}];
};}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"behaviorEpoch",
    b"usesDefaultAllocation",
    b"canOmitAllocationFor:",
    b"nativeBindingsMatch:",
    b"retain:",
    b"release:",
    b"behaviorOf:",
    b"allocationSizeOf:",
    b"tryRepresentationIdentityOf:behavior:bytes:",
    b"representationsMatch",
    b"allocate:bytes:",
    b"at:",
    b"at:put:",
    b"rootsDo:",
    b"referencesOf:do:",
    b"addMethod",
    b"register:",
    b"trace:with:",
    b"ownerOf:put:",
    b"Word",
    b"WordVt",
    b"Roots",
    b"GC",
    b"ObjectModel",
    b"true",
    b"false",
    b"nil",
    b"ObjectVt",
    b"sizeBehavior",
    b"parentOf:put:",
    b"delegated",
    b"setLayoutOf:to:rest:",
    b"sequence",
    b"methodAt:",
    b"withContext:",
    b"visit:",
    b"precisePayloads",
    b"allocationAt:",
    b"size",
    b"selectIfSameObject:with:ifEqual:ifDifferent:",
    b"tryPrimitiveExpression:",
    b"tryPrimitiveExpressionAtEpoch:",
    b"sameObject:with:",
    b"selectIfZero:ifNonzero:",
    b"bitNot",
    b"wrappingAdd:",
    b"wrappingSubtract:",
    b"wrappingMultiply:",
    b"unsignedDivide:",
    b"unsignedRemainder:",
    b"bitAnd:",
    b"bitOr:",
    b"bitXor:",
    b"shiftLeft:",
    b"shiftRight:",
    b"unsignedLessThan:",
    b"fromBits:",
    b"bits",
    b"=",
    b"fromUnsignedLow:high:",
    b"low32",
    b"high32",
    b"sizeWord",
    b"atWord:"
];
const fn index(name: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        if NAMES[i].len() == name.len() {
            let mut j = 0;
            while j < name.len() && NAMES[i][j] == name[j] {
                j += 1;
            }
            if j == name.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("missing literal")
}
operations![
    (b"fromBits:", from_bits, 0),
    (b"bits", bits, 1),
    (b"=", equal, 2),
    (b"bitNot", bit_not, 5),
    (b"wrappingAdd:", wrapping_add, 6),
    (b"wrappingSubtract:", wrapping_sub, 7),
    (b"wrappingMultiply:", wrapping_mul, 8),
    (b"unsignedDivide:", unsigned_div, 9),
    (b"unsignedRemainder:", unsigned_rem, 10),
    (b"bitAnd:", bit_and, 11),
    (b"bitOr:", bit_or, 12),
    (b"bitXor:", bit_xor, 13),
    (b"shiftLeft:", shift_left, 14),
    (b"shiftRight:", shift_right, 15),
    (b"unsignedLessThan:", unsigned_less, 16),
    (b"sameObject:with:", same_object, 17),
    (b"selectIfZero:ifNonzero:", select_object, 18),
    (b"fromUnsignedLow:high:", from_halves, 19),
    (b"low32", low32, 20),
    (b"high32", high32, 21),
    (
        b"selectIfSameObject:with:ifEqual:ifDifferent:",
        select_same,
        22
    )
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    argc: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    let id = unsafe { *p };
    let Some([namespace]) = (unsafe { abi::borrowed_slice(a, argc) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { abi::borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if symbols.len() != NAMES.len() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.ready.get() && s.namespace == *namespace {
            s.prototype
        } else {
            id.invalid
        };
    }
    (|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |name: &[u8]| send(*namespace, b"at:", &[names[name]]);
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let model = named(b"ObjectModel")?;
        let truth = named(b"true")?;
        let falsity = named(b"false")?;
        let nil = named(b"nil")?;
        let parent = named(b"ObjectVt")?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, b"retain:", &[v])?);
                Ok(v)
            };
            let behavior = keep(send(parent, b"delegated", &[])?)?;
            let empty = keep(send(id.root, b"sequence", &[])?)?;
            send(
                model,
                b"setLayoutOf:to:rest:",
                &[behavior, empty, abi::size(0).unwrap()],
            )?;
            let prototype = keep(send(
                model,
                b"allocate:bytes:",
                &[behavior, abi::size(8).unwrap()],
            )?)?;
            let s = Box::into_raw(Box::new(Number {
                id,
                namespace: *namespace,
                roots,
                gc,
                model,
                behavior,
                prototype,
                truth,
                falsity,
                nil,
                names: names.clone(),
                scoped: RefCell::new(Vec::new()),
                witnesses: std::array::from_fn(|_| Cell::new(ptr::null_mut())),
                expression_meanings: Cell::new((usize::MAX, 0)),
                ready: Cell::new(false),
            }));
            INSTANCE.store(s, Ordering::Release);
            let s = &*s;
            for spec in METHODS {
                let name = NAMES[spec.name];
                let method = s.send(image, b"methodAt:", &[symbols[spec.name]])?;
                let method = s.send(method, b"withContext:", &[prototype])?;
                s.send(behavior, b"addMethod", &[symbols[spec.name], method])?;
                if let Some(code) = crate::expression::code_for(name) {
                    s.witnesses[code as usize].set(method);
                }
            }
            s.send(roots, b"register:", &[prototype])?;
            s.send(gc, b"trace:with:", &[behavior, prototype])?;
            s.send(id.root, b"ownerOf:put:", &[behavior, prototype])?;
            // Boundary sizes have the same unsigned operations, without changing
            // the identity or eight-byte storage of constructed Word objects.
            let size_behavior = s.send(id.root, b"sizeBehavior", &[])?;
            s.send(id.root, b"parentOf:put:", &[size_behavior, behavior])?;
            for (name, value) in [(b"Word".as_slice(), prototype), (b"WordVt", behavior)] {
                s.send(*namespace, b"at:put:", &[s.names[name], value])?;
            }
            s.ready.set(true);
            Ok(prototype)
        })();
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        result
    })()
    .unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v2: Module = Module {
    version: 2,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
