//! Owned root inventory, still using the existing indexed token store. All
//! public arguments, tokens, visitors and results are Oops.
use birnam_abi::{
    self as abi, Oop,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::cell::RefCell;
mod word_index;
use word_index::WordIndex;
#[derive(Default)]
struct Roots {
    providers: Vec<Oop>,
    retained: Vec<(usize, Oop)>,
    positions: WordIndex<usize>,
    next: usize,
}
impl Roots {
    fn retain(&mut self, o: Oop) -> Option<usize> {
        self.next = self.next.checked_add(1).filter(|n| *n <= usize::MAX >> 1)?;
        self.positions.insert(self.next, self.retained.len());
        self.retained.push((self.next, o));
        Some(self.next)
    }
    fn release(&mut self, t: usize) -> bool {
        let Some(i) = self.positions.remove(&t) else {
            return false;
        };
        self.retained.swap_remove(i);
        if let Some((moved, _)) = self.retained.get(i) {
            *self.positions.get_mut(moved).unwrap() = i;
        }
        true
    }
}
static API: std::sync::atomic::AtomicPtr<Protocol> =
    std::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
static INSTANCE: std::sync::atomic::AtomicPtr<Registry> =
    std::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
struct Registry {
    id: Protocol,
    receiver: Oop,
    roots: RefCell<Roots>,
    names: Vec<Oop>,
}
const NAMES: &[&[u8]] = &[
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
    b"referencesOf:do:",
    b"visit:",
];
static LITERALS: [Literal; 12] = [
    Literal::new(NAMES[0]),
    Literal::new(NAMES[1]),
    Literal::new(NAMES[2]),
    Literal::new(NAMES[3]),
    Literal::new(NAMES[4]),
    Literal::new(NAMES[5]),
    Literal::new(NAMES[6]),
    Literal::new(NAMES[7]),
    Literal::new(NAMES[8]),
    Literal::new(NAMES[9]),
    Literal::new(NAMES[10]),
    Literal::new(NAMES[11]),
];
unsafe fn state<'a>(c: Oop) -> Result<&'a Registry, Oop> {
    let raw = INSTANCE.load(std::sync::atomic::Ordering::Acquire);
    if raw.is_null() {
        let api = API.load(std::sync::atomic::Ordering::Acquire);
        if api.is_null() {
            std::process::abort()
        }
        return Err(unsafe { (*api).invalid });
    }
    let s = unsafe { &*raw };
    if c == s.receiver {
        Ok(s)
    } else {
        Err(s.id.invalid)
    }
}
unsafe extern "C" fn retain(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    let Some([v]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.id.invalid;
    };
    s.roots
        .borrow_mut()
        .retain(*v)
        .and_then(abi::size)
        .unwrap_or(s.id.invalid)
}
unsafe extern "C" fn release(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    let Some([t]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.id.invalid;
    };
    let Some(t) = abi::as_size(*t) else {
        return s.id.invalid;
    };
    abi::size(s.roots.borrow_mut().release(t) as usize).unwrap()
}
unsafe extern "C" fn register(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    let Some([o]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.id.invalid;
    };
    s.roots.borrow_mut().providers.push(*o);
    *o
}
unsafe extern "C" fn roots(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    let Some([visitor]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.id.invalid;
    };
    (unsafe {
        (|| {
            let (values, providers) = {
                let r = s.roots.borrow();
                (
                    r.retained.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
                    r.providers.clone(),
                )
            };
            for v in s
                .names
                .iter()
                .copied()
                .chain([s.receiver])
                .chain(values)
                .chain(providers.iter().copied())
            {
                s.id.send(*visitor, s.names[11], &[v])?;
            }
            for p in providers {
                s.id.send(p, s.names[9], &[*visitor])?;
            }
            Ok(core::ptr::null_mut())
        })()
    })
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn references(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    let Some([object, visitor]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.id.invalid;
    };
    if *object != s.receiver {
        return s.id.invalid;
    }
    unsafe { roots(_id, c, s.receiver, visitor, 1) }
}
static METHODS: [MethodSpec; 5] = [
    MethodSpec {
        name: 6,
        entry: Some(retain),
    },
    MethodSpec {
        name: 7,
        entry: Some(release),
    },
    MethodSpec {
        name: 8,
        entry: Some(register),
    },
    MethodSpec {
        name: 9,
        entry: Some(roots),
    },
    MethodSpec {
        name: 10,
        entry: Some(references),
    },
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    _: *const Oop,
    argc: usize,
) -> Oop {
    API.store(p.cast_mut(), std::sync::atomic::Ordering::Release);
    let id = unsafe { *p };
    let existing = INSTANCE.load(std::sync::atomic::Ordering::Acquire);
    if !existing.is_null() {
        return unsafe { (*existing).receiver };
    }
    if argc != 0 {
        return id.invalid;
    }
    let Some(names) = (unsafe { abi::borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if names.len() != NAMES.len() {
        return id.invalid;
    }
    (unsafe {
        (|| {
            let base = id.send(id.root, names[0], &[])?;
            let vt = id.send(base, names[1], &[])?;
            let receiver = id.send(vt, names[2], &[abi::size(size_of::<usize>()).unwrap()])?;
            let s = Box::new(Registry {
                id,
                receiver,
                roots: RefCell::new(Roots::default()),
                names: names.to_vec(),
            });
            let raw = Box::into_raw(s);
            INSTANCE.store(raw, std::sync::atomic::Ordering::Release);
            receiver.cast::<*const Registry>().write(raw);
            for spec in &METHODS {
                let method = id.send(image, names[3], &[names[spec.name]])?;
                let method = id.send(method, names[4], &[receiver])?;
                id.send(vt, names[5], &[names[spec.name], method])?;
            }
            Ok(receiver)
        })()
    })
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
