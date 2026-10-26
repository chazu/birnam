//! Integer owns signed payloads and recognizes its Methods through Oop witnesses.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
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
    image: Oop,
    extended: Cell<bool>,
    witnesses: [Cell<Oop>; 12],
    mixed_witnesses: [Cell<Oop>; 8],
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
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
fn invalid() -> Oop {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        std::process::abort();
    }
    unsafe { abi::object_reference((*p).invalid) }
}
fn object(v: Oop) -> Result<Oop, Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
fn done(v: Result<Oop, Oop>) -> Oop {
    v.unwrap_or_else(abi::fault_reference)
}
unsafe fn instance<'a>() -> Result<&'a Number, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    Ok(unsafe { &*p })
}
unsafe fn state<'a>(c: Oop) -> Result<&'a Number, Oop> {
    let n = unsafe { instance()? };
    if c == n.prototype {
        Ok(n)
    } else {
        Err(invalid())
    }
}
impl Number {
    unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            self.id
                .send(r, *self.names.get(name).ok_or_else(invalid)?, a)
        }
    }
    unsafe fn rooted<T>(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<T, Oop>,
    ) -> Result<T, Oop> {
        let start = self.scoped.borrow().len();
        self.scoped.borrow_mut().extend_from_slice(values);
        let _scope = Scope {
            values: &self.scoped,
            start,
        };
        f()
    }
    unsafe fn matches_representation(&self, value: Oop) -> Result<bool, Oop> {
        unsafe {
            match self.send(
                self.id.root,
                b"tryRepresentationIdentityOf:behavior:bytes:",
                &[value, self.behavior, abi::size(8).unwrap()],
            ) {
                Ok(v) if v.is_null() => {}
                Ok(v) => return abi::as_size(v).map(|n| n != 0).ok_or_else(invalid),
                Err(e) => {
                    if self.send(e, b"faultKind", &[]).ok()
                        != Some(self.names[b"unbound".as_slice()])
                    {
                        return Err(e);
                    }
                }
            }
            if self.send(self.id.root, b"behaviorOf:", &[value])? != self.behavior {
                return Ok(false);
            }
            Ok(self.send(self.id.root, b"allocationSizeOf:", &[value])? == abi::size(8).unwrap())
        }
    }
    unsafe fn read(&self, value: Oop) -> Result<Option<i64>, Oop> {
        unsafe {
            if !self.matches_representation(value)? {
                return Ok(None);
            }
            Ok(Some(value.cast::<i64>().read()))
        }
    }
    unsafe fn make(&self, bits: u64) -> Result<Oop, Oop> {
        unsafe {
            let value = self.send(
                self.model,
                b"allocate:bytes:",
                &[self.behavior, abi::size(8).unwrap()],
            )?;
            self.rooted(&[value], || {
                if !self.matches_representation(value)? {
                    return Err(invalid());
                }
                value.cast::<u64>().write(bits);
                Ok(value)
            })
        }
    }
    unsafe fn binding(&self, name: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            self.send(
                self.namespace,
                b"at:",
                &[*self.names.get(name).ok_or_else(invalid)?],
            )
        }
    }
    unsafe fn is_error(&self, value: Oop) -> Result<bool, Oop> {
        unsafe {
            // Keep the current ErrorVt binding, including rebinding to numeric vtables.
            let behavior = self.binding(b"ErrorVt")?;
            self.rooted(&[behavior], || {
                let actual = self.send(self.id.root, b"behaviorOf:", &[value])?;
                Ok(!behavior.is_null() && behavior == actual)
            })
        }
    }
    unsafe fn text(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let source = self.send(self.id.root, b"sequence", &values)?;
            self.send(self.string, b"fromBytes:", &[source])
        }
    }
    unsafe fn error(&self, kind: &[u8], message: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let message = self.text(message)?;
            self.rooted(&[message], || {
                let factory = self.binding(b"Error")?;
                if factory.is_null() {
                    return Ok(self.nil);
                }
                self.send(
                    factory,
                    b"kind:message:",
                    &[*self.names.get(kind).ok_or_else(invalid)?, message],
                )
            })
        }
    }
    fn boolean(&self, v: bool) -> Oop {
        if v { self.truth } else { self.falsity }
    }
    unsafe fn word(&self, bits: u64) -> Result<Oop, Oop> {
        unsafe {
            if let Some(v) = abi::size(bits as usize) {
                return Ok(v);
            }
            self.send(
                self.binding(b"Word")?,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(bits as u32 as usize).unwrap(),
                    abi::size((bits >> 32) as usize).unwrap(),
                ],
            )
        }
    }
    unsafe fn word_bits(&self, v: Oop) -> Result<u64, Oop> {
        unsafe {
            if let Some(n) = abi::as_size(v) {
                return Ok(n as u64);
            }
            let lo = abi::as_size(self.send(v, b"low32", &[])?)
                .filter(|v| *v <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            let hi = abi::as_size(self.send(v, b"high32", &[])?)
                .filter(|v| *v <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            Ok((lo as u64) | ((hi as u64) << 32))
        }
    }

    unsafe fn method(&self, name: &[u8], context: Oop) -> Result<Oop, Oop> {
        unsafe {
            let m = self.send(self.image, b"methodAt:", &[self.names[name]])?;
            self.send(m, b"withContext:", &[context])
        }
    }
    unsafe fn same_method(&self, a: Oop, b: Oop) -> Result<bool, Oop> {
        unsafe {
            if a.is_null() || b.is_null() {
                return Ok(false);
            }
            Ok(abi::as_size(self.send(self.id.root, b"sameNativeMethod:as:", &[a, b])?) == Some(1))
        }
    }
    unsafe fn operation(&self, selected: Oop) -> Result<Option<(usize, bool)>, Oop> {
        unsafe {
            // Compare the entered code, context and owner. Equivalent copied native
            // Methods remain recognizable without consulting their message bindings.
            for (mixed, ws) in [
                (false, &self.witnesses[..10]),
                (true, &self.mixed_witnesses[..]),
            ] {
                if let Some(i) = ws.iter().position(|w| w.get() == selected) {
                    return Ok(self
                        .same_method(selected, ws[i].get())?
                        .then_some((if mixed && i >= 4 { i + 2 } else { i }, mixed)));
                }
            }
            let mut candidates = [ptr::null_mut(); 19];
            candidates[0] = selected;
            for (to, from) in candidates[1..11].iter_mut().zip(&self.witnesses[..10]) {
                *to = from.get();
            }
            for (to, from) in candidates[11..].iter_mut().zip(&self.mixed_witnesses) {
                *to = from.get();
            }
            let index = self.send(self.id.root, b"nativeMethodIndex", &candidates)?;
            let Some(index) = abi::as_size(index) else {
                return if index.is_null() {
                    Ok(None)
                } else {
                    Err(invalid())
                };
            };
            if index >= 18 {
                return Err(invalid());
            }
            Ok(Some(if index < 10 {
                (index, false)
            } else {
                let i = index - 10;
                (if i >= 4 { i + 2 } else { i }, true)
            }))
        }
    }
    unsafe fn integer(&self, value: i128) -> Result<Oop, Oop> {
        unsafe {
            match i64::try_from(value) {
                Ok(v) => self.send(self.prototype, b"fromWord:", &[self.word(v as u64)?]),
                Err(_) => self.error(b"overflow", b"integer overflow"),
            }
        }
    }
    unsafe fn calculate(&self, left: i64, right: i64, op: usize) -> Result<Oop, Oop> {
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
    unsafe fn calculate_with(
        &self,
        left: i64,
        right: i64,
        op: usize,
        visitor: Option<Oop>,
    ) -> Result<Oop, Oop> {
        unsafe {
            let Some(visitor) = visitor else {
                return self.calculate(left, right, op);
            };
            let left_bits = self.word(left as u64)?;
            self.rooted(&[left_bits], || {
                let right_bits = self.word(right as u64)?;
                let result = self.send(
                    visitor,
                    b"numericOperation:left:right:",
                    &[abi::size(op + 1).unwrap(), left_bits, right_bits],
                )?;
                if result.is_null() {
                    return self.calculate(left, right, op);
                }
                self.send(self.prototype, b"fromWord:", &[result])
            })
        }
    }
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, count: usize) -> Oop {
    done((|| unsafe {
        let n = state(c)?;
        let [visitor] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
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
            n.image,
        ];
        values.extend(n.names.values().copied());
        values.extend(n.witnesses.iter().map(Cell::get));
        values.extend(n.mixed_witnesses.iter().map(Cell::get));
        values.extend(n.scoped.borrow().iter().copied());
        for v in values {
            n.send(*visitor, b"visit:", &[v])?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn references(_: Oop, c: Oop, _: Oop, a: *const Oop, count: usize) -> Oop {
    done((|| unsafe {
        let n = state(c)?;
        let [value, visitor] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let precise =
            abi::as_size(n.send(n.gc, b"precisePayloads", &[])?).ok_or_else(invalid)? != 0;
        if !precise {
            let bytes = abi::as_size(n.send(n.id.root, b"allocationSizeOf:", &[*value])?)
                .ok_or_else(invalid)?;
            for i in 0..bytes.div_ceil(size_of::<usize>()) {
                let bits = value.cast::<usize>().add(i).read();
                if bits & 1 != 0 {
                    continue;
                }
                let Some(address) = abi::size(bits) else {
                    continue;
                };
                let edge = n.send(n.id.root, b"allocationAt:", &[address])?;
                if !edge.is_null() {
                    n.send(*visitor, b"visit:", &[edge])?;
                }
            }
        }
        Ok(ptr::null_mut())
    })())
}
macro_rules! entry {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
            done((|| unsafe {
                execute(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $op)
            })())
        }
    };
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

unsafe fn execute(n: &Number, r: Oop, args: &[Oop], op: usize) -> Result<Oop, Oop> {
    unsafe {
        if op == 32 || op == 33 {
            if args.len() != if op == 32 { 1 } else { 2 } {
                return Err(invalid());
            }
            let left = n.send(r, b"wordOf:", &args[..1])?;
            if left.is_null() {
                return Ok(n.nil);
            }
            let left = n.word_bits(left)?;
            let value = if op == 32 {
                left
            } else {
                let right = n.send(r, b"wordOf:", &args[1..])?;
                if right.is_null() {
                    return Ok(n.nil);
                }
                let right = n.word_bits(right)?;
                match (left as i64).cmp(&(right as i64)) {
                    std::cmp::Ordering::Less => -1i64 as u64,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                }
            };
            return n.send(r, b"fromWord:", &[n.word(value)?]);
        }
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
            return Ok(abi::size(mix(bits)).unwrap());
        }
        if op == 10 {
            let [v] = args else {
                return Err(invalid());
            };
            return n.make(n.word_bits(*v)?);
        }
        if op == 11 {
            let [v] = args else {
                return Err(invalid());
            };
            return match n.read(object(*v)?)? {
                Some(v) => n.word(v as u64),
                None => Ok(ptr::null_mut()),
            };
        }
        if op <= 9 {
            let [right] = args else {
                return Err(invalid());
            };
            let right = object(*right)?;
            let Some(left) = n.read(r)? else {
                return Ok(n.nil);
            };
            if op != 5 && n.is_error(right)? {
                return Ok(right);
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
                return Ok(n.nil);
            };
            while value <= end {
                let arg = object(n.integer(i128::from(value))?)?;
                let answer = n.send(block, b"value:", &[arg])?;
                let answer = object(answer)?;
                let fault = n.rooted(&[answer], || n.is_error(answer))?;
                if fault {
                    return Ok(answer);
                }
                if value == i64::MAX {
                    break;
                }
                value += 1;
            }
            return Ok(r);
        }
        if !args.is_empty() {
            return Err(invalid());
        }
        let Some(value) = n.read(r)? else {
            return if op == 13 { Ok(n.nil) } else { Err(invalid()) };
        };
        match op {
            12 => n.word(value as u64),
            13 => n.text(value.to_string().as_bytes()),
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

unsafe extern "C" fn has_bootstrap_arithmetic(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    count: usize,
) -> Oop {
    done((|| unsafe {
        let n = state(c)?;
        let [selector] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let selected = n.send(n.id.root, b"bind:to:", &[*selector, r])?;
        n.rooted(&[selected], || {
            let expected = [b"+".as_slice(), b"-", b"*", b"/", b"mod:"]
                .iter()
                .position(|name| n.names[*name] == *selector);
            let actual = n
                .operation(selected)?
                .filter(|(op, _)| *op < 5)
                .map(|(op, _)| op);
            Ok(n.boolean(expected.is_some() && expected == actual))
        })
    })())
}
unsafe extern "C" fn evaluate_numeric(_: Oop, c: Oop, _: Oop, a: *const Oop, count: usize) -> Oop {
    done((|| unsafe {
        let n = state(c)?;
        let [selected, left, right, visitor] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let Some((op, mixed)) = n.operation(*selected)?.filter(|(op, _)| *op < 5) else {
            return Ok(ptr::null_mut());
        };
        if mixed && (n.read(*left)?.is_none() || n.read(*right)?.is_none()) {
            let args = n.send(n.id.root, b"sequence", &[*right])?;
            return n.send(*selected, b"invokeOn:arguments:", &[*left, args]);
        }
        let Some(left) = n.read(*left)? else {
            return Ok(n.nil);
        };
        if n.is_error(*right)? {
            return Ok(*right);
        }
        let Some(right) = n.read(*right)? else {
            return n.error(
                b"typeMismatch",
                if op == 4 {
                    b"mod: needs an Integer"
                } else {
                    b"Integer arithmetic needs a number"
                },
            );
        };
        n.calculate_with(left, right, op, Some(*visitor))
    })())
}
unsafe extern "C" fn can_omit_indexing(_: Oop, c: Oop, r: Oop, _: *const Oop, count: usize) -> Oop {
    done((|| unsafe {
        let n = state(c)?;
        if count != 0 {
            return Err(invalid());
        }
        let no = abi::size(0).unwrap();
        if r != n.prototype {
            return Ok(no);
        }
        let epoch = abi::as_size(n.send(n.id.root, b"behaviorEpoch", &[])?).ok_or_else(invalid)?;
        if epoch == usize::MAX >> 1
            || n.send(n.id.root, b"usesDefaultLookup:", &[r])? != abi::size(1).unwrap()
        {
            return Ok(no);
        }
        for (name, i) in [(b"fromWord:".as_slice(), 10), (b"wordOf:", 11)] {
            let selected = n.send(n.id.root, b"bind:to:", &[n.names[name], r])?;
            if !n.same_method(selected, n.witnesses[i].get())? {
                return Ok(no);
            }
        }
        let permitted =
            n.send(n.model, b"canOmitAllocationFor:", &[n.behavior])? == abi::size(1).unwrap();
        Ok(abi::size(usize::from(
            permitted && n.send(n.id.root, b"behaviorEpoch", &[])? == abi::size(epoch).unwrap(),
        ))
        .unwrap())
    })())
}
entry!(integer_value, 32);
entry!(compare_integer, 33);
entry!(collection_hash, 30);
entry!(collection_equal, 31);
#[path = "mixed.rs"]
mod mixed;
#[path = "predicate.rs"]
mod predicate;
fn mix(mut value: u64) -> usize {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u32 as usize
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"*",
    b"+",
    b"-",
    b"/",
    b"<",
    b"<=",
    b"=",
    b">",
    b">=",
    b"Error",
    b"ErrorVt",
    b"GC",
    b"Integer",
    b"IntegerVt",
    b"ObjectModel",
    b"ObjectVt",
    b"Roots",
    b"String",
    b"Word",
    b"addMethod",
    b"allocate:bytes:",
    b"allocationAt:",
    b"allocationSizeOf:",
    b"invokeOn:arguments:",
    b"asFloat",
    b"at:",
    b"at:put:",
    b"behaviorEpoch",
    b"behaviorOf:",
    b"bind:to:",
    b"bitsOf:",
    b"canOmitAllocationFor:",
    b"canOmitIndexing",
    b"collectionEquals:",
    b"collectionHash",
    b"commitNumericPredicate:",
    b"compareInteger:with:",
    b"delegated",
    b"domain",
    b"evaluate:receiver:argument:with:",
    b"evaluatePredicate:then:left:right:limit:in:epoch:with:",
    b"false",
    b"faultKind",
    b"fromBits:",
    b"fromBytes:",
    b"fromUnsignedLow:high:",
    b"fromWord:",
    b"hasBootstrapArithmetic:",
    b"high32",
    b"installFloat:",
    b"integerValue:",
    b"kind:message:",
    b"lookup",
    b"low32",
    b"methodAt:",
    b"mixed::add",
    b"mixed::as_float",
    b"mixed::div",
    b"mixed::ge",
    b"mixed::gt",
    b"mixed::le",
    b"mixed::lt",
    b"mixed::mul",
    b"mixed::sub",
    b"mod:",
    b"nil",
    b"numericOperation:left:right:",
    b"overflow",
    b"ownerOf:put:",
    b"precisePayloads",
    b"printString",
    b"referencesOf:do:",
    b"register:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"sameNativeMethod:as:",
    b"nativeMethodIndex",
    b"sequence",
    b"setLayoutOf:to:rest:",
    b"to:do:",
    b"trace:with:",
    b"true",
    b"tryRepresentationIdentityOf:behavior:bytes:",
    b"typeMismatch",
    b"unbound",
    b"usesDefaultAllocation",
    b"usesDefaultLookup:",
    b"value:",
    b"visit:",
    b"withContext:",
    b"word",
    b"wordOf:",
    b"zeroDivide",
];
const fn index(n: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        let c = NAMES[i];
        if c.len() == n.len() {
            let mut j = 0;
            while j < n.len() && c[j] == n[j] {
                j += 1;
            }
            if j == n.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("missing literal")
}
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: index(b"hasBootstrapArithmetic:"),
        entry: Some(has_bootstrap_arithmetic),
    },
    MethodSpec {
        name: index(b"integerValue:"),
        entry: Some(integer_value),
    },
    MethodSpec {
        name: index(b"compareInteger:with:"),
        entry: Some(compare_integer),
    },
    MethodSpec {
        name: index(b"collectionEquals:"),
        entry: Some(collection_equal),
    },
    MethodSpec {
        name: index(b"collectionHash"),
        entry: Some(collection_hash),
    },
    MethodSpec {
        name: index(b"+"),
        entry: Some(add),
    },
    MethodSpec {
        name: index(b"-"),
        entry: Some(sub),
    },
    MethodSpec {
        name: index(b"*"),
        entry: Some(mul),
    },
    MethodSpec {
        name: index(b"/"),
        entry: Some(div),
    },
    MethodSpec {
        name: index(b"mod:"),
        entry: Some(modulo),
    },
    MethodSpec {
        name: index(b"="),
        entry: Some(equal),
    },
    MethodSpec {
        name: index(b"<"),
        entry: Some(lt),
    },
    MethodSpec {
        name: index(b"<="),
        entry: Some(le),
    },
    MethodSpec {
        name: index(b">"),
        entry: Some(gt),
    },
    MethodSpec {
        name: index(b">="),
        entry: Some(ge),
    },
    MethodSpec {
        name: index(b"fromWord:"),
        entry: Some(from_word),
    },
    MethodSpec {
        name: index(b"canOmitIndexing"),
        entry: Some(can_omit_indexing),
    },
    MethodSpec {
        name: index(b"wordOf:"),
        entry: Some(word_of),
    },
    MethodSpec {
        name: index(b"word"),
        entry: Some(word),
    },
    MethodSpec {
        name: index(b"printString"),
        entry: Some(print),
    },
    MethodSpec {
        name: index(b"to:do:"),
        entry: Some(to_do),
    },
    MethodSpec {
        name: index(b"installFloat:"),
        entry: Some(mixed::install),
    },
    MethodSpec {
        name: index(b"evaluatePredicate:then:left:right:limit:in:epoch:with:"),
        entry: Some(predicate::evaluate),
    },
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots),
    },
    MethodSpec {
        name: index(b"referencesOf:do:"),
        entry: Some(references),
    },
    MethodSpec {
        name: index(b"evaluate:receiver:argument:with:"),
        entry: Some(evaluate_numeric),
    },
    MethodSpec {
        name: index(b"mixed::add"),
        entry: Some(mixed::add),
    },
    MethodSpec {
        name: index(b"mixed::sub"),
        entry: Some(mixed::sub),
    },
    MethodSpec {
        name: index(b"mixed::mul"),
        entry: Some(mixed::mul),
    },
    MethodSpec {
        name: index(b"mixed::div"),
        entry: Some(mixed::div),
    },
    MethodSpec {
        name: index(b"mixed::lt"),
        entry: Some(mixed::lt),
    },
    MethodSpec {
        name: index(b"mixed::le"),
        entry: Some(mixed::le),
    },
    MethodSpec {
        name: index(b"mixed::gt"),
        entry: Some(mixed::gt),
    },
    MethodSpec {
        name: index(b"mixed::ge"),
        entry: Some(mixed::ge),
    },
    MethodSpec {
        name: index(b"mixed::as_float"),
        entry: Some(mixed::as_float),
    },
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
    let Some([namespace]) = (unsafe { borrowed_slice(a, argc) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if symbols.len() != NAMES.len() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let n = unsafe { &*old };
        return if n.ready.get() && n.namespace == *namespace {
            n.prototype
        } else {
            id.invalid
        };
    }
    done((|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| object(send(*namespace, b"at:", &[names[n]])?);
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let model = named(b"ObjectModel")?;
        let parent = named(b"ObjectVt")?;
        let string = named(b"String")?;

        let truth = named(b"true")?;
        let falsity = named(b"false")?;
        let nil = named(b"nil")?;
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
            let n = Box::into_raw(Box::new(Number {
                id,
                roots,
                gc,
                model,
                namespace: *namespace,
                string,
                behavior,
                prototype,
                truth,
                falsity,
                nil,
                image,
                extended: Cell::new(false),
                witnesses: std::array::from_fn(|_| Cell::new(ptr::null_mut())),
                mixed_witnesses: std::array::from_fn(|_| Cell::new(ptr::null_mut())),
                names: names.clone(),
                scoped: RefCell::new(Vec::new()),
                ready: Cell::new(false),
            }));
            INSTANCE.store(n, Ordering::Release);
            let n = &*n;
            for spec in METHODS {
                let name = NAMES[spec.name];
                if name.starts_with(b"mixed::") {
                    continue;
                }
                let method = n.method(name, prototype)?;
                n.send(behavior, b"addMethod", &[symbols[spec.name], method])?;
                if let Some(i) = [
                    b"+".as_slice(),
                    b"-",
                    b"*",
                    b"/",
                    b"mod:",
                    b"=",
                    b"<",
                    b"<=",
                    b">",
                    b">=",
                    b"fromWord:",
                    b"wordOf:",
                ]
                .iter()
                .position(|&v| v == name)
                {
                    n.witnesses[i].set(method);
                }
            }
            n.send(roots, b"register:", &[prototype])?;
            n.send(gc, b"trace:with:", &[behavior, prototype])?;
            n.send(id.root, b"ownerOf:put:", &[behavior, prototype])?;
            for (name, v) in [(b"Integer".as_slice(), prototype), (b"IntegerVt", behavior)] {
                n.send(*namespace, b"at:put:", &[names[name], v])?;
            }
            n.ready.set(true);
            Ok(prototype)
        })();
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        result
    })())
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
