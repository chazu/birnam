//! Float owns its numeric payload; dependencies communicate through Oop messages.
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
    integer: Oop,
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
unsafe fn state<'a>(c: Oop) -> Result<&'a Number, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let n = unsafe { &*p };
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
    unsafe fn read(&self, value: Oop) -> Result<Option<f64>, Oop> {
        unsafe {
            if self.send(self.id.root, b"behaviorOf:", &[value])? != self.behavior
                || self.send(self.id.root, b"allocationSizeOf:", &[value])? != abi::size(8).unwrap()
            {
                return Ok(None);
            }
            Ok(Some(f64::from_bits(value.cast::<u64>().read())))
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
                if self.send(self.id.root, b"behaviorOf:", &[value])? != self.behavior
                    || self.send(self.id.root, b"allocationSizeOf:", &[value])?
                        != abi::size(8).unwrap()
                {
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
        unsafe { self.rooted(&[], || {
            let source = abi::byte_transfer::buffer(bytes, self.binding(b"Bytes")?,
                |r,n,a| self.send(r,n,a), |v| {self.scoped.borrow_mut().push(v); Ok(v)})?;
            self.send(self.string, b"fromBytes:", &[source])
        }) }
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
    unsafe fn floating(&self, value: f64) -> Result<Oop, Oop> {
        unsafe { self.send(self.prototype, b"fromBits:", &[self.word(value.to_bits())?]) }
    }
    unsafe fn integer(&self, value: f64) -> Result<Oop, Oop> {
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
                &[self.word(value as i64 as u64)?],
            )
        }
    }
    unsafe fn number(&self, value: Oop) -> Result<Option<f64>, Oop> {
        unsafe {
            let word = self.send(self.integer, b"wordOf:", &[value])?;
            if !word.is_null() {
                return Ok(Some(self.word_bits(word)? as i64 as f64));
            }
            self.read(value)
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
            n.integer,
        ];
        values.extend(n.names.values().copied());
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
unsafe fn execute(n: &Number, r: Oop, args: &[Oop], op: usize) -> Result<Oop, Oop> {
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
                .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
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
                Some(value) => n.word(value.to_bits()),
                None => Ok(ptr::null_mut()),
            };
        }
        let Some(value) = n.read(r)? else {
            return if op == 12 { Err(invalid()) } else { Ok(n.nil) };
        };
        if op <= 9 {
            let [right] = args else {
                return Err(invalid());
            };
            let right = object(*right)?;
            if op != 5 && n.is_error(right)? {
                return Ok(right);
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
            12 => n.word(value.to_bits()),
            13 => {
                let mut text = value.to_string();
                if !text.contains(['.', 'e', 'E']) {
                    text.push_str(".0");
                }
                n.text(text.as_bytes())
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
fn mix(mut value: u64) -> usize {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u32 as usize
}
entry!(collection_hash, 30);

entry!(collection_equal, 31);
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
    b"Float",
    b"FloatVt",
    b"GC",
    b"Integer",
    b"ObjectModel",
    b"ObjectVt",
    b"Roots",
    b"String",
    b"Word",
    b"abs",
    b"addMethod",
    b"allocate:bytes:",
    b"allocationAt:",
    b"allocationSizeOf:",
    b"asInteger",
    b"at:",
    b"at:put:",
    b"behaviorOf:",
    b"bits",
    b"bitsOf:",
    b"ceiling",
    b"collectionEquals:",
    b"collectionHash",
    b"delegated",
    b"domain",
    b"false",
    b"floor",
    b"fromBits:",
    b"fromBytes:",
    b"fromUnsignedLow:high:",
    b"fromWord:",
    b"high32",
    b"kind:message:",
    b"low32",
    b"methodAt:",
    b"negated",
    b"nil",
    b"overflow",
    b"ownerOf:put:",
    b"precisePayloads",
    b"printString",
    b"referencesOf:do:",
    b"register:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"rounded",
    b"sequence",
    b"setLayoutOf:to:rest:",
    b"sqrt",
    b"trace:with:",
    b"true",
    b"typeMismatch",
    b"visit:",
    b"withContext:",
    b"wordOf:",
    b"zeroDivide",
    b"Bytes",
    b"fromByteValues",
    b"appendByteValues",
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
        name: index(b"fromBits:"),
        entry: Some(from_bits),
    },
    MethodSpec {
        name: index(b"bitsOf:"),
        entry: Some(bits_of),
    },
    MethodSpec {
        name: index(b"bits"),
        entry: Some(bits),
    },
    MethodSpec {
        name: index(b"printString"),
        entry: Some(print),
    },
    MethodSpec {
        name: index(b"negated"),
        entry: Some(negated),
    },
    MethodSpec {
        name: index(b"asInteger"),
        entry: Some(as_integer),
    },
    MethodSpec {
        name: index(b"abs"),
        entry: Some(abs),
    },
    MethodSpec {
        name: index(b"floor"),
        entry: Some(floor),
    },
    MethodSpec {
        name: index(b"ceiling"),
        entry: Some(ceiling),
    },
    MethodSpec {
        name: index(b"rounded"),
        entry: Some(rounded),
    },
    MethodSpec {
        name: index(b"sqrt"),
        entry: Some(sqrt),
    },
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots),
    },
    MethodSpec {
        name: index(b"referencesOf:do:"),
        entry: Some(references),
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
        let integer = named(b"Integer")?;
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
                integer,
                names: names.clone(),
                scoped: RefCell::new(Vec::new()),
                ready: Cell::new(false),
            }));
            INSTANCE.store(n, Ordering::Release);
            let n = &*n;
            for spec in METHODS {
                let method = n.send(image, b"methodAt:", &[symbols[spec.name]])?;
                let method = n.send(method, b"withContext:", &[prototype])?;
                n.send(behavior, b"addMethod", &[symbols[spec.name], method])?;
            }
            n.send(roots, b"register:", &[prototype])?;
            n.send(gc, b"trace:with:", &[behavior, prototype])?;
            n.send(id.root, b"ownerOf:put:", &[behavior, prototype])?;
            for (name, v) in [(b"Float".as_slice(), prototype), (b"FloatVt", behavior)] {
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
