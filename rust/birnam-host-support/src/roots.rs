//! Host's native temporary references, reported through ordinary GC messages.
use crate::{Context, HostError, native_error, size};
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

struct References {
    id: Protocol,
    visit: Oop,
    scopes: Vec<Option<Vec<Oop>>>,
    free: Vec<usize>,
}
thread_local! {
    // This implementation's objects only; entries do not keep objects or scopes alive.
    static OBJECTS: RefCell<HashMap<(usize, usize), Weak<RefCell<References>>>> = RefCell::new(HashMap::new());
}
pub(crate) struct Provider {
    references: Rc<RefCell<References>>,
    object: Oop,
    behavior: Oop,
    token: Oop,
    gc: Oop,
    roots: Oop,
    trace: Oop,
    release: Oop,
}
pub(crate) struct Scope {
    references: Rc<RefCell<References>>,
    index: usize,
}
impl Scope {
    pub fn keep(&self, value: Oop) {
        self.references.borrow_mut().scopes[self.index]
            .as_mut()
            .unwrap()
            .push(value);
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        let mut r = self.references.borrow_mut();
        r.scopes[self.index] = None;
        r.free.push(self.index);
    }
}
impl Provider {
    pub fn scope(&self) -> Scope {
        let mut r = self.references.borrow_mut();
        let index = r.free.pop().unwrap_or_else(|| {
            r.scopes.push(None);
            r.scopes.len() - 1
        });
        r.scopes[index] = Some(Vec::new());
        Scope {
            references: self.references.clone(),
            index,
        }
    }
    pub fn new(h: &Context, method: Oop) -> Result<Self, HostError> {
        let q = h.scope();
        q.keep(method)?;
        let base = q.call(h.id.root, b"objectBehavior", &[])?;
        let behavior = q.call(base, b"delegated", &[])?;
        let object = q.call(behavior, b"allocate", &[size(0)?])?;
        q.call(
            behavior,
            b"addMethod",
            &[
                h.intern(b"referencesOf:do:")
                    .map_err(|e| native_error("trace selector", e))?,
                method,
            ],
        )?;
        let references = Rc::new(RefCell::new(References {
            id: h.id,
            visit: h
                .intern(b"visit:")
                .map_err(|e| native_error("visit selector", e))?,
            scopes: Vec::new(),
            free: Vec::new(),
        }));
        let gc = h.named(b"GC")?;
        let trace = h
            .intern(b"trace:with:")
            .map_err(|e| native_error("trace selector", e))?;
        let release = h
            .intern(b"releaseTokensProtecting")
            .map_err(|e| native_error("release selector", e))?;
        let token = h.send(h.roots, b"retain:", &[object])?;
        let provider = Self {
            references,
            object,
            behavior,
            token,
            gc,
            roots: h.roots,
            trace,
            release,
        };
        OBJECTS.with(|objects| {
            objects.borrow_mut().insert(
                (h.id.root.addr(), object.addr()),
                Rc::downgrade(&provider.references),
            );
        });
        unsafe { h.id.send(gc, trace, &[behavior, object]) }
            .map_err(|e| native_error("Host root tracing", e))?;
        Ok(provider)
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        let id = self.references.borrow().id;
        // No native reference points into an object payload. A retained old callback
        // finds no state after teardown, rather than following a freed native pointer.
        let _ = unsafe { id.send(self.gc, self.trace, &[self.behavior, std::ptr::null_mut()]) };
        OBJECTS.with(|objects| {
            objects
                .borrow_mut()
                .remove(&(id.root.addr(), self.object.addr()));
        });
        let _ = unsafe {
            id.send(
                self.roots,
                self.release,
                &[std::ptr::null_mut(), self.token],
            )
        };
    }
}
pub unsafe extern "C" fn trace_entry(
    root: Oop,
    _: Oop,
    receiver: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    let Some([object, visitor]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return abi::fault_reference(root);
    };
    if *object != receiver {
        return abi::fault_reference(root);
    }
    let state = OBJECTS.with(|objects| {
        objects
            .borrow()
            .get(&(root.addr(), receiver.addr()))
            .and_then(Weak::upgrade)
    });
    let Some(state) = state else {
        return std::ptr::null_mut();
    };
    let (id, visit, values) = {
        let r = state.borrow();
        (
            r.id,
            r.visit,
            r.scopes
                .iter()
                .flatten()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
        )
    };
    for value in values {
        if let Err(e) = unsafe { id.send(*visitor, visit, &[value]) } {
            return abi::fault_reference(e);
        }
    }
    std::ptr::null_mut()
}
