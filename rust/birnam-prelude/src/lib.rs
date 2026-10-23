//! Construct System through ordinary Id, ObjectModel, Roots and Namespace messages.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, Module, Protocol},
};
use core::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
struct State {
    id: Oop,
    namespace: Oop,
    system: Oop,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
struct Constructing;
impl Drop for Constructing {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"at:",
    b"System",
    b"SystemVt",
    b"Roots",
    b"ObjectVt",
    b"ObjectModel",
    b"delegated",
    b"allocate:bytes:",
    b"ownerOf:put:",
    b"at:put:",
    b"retain:",
    b"release:"
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    _: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    let id = unsafe { *p };
    let invalid = abi::object_reference(id.invalid);
    let Some([namespace]) = (unsafe { borrowed_slice(a, count) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if n != NAMES.len() || namespace.is_null() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.id == id.root && s.namespace == *namespace {
            s.system
        } else {
            id.invalid
        };
    }
    if BUSY.swap(true, Ordering::AcqRel) {
        return id.invalid;
    }
    let _constructing = Constructing;
    (|| unsafe {
        let send = |r, name: &[u8], a: &[Oop]| -> Result<Oop, Oop> {
            let index = NAMES.iter().position(|&n| n == name).ok_or(invalid)?;
            id.send(r, symbols[index], a)
        };
        let symbol = |name: &[u8]| symbols[NAMES.iter().position(|&n| n == name).unwrap()];
        let named = |name: &[u8]| send(*namespace, b"at:", &[symbol(name)]);
        let roots = named(b"Roots")?;
        if roots.is_null() {
            return Err(invalid);
        }
        let existing = named(b"System")?;
        let mut temporary = Vec::new();
        let mut keep = |v| -> Result<Oop, Oop> {
            temporary.push(send(roots, b"retain:", &[v])?);
            Ok(v)
        };
        let result = (|| {
            let system = if !existing.is_null() {
                keep(existing)?
            } else {
                let parent = named(b"ObjectVt")?;
                if parent.is_null() {
                    return Err(invalid);
                }
                let behavior = keep(send(parent, b"delegated", &[])?)?;
                let system = keep(send(
                    named(b"ObjectModel")?,
                    b"allocate:bytes:",
                    &[behavior, abi::size(0).unwrap()],
                )?)?;
                send(id.root, b"ownerOf:put:", &[behavior, system])?;
                for (name, value) in [(b"SystemVt".as_slice(), behavior), (b"System", system)] {
                    send(*namespace, b"at:put:", &[symbol(name), value])?;
                }
                system
            };
            // Repeated initialization returns this result even after namespace
            // rebinding. Keep precisely the objects needed by that contract.
            let namespace_token = send(roots, b"retain:", &[*namespace])?;
            if let Err(error) = send(roots, b"retain:", &[system]) {
                send(roots, b"release:", &[namespace_token])?;
                return Err(error);
            }
            INSTANCE.store(
                Box::into_raw(Box::new(State {
                    id: id.root,
                    namespace: *namespace,
                    system,
                })),
                Ordering::Release,
            );
            Ok(system)
        })();
        for token in temporary {
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
    methods: ptr::null(),
    method_count: 0,
    initialize: Some(initialize),
};
