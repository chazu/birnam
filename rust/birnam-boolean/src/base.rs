//! Boolean owns canonical identities; conditionals send only to the chosen block.
use birnam_abi::{
    self as abi, Oop,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{cell::Cell, ptr};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    image: Oop,
    nil: Oop,
    truth: Oop,
    falsity: Oop,
    names: HashMap<&'static [u8], Oop>,
    true_text: Oop,
    false_text: Oop,
    printing: Cell<bool>,
    ready: Cell<bool>,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        let api = API.load(Ordering::Acquire);
        if api.is_null() {
            std::process::abort();
        }
        return Err(unsafe { (*api).invalid });
    }
    let s = unsafe { &*raw };
    if c == s.truth {
        Ok(s)
    } else {
        Err(s.id.invalid)
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    unsafe fn perform(&self, op: Op, r: Oop, a: &[Oop]) -> Result<Oop, Oop> {
        let invalid = self.id.invalid;
        let one = || if let [v] = a { Ok(*v) } else { Err(invalid) };
        unsafe {
            match op {
                Op::Identical => {
                    let v = one()?;
                    if v.is_null() {
                        return Err(invalid);
                    }
                    Ok(if r == v { self.truth } else { self.falsity })
                }
                Op::Not => {
                    if !a.is_empty() {
                        return Err(invalid);
                    }
                    Ok(if r == self.truth {
                        self.falsity
                    } else if r == self.falsity {
                        self.truth
                    } else {
                        self.nil
                    })
                }
                Op::And | Op::Or => {
                    let v = one()?;
                    if v.is_null() {
                        return Err(invalid);
                    }
                    Ok(if r == self.truth {
                        if matches!(op, Op::And) { v } else { r }
                    } else if r == self.falsity {
                        if matches!(op, Op::And) { r } else { v }
                    } else {
                        self.nil
                    })
                }
                Op::IfBoth | Op::IfTrue | Op::IfFalse | Op::AndBlock | Op::OrBlock => {
                    let both = matches!(op, Op::IfBoth);
                    if a.len() != if both { 2 } else { 1 } {
                        return Err(invalid);
                    }
                    if a.iter().any(|v| v.is_null()) {
                        return Err(invalid);
                    }
                    let truth = if r == self.truth {
                        true
                    } else if r == self.falsity {
                        false
                    } else {
                        return Ok(self.nil);
                    };
                    let choose_true = matches!(op, Op::IfBoth | Op::IfTrue | Op::AndBlock);
                    if !both && truth != choose_true {
                        return Ok(if matches!(op, Op::AndBlock | Op::OrBlock) {
                            r
                        } else {
                            self.nil
                        });
                    }
                    self.send(a[usize::from(both && !truth)], b"value", &[])
                }
                Op::Roots => {
                    let visitor = one()?;
                    for v in [
                        self.namespace,
                        self.roots,
                        self.image,
                        self.nil,
                        self.truth,
                        self.falsity,
                    ]
                    .into_iter()
                    .chain(self.names.values().copied())
                    {
                        self.send(visitor, b"visit:", &[v])?;
                    }
                    Ok(ptr::null_mut())
                }
                Op::InstallPrinting => {
                    let string = one()?;
                    if r != self.truth || string.is_null() || self.printing.get() {
                        return Err(invalid);
                    }
                    let behavior = self.send(self.id.root, b"behaviorOf:", &[r])?;
                    let selector = self.names[b"printString".as_slice()];
                    if self.send(behavior, b"lookup", &[selector])?.is_null() {
                        let method = self.send(
                            self.image,
                            b"methodAt:",
                            &[self.names[b"printBoolean".as_slice()]],
                        )?;
                        let method = self.send(method, b"withContext:", &[string])?;
                        self.send(behavior, b"addMethod", &[selector, method])?;
                    }
                    self.printing.set(true);
                    Ok(ptr::null_mut())
                }
            }
        }
    }
}
macro_rules! operations {($(($name:literal,$op:ident)),* $(,)?)=>{
    #[derive(Clone,Copy)]enum Op{$($op),*}
    #[allow(non_snake_case)]mod entries{use super::*;$(pub unsafe extern "C" fn $op(_id:Oop,c:Oop,r:Oop,a:*const Oop,n:usize)->Oop{
        let s=match unsafe{state(c)}{Ok(s)=>s,Err(e)=>return e};
        let Some(a)=(unsafe{abi::borrowed_slice(a,n)})else{return s.id.invalid;};
        unsafe{s.perform(Op::$op,r,a)}.unwrap_or_else(|e|e)
    })*}
    static METHODS:&[MethodSpec]=&[$(MethodSpec{name:index($name),entry:Some(entries::$op)}),*,
        MethodSpec{name:index(b"printBoolean"),entry:Some(print)}];
};}
macro_rules! literals{($($n:literal),* $(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"at:",
    b"at:put:",
    b"Roots",
    b"objectBehavior",
    b"delegated",
    b"allocate",
    b"methodAt:",
    b"withContext:",
    b"addMethod",
    b"retain:",
    b"release:",
    b"register:",
    b"rootsDo:",
    b"visit:",
    b"==",
    b"not",
    b"&",
    b"|",
    b"ifTrue:ifFalse:",
    b"ifTrue:",
    b"ifFalse:",
    b"and:",
    b"or:",
    b"installPrinting:",
    b"Boolean",
    b"False",
    b"true",
    b"false",
    b"nil",
    b"value",
    b"behaviorOf:",
    b"lookup",
    b"printString",
    b"printBoolean",
    b"literalAt:",
    b"sequence",
    b"fromBytes:"
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
    (b"==", Identical),
    (b"not", Not),
    (b"&", And),
    (b"|", Or),
    (b"ifTrue:ifFalse:", IfBoth),
    (b"ifTrue:", IfTrue),
    (b"ifFalse:", IfFalse),
    (b"and:", AndBlock),
    (b"or:", OrBlock),
    (b"installPrinting:", InstallPrinting),
    (b"rootsDo:", Roots)
];
unsafe extern "C" fn print(_id: Oop, c: Oop, r: Oop, _a: *const Oop, n: usize) -> Oop {
    let raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        std::process::abort();
    }
    let s = unsafe { &*raw };
    if n != 0 {
        return s.id.invalid;
    }
    (|| unsafe {
        // The only per-Method state is its String dependency: use that Oop
        // directly as context, with no wrapper or per-field message reads.
        let text = if r == s.truth {
            s.true_text
        } else if r == s.falsity {
            s.false_text
        } else {
            return Ok(s.nil);
        };
        s.send(c, b"fromBytes:", &[text])
    })()
    .unwrap_or_else(|e| e)
}
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
            s.truth
        } else {
            id.invalid
        };
    }
    (|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let roots = send(*namespace, b"at:", &[names[b"Roots".as_slice()]])?;
        let parent = send(id.root, b"objectBehavior", &[])?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, b"retain:", &[v])?);
                Ok(v)
            };
            let nil_behavior = keep(send(parent, b"delegated", &[])?)?;
            let behavior = keep(send(parent, b"delegated", &[])?)?;
            let nil = keep(send(nil_behavior, b"allocate", &[abi::size(0).unwrap()])?)?;
            let truth = keep(send(behavior, b"allocate", &[abi::size(0).unwrap()])?)?;
            let falsity = keep(send(behavior, b"allocate", &[abi::size(0).unwrap()])?)?;
            let true_text = send(image, b"literalAt:", &[abi::size(index(b"true")).unwrap()])?;
            let false_text = send(image, b"literalAt:", &[abi::size(index(b"false")).unwrap()])?;
            let s = Box::into_raw(Box::new(State {
                id,
                namespace: *namespace,
                roots,
                image,
                nil,
                truth,
                falsity,
                names: names.clone(),
                true_text,
                false_text,
                printing: Cell::new(false),
                ready: Cell::new(false),
            }));
            INSTANCE.store(s, Ordering::Release);
            let s = &*s;
            for spec in METHODS {
                if spec.name == index(b"printBoolean") {
                    continue;
                }
                let method = s.send(image, b"methodAt:", &[symbols[spec.name]])?;
                let method = s.send(method, b"withContext:", &[truth])?;
                let target = if spec.name == index(b"==") {
                    parent
                } else {
                    behavior
                };
                s.send(target, b"addMethod", &[symbols[spec.name], method])?;
            }
            s.send(roots, b"register:", &[truth])?;
            for (name, value) in [
                (b"Boolean".as_slice(), truth),
                (b"False", falsity),
                (b"true", truth),
                (b"false", falsity),
                (b"nil", nil),
            ] {
                s.send(*namespace, b"at:put:", &[s.names[name], value])?;
            }
            s.ready.set(true);
            Ok(truth)
        })();
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        result
    })()
    .unwrap_or_else(|e| e)
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
