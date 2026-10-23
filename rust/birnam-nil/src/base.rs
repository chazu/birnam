//! Nil predicates use the canonical identities published by Boolean.
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
    text: Oop,
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
    if c == s.nil { Ok(s) } else { Err(s.id.invalid) }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
}
unsafe extern "C" fn yes(_id: Oop, c: Oop, _r: Oop, _a: *const Oop, n: usize) -> Oop {
    match unsafe { state(c) } {
        Ok(s) => {
            if n == 0 {
                s.truth
            } else {
                s.id.invalid
            }
        }
        Err(e) => e,
    }
}
unsafe extern "C" fn no(_id: Oop, c: Oop, _r: Oop, _a: *const Oop, n: usize) -> Oop {
    match unsafe { state(c) } {
        Ok(s) => {
            if n == 0 {
                s.falsity
            } else {
                s.id.invalid
            }
        }
        Err(e) => e,
    }
}
unsafe extern "C" fn roots(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([visitor]) = abi::borrowed_slice(a, n) else {
            return Err(s.id.invalid);
        };
        for v in [s.namespace, s.roots, s.image, s.nil, s.truth, s.falsity]
            .into_iter()
            .chain(s.names.values().copied())
        {
            s.send(*visitor, b"visit:", &[v])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(|e| e)
}
unsafe extern "C" fn print(_id: Oop, c: Oop, _r: Oop, _a: *const Oop, n: usize) -> Oop {
    let raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        std::process::abort();
    }
    let s = unsafe { &*raw };
    if n != 0 {
        return s.id.invalid;
    }
    (|| unsafe { s.send(c, b"fromBytes:", &[s.text]) })().unwrap_or_else(|e| e)
}
unsafe extern "C" fn install(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([string]) = abi::borrowed_slice(a, n) else {
            return Err(s.id.invalid);
        };
        if r != s.nil || string.is_null() || s.printing.get() {
            return Err(s.id.invalid);
        }
        let behavior = s.send(s.id.root, b"behaviorOf:", &[r])?;
        let selector = s.names[b"printString".as_slice()];
        if s.send(behavior, b"lookup", &[selector])?.is_null() {
            let method = s.send(s.image, b"methodAt:", &[s.names[b"printNil".as_slice()]])?;
            let method = s.send(method, b"withContext:", &[*string])?;
            s.send(behavior, b"addMethod", &[selector, method])?;
        }
        s.printing.set(true);
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(|e| e)
}
macro_rules! literals{($($n:literal),* $(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"at:",
    b"Roots",
    b"nil",
    b"true",
    b"false",
    b"objectBehavior",
    b"behaviorOf:",
    b"methodAt:",
    b"withContext:",
    b"addMethod",
    b"register:",
    b"rootsDo:",
    b"visit:",
    b"isNil",
    b"notNil",
    b"yes",
    b"no",
    b"installPrinting:",
    b"printNil",
    b"lookup",
    b"printString",
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
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: index(b"yes"),
        entry: Some(yes),
    },
    MethodSpec {
        name: index(b"no"),
        entry: Some(no),
    },
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots),
    },
    MethodSpec {
        name: index(b"printNil"),
        entry: Some(print),
    },
    MethodSpec {
        name: index(b"installPrinting:"),
        entry: Some(install),
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
            ptr::null_mut()
        } else {
            id.invalid
        };
    }
    (|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let get = |name: &[u8]| send(*namespace, b"at:", &[names[name]]);
        let roots = get(b"Roots")?;
        let nil = get(b"nil")?;
        let truth = get(b"true")?;
        let falsity = get(b"false")?;
        if [roots, nil, truth, falsity].iter().any(|o| o.is_null()) {
            return Err(id.invalid);
        }
        let parent = send(id.root, b"objectBehavior", &[])?;
        let behavior = send(id.root, b"behaviorOf:", &[nil])?;
        let text = send(image, b"literalAt:", &[abi::size(index(b"nil")).unwrap()])?;
        let s = Box::into_raw(Box::new(State {
            id,
            namespace: *namespace,
            roots,
            image,
            nil,
            truth,
            falsity,
            names,
            text,
            printing: Cell::new(false),
            ready: Cell::new(false),
        }));
        INSTANCE.store(s, Ordering::Release);
        let s = &*s;
        for (target, name, implementation) in [
            (parent, b"isNil".as_slice(), b"no".as_slice()),
            (parent, b"notNil", b"yes"),
            (behavior, b"isNil", b"yes"),
            (behavior, b"notNil", b"no"),
            (behavior, b"rootsDo:", b"rootsDo:"),
            (behavior, b"installPrinting:", b"installPrinting:"),
        ] {
            let method = s.send(image, b"methodAt:", &[s.names[implementation]])?;
            let method = s.send(method, b"withContext:", &[nil])?;
            s.send(target, b"addMethod", &[s.names[name], method])?;
        }
        s.send(roots, b"register:", &[nil])?;
        s.ready.set(true);
        Ok(ptr::null_mut())
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
