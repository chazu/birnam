use crate::layout::{Layout, OOP, UNKNOWN};
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::RefCell, ffi::c_void, ptr};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};
#[derive(Clone)]
struct Record<T> {
    identity: usize,
    value: T,
}
struct Model {
    id: Protocol,
    binding_guard: birnam_abi::binding_guard::BindingGuard,
    namespace: Oop,
    roots: Oop,
    gc: Oop,
    receiver: Oop,
    truth: Oop,
    falsity: Oop,
    thread: std::thread::ThreadId,
    names: HashMap<&'static [u8], Oop>,
    // Same retained Symbol as the inventory entry; never a cached method.
    representation_query: Oop,
    owners: RefCell<HashMap<usize, Record<Oop>>>,
    instances: RefCell<HashMap<usize, Record<usize>>>,
    layouts: RefCell<HashMap<usize, Record<Layout>>>,
    copies: RefCell<Vec<Rc<[u8]>>>,
    scoped: RefCell<Vec<Oop>>,
}
struct Scope<'a> { values: &'a RefCell<Vec<Oop>>, start: usize }
impl Drop for Scope<'_> {
    fn drop(&mut self) { self.values.borrow_mut().truncate(self.start); }
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|p| !p.is_null()).ok_or_else(invalid)
}
fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
unsafe fn model<'a>(context: *mut c_void) -> Result<&'a Model, Outcome> {
    let model = unsafe { &*context.cast::<Model>() };
    if model.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(model)
}
impl Model {
    unsafe fn send(
        &self,
        receiver: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { self.id.send(receiver, self.names[name], args) }
    }
    unsafe fn identity(&self, value: Oop) -> Result<usize, Outcome> {
        unsafe { self.send(self.id.receiver, b"identityOf:", &[Operand::object(value)]) }?
            .as_word()
            .filter(|n| *n != 0)
            .ok_or_else(invalid)
    }
    unsafe fn behavior(&self, value: Oop) -> Result<Oop, Outcome> {
        object(unsafe { self.send(self.id.receiver, b"behaviorOf:", &[Operand::object(value)]) }?)
    }
    unsafe fn is_behavior(&self, value: Oop) -> Result<bool, Outcome> {
        Ok(
            unsafe { self.send(self.id.receiver, b"isBehavior:", &[Operand::object(value)]) }?
                .as_word()
                .ok_or_else(invalid)?
                == 1,
        )
    }
    unsafe fn rooted(
        &self,
        values: &[Oop],
        operation: impl FnOnce() -> Result<Operand, Outcome>,
    ) -> Result<Operand, Outcome> {
        // This owner is a registered root provider. Publish the entire scope
        // before any callback, and restore exactly the parent's scope on every
        // return path. No send occurs between popping a result and returning it.
        let start = self.scoped.borrow().len();
        self.scoped.borrow_mut().extend(values.iter().copied().filter(|v| !v.is_null()));
        let _scope = Scope { values: &self.scoped, start };
        operation()
    }

    unsafe fn owner(&self, behavior: Oop) -> Result<Option<Oop>, Outcome> {
        let record = self.owners.borrow().get(&behavior.addr()).cloned();
        match record {
            Some(r) if unsafe { self.identity(behavior) }? == r.identity => Ok(Some(r.value)),
            Some(_) => Err(invalid()),
            None => Ok(None),
        }
    }
    unsafe fn layout(&self, behavior: Oop) -> Result<Option<Layout>, Outcome> {
        let record = self.layouts.borrow().get(&behavior.addr()).cloned();
        match record {
            Some(r) if unsafe { self.identity(behavior) }? == r.identity => Ok(Some(r.value)),
            Some(_) => Err(invalid()),
            None => Ok(None),
        }
    }
    unsafe fn image_layout(&self, value: Oop) -> Result<Layout, Outcome> {
        unsafe {
            // Behavior sharing and additional private layers preserve the
            // representation layout inherited from the nearest registered owner.
            let mut behavior = self.behavior(value)?;
            let mut seen = std::collections::HashSet::new();
            loop {
                if !seen.insert(behavior) {
                    return Err(invalid());
                }
                if let Some(layout) = self.layout(behavior)? {
                    return Ok(layout);
                }
                behavior = object(self.send(
                    self.id.receiver,
                    b"parentOf:",
                    &[Operand::object(behavior)],
                )?)?;
            }
        }
    }
    unsafe fn extent(&self, value: Oop) -> Result<usize, Outcome> {
        let record = self.instances.borrow().get(&value.addr()).cloned();
        if let Some(r) = record {
            if unsafe { self.identity(value) }? == r.identity {
                return Ok(r.value);
            }
            return Err(invalid());
        }
        // The default clone may copy empty ordinary objects, never foreign data.
        if unsafe { self.is_behavior(value) }? {
            return Err(invalid());
        }
        match unsafe {
            self.send(
                self.id.receiver,
                b"allocationSizeOf:",
                &[Operand::object(value)],
            )
        }?
        .as_word()
        {
            Some(0) => Ok(0),
            _ => Err(invalid()),
        }
    }
    unsafe fn allocate(&self, behavior: Oop, bytes: usize) -> Result<Operand, Outcome> {
        unsafe {
            // The caller keeps the behavior argument live. The fresh result
            // needs this owner's traced local scope before validation callbacks.
            let value = self.send(behavior, b"allocate", &[Operand::word(bytes)])?;
            let object = object(value)?;
            let start = self.scoped.borrow().len();
            self.scoped.borrow_mut().push(object);
            let _scope = Scope { values: &self.scoped, start };
            let identity = match self.id.send(self.id.receiver, self.representation_query,
                &[value, Operand::object(behavior), Operand::word(bytes)]) {
                Ok(result) if result.kind() == native::UNIT => None,
                Err(error) if error.status == native::UNBOUND => None,
                Ok(result) => Some(result.as_word().filter(|id| *id != 0).ok_or_else(invalid)?),
                Err(error) => return Err(error),
            };
            let identity = if let Some(identity) = identity { identity } else {
                let identity = self.identity(object)?;
                if self.behavior(object)? != behavior
                    || self.send(self.id.receiver, b"allocationSizeOf:", &[value])?.as_word() != Some(bytes)
                { return Err(invalid()); }
                identity
            };
            self.instances.borrow_mut().insert(object.addr(), Record { identity, value: bytes });
            Ok(value)
        }
    }

}
unsafe extern "C" fn enumerate_roots(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        let mut roots = vec![m.receiver, m.namespace, m.roots, m.gc, m.truth, m.falsity];
        roots.extend(m.names.values().copied());
        roots.extend(m.scoped.borrow().iter().copied());
        // Ownership/instance/layout keys are weak. Owners are reached only from
        // their live behaviors, never from the module's own root inventory.
        for value in roots {
            m.id.send_named(visitor, b"visit:", &[Operand::object(value)])?;
        }
        // A clone's payload snapshot remains rooted even if an allocating method
        // changes the source before collecting. Copies are this owner's storage.
        let copies = m.copies.borrow().clone();
        for copy in copies {
            for word in copy.chunks_exact(size_of::<usize>()) {
                let word = usize::from_ne_bytes(word.try_into().unwrap());
                if let Some(value) = m
                    .send(m.id.receiver, b"allocationAt:", &[Operand::word(word)])?
                    .as_object()
                {
                    m.id.send_named(visitor, b"visit:", &[Operand::object(value)])?;
                }
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn retire(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([bytes]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let bytes = bytes.as_bytes().ok_or_else(invalid)?;
        let bytes = borrowed_bytes(bytes.pointer, bytes.length).ok_or_else(invalid)?;
        if bytes.len() % size_of::<usize>() != 0 {
            return Err(invalid());
        }
        let dead: HashSet<_> = bytes
            .chunks_exact(size_of::<usize>())
            .map(|b| usize::from_ne_bytes(b.try_into().unwrap()))
            .collect();
        m.owners.borrow_mut().retain(|a, _| !dead.contains(a));
        m.instances.borrow_mut().retain(|a, _| !dead.contains(a));
        m.layouts.borrow_mut().retain(|a, _| !dead.contains(a));
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
        let m = model(context)?;
        let Some([value, visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let value = object(*value)?;
        let visitor = object(*visitor)?;
        let emit = |edge| {
            m.id.send_named(visitor, b"visit:", &[Operand::object(edge)])
        };
        if m.is_behavior(value)? {
            if let Some(owner) = m.owner(value)? {
                emit(owner)?;
            }
        } else {
            let bytes = m.extent(value)?;
            let precise = m
                .send(m.gc, b"precisePayloads", &[])?
                .as_word()
                .ok_or_else(invalid)?
                != 0;
            let layout = if precise {
                m.layout(m.behavior(value)?)?
            } else {
                None
            };
            for index in 0..bytes.div_ceil(size_of::<Oop>()) {
                let kind = layout.as_ref().map_or(UNKNOWN, |l| l.kind(index));
                if layout.is_some() && kind != OOP {
                    continue;
                }
                // Only this owner's ordinary storage is read here. With no exact
                // layout, preserve conservative admission of complete padded words.
                let word = value.cast::<usize>().add(index).read();
                if let Some(edge) = m
                    .send(m.id.receiver, b"allocationAt:", &[Operand::word(word)])?
                    .as_object()
                {
                    emit(edge)?;
                } else if layout.is_some() && word != 0 {
                    return Err(invalid());
                }
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn query_owner(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([b]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        Ok(m.owner(b.as_object().ok_or_else(invalid)?)?
            .map(Operand::object)
            .unwrap_or_else(Operand::unit))
    })())
}
unsafe extern "C" fn assign_owner(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([b, v]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let b = object(*b)?;
        let v = if v.kind() == native::UNIT {
            ptr::null_mut()
        } else {
            object(*v)?
        };
        m.rooted(&[b, v], || {
            m.send(m.gc, b"prepareAllocation", &[])?;
            if !m.is_behavior(b)? {
                return Err(invalid());
            }
            let identity = m.identity(b)?;
            if !v.is_null() {
                m.identity(v)?;
                m.owners
                    .borrow_mut()
                    .insert(b.addr(), Record { identity, value: v });
            } else {
                m.owners.borrow_mut().remove(&b.addr());
            }
            Ok(Operand::object(b))
        })
    })())
}
unsafe extern "C" fn prepare(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([v]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        m.rooted(&[v], || {
            m.send(m.gc, b"prepareAllocation", &[])?;
            Ok(Operand::object(m.behavior(v)?))
        })
    })())
}
unsafe extern "C" fn ensure(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([v]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        m.rooted(&[v], || {
            let behavior = object(m.send(
                m.id.receiver,
                b"prepareBehaviorMutationOf:",
                &[Operand::object(v)],
            )?)?;
            if behavior == m.id.receiver {
                return Ok(Operand::object(v));
            }
            m.rooted(&[behavior], || {
                if m.send(m.id.receiver, b"ownerOf:", &[Operand::object(behavior)])?
                    .as_object()
                    == Some(v)
                {
                    return Ok(Operand::object(behavior));
                }
                let private = object(m.send(behavior, b"delegated", &[])?)?;
                m.rooted(&[private], || {
                    m.send(
                        m.id.receiver,
                        b"ownerOf:put:",
                        &[Operand::object(private), Operand::object(v)],
                    )?;
                    m.send(
                        m.id.receiver,
                        b"reassociateBehaviorOf:with:",
                        &[Operand::object(v), Operand::object(private)],
                    )?;
                    Ok(Operand::object(private))
                })
            })
        })
    })())
}
unsafe extern "C" fn clone_object(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        if count != 0 {
            return Err(invalid());
        }
        let m = model(context)?;
        m.rooted(&[receiver], || {
            m.send(m.gc, b"prepareAllocation", &[])?;
            let bytes = m.extent(receiver)?.div_ceil(size_of::<Oop>()) * size_of::<Oop>();
            let behavior = m.behavior(receiver)?;
            // Snapshot before the allocating send; the source remains rooted.
            let payload: Rc<[u8]> = borrowed_bytes(receiver.cast(), bytes)
                .ok_or_else(invalid)?
                .into();
            m.copies.borrow_mut().push(payload.clone());
            let answer = (|| {
                let copy = m.allocate(behavior, bytes)?;
                ptr::copy_nonoverlapping(payload.as_ptr(), object(copy)?.cast(), bytes);
                Ok(copy)
            })();
            m.copies.borrow_mut().pop();
            answer
        })
    })())
}
unsafe extern "C" fn allocate(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([behavior, bytes]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        m.allocate(object(*behavior)?, bytes.as_word().ok_or_else(invalid)?)
    })())
}
unsafe extern "C" fn set_layout(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([behavior, kinds, rest]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let behavior = object(*behavior)?;
        let kinds = kinds.as_bytes().ok_or_else(invalid)?;
        let kinds = borrowed_bytes(kinds.pointer, kinds.length)
            .ok_or_else(invalid)?
            .to_vec();
        let rest = u8::try_from(rest.as_word().ok_or_else(invalid)?).map_err(|_| invalid())?;
        m.rooted(&[behavior], || {
            // Replacing owned metadata allocates no language object. Trace hooks
            // may change a later object's layout during the current collection.
            if !m.is_behavior(behavior)? {
                return Err(invalid());
            }
            let identity = m.identity(behavior)?;
            m.layouts.borrow_mut().insert(
                behavior.addr(),
                Record {
                    identity,
                    value: Layout::new(&kinds, rest),
                },
            );
            Ok(Operand::unit())
        })
    })())
}
unsafe extern "C" fn slot_kind(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([behavior, index]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let behavior = object(*behavior)?;
        if !m.is_behavior(behavior)? {
            return Err(invalid());
        }
        let index = index.as_word().ok_or_else(invalid)?;
        Ok(Operand::word(
            m.layout(behavior)?
                .as_ref()
                .map_or(UNKNOWN, |l| l.kind(index)) as usize,
        ))
    })())
}
unsafe extern "C" fn all_oop(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([behavior, words]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let behavior = object(*behavior)?;
        if !m.is_behavior(behavior)? {
            return Err(invalid());
        }
        let words = words.as_word().ok_or_else(invalid)?;
        Ok(Operand::word(usize::from(
            m.layout(behavior)?
                .as_ref()
                .is_some_and(|l| l.all_oop(words)),
        )))
    })())
}
// Language-facing reference storage. Vector/Integer representations remain
// opaque; this owner alone allocates, describes and accesses ordinary slots.
unsafe fn named(m: &Model, name: &[u8]) -> Result<Oop, Outcome> {
    unsafe { object(m.send(m.namespace, b"at:", &[Operand::object(m.names[name])])?) }
}
unsafe extern "C" fn new_slots(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let [values, parent] = borrowed_slice(a, n).ok_or_else(invalid)? else { return Err(invalid()); };
        let (values, parent) = (object(*values)?, object(*parent)?);
        m.rooted(&[r, values, parent], || {
            let integer = named(m, b"Integer")?;
            let size = object(m.id.send_named(values, b"size", &[])?)?;
            m.rooted(&[integer, size], || {
                let count = m.id.send_named(integer, b"wordOf:", &[Operand::object(size)])?.as_word().filter(|n| *n <= 1_048_576).ok_or_else(invalid)?;
                let behavior = object(m.id.send_named(parent, b"delegated", &[])?)?;
                m.rooted(&[behavior], || {
                    let kinds = vec![OOP; count];
                    m.send(m.receiver, b"setLayoutOf:to:rest:", &[Operand::object(behavior), Operand::bytes(&kinds), Operand::word(UNKNOWN as usize)])?;
                    let result = object(m.allocate(behavior, count.checked_mul(size_of::<Oop>()).ok_or_else(invalid)?)?)?;
                    m.rooted(&[result], || {
                        for i in 0..count {
                            let index = object(m.id.send_named(integer, b"fromWord:", &[Operand::word(i)])?)?;
                            m.rooted(&[index], || {
                                let value = object(m.id.send_named(values, b"at:", &[Operand::object(index)])?)?;
                                m.rooted(&[value], || m.send(m.receiver, b"slotAt:in:put:", &[Operand::word(i), Operand::object(result), Operand::object(value)]))
                            })?;
                        }
                        m.send(m.id.receiver, b"ownerOf:put:", &[Operand::object(behavior), Operand::object(result)])?;
                        Ok(Operand::object(result))
                    })
                })
            })
        })
    })())
}
// Snapshot the owned reference-slot indices before allocating language results.
// Foreign payloads are never read; private vtables inherit their nearest layout.
unsafe extern "C" fn reference_slots(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else { return Err(invalid()); };
        let value = object(*value)?;
        m.rooted(&[r, value], || {
            let owned = m.instances.borrow().contains_key(&value.addr());
            if !owned && (m.is_behavior(value)? || m.send(m.id.receiver, b"allocationSizeOf:",
                &[Operand::object(value)])?.as_word() != Some(0)) {
                return Ok(Operand::object(named(m, b"nil")?));
            }
            let words = m.extent(value)?.div_ceil(size_of::<Oop>());
            let indices = if words == 0 { Vec::new() } else {
                let start = m.scoped.borrow().len();
                let _scope = Scope { values: &m.scoped, start };
                let mut behavior = m.behavior(value)?;
                let mut seen = HashSet::new();
                let layout = loop {
                    // A live metadata callback may detach this behavior from the
                    // object before collecting. The traversal owns its snapshot.
                    m.scoped.borrow_mut().push(behavior);
                    if !seen.insert(behavior) { break None; }
                    if let Some(layout) = m.layout(behavior)? { break Some(layout); }
                    let parent = m.send(m.id.receiver, b"parentOf:", &[Operand::object(behavior)])?;
                    match parent.as_object().filter(|p| !p.is_null()) {
                        Some(parent) => behavior = parent,
                        None if parent.kind() == native::UNIT => break None,
                        _ => return Err(invalid()),
                    }
                };
                let Some(layout) = layout else { return Ok(Operand::object(named(m, b"nil")?)); };
                (0..words).filter(|index| layout.kind(*index) == OOP).collect::<Vec<_>>()
            };
            let vector = named(m, b"Vector")?;
            m.rooted(&[vector], || {
                let integer = named(m, b"Integer")?;
                m.rooted(&[integer], || {
                    let mut result = object(m.send(vector, b"new", &[])?)?;
                    // Keep all intermediates in this owner's nested scope before
                    // constructors or persistent Vector additions can collect.
                    let start = m.scoped.borrow().len();
                    let _scope = Scope { values: &m.scoped, start };
                    m.scoped.borrow_mut().push(result);
                    for index in indices {
                        let index = object(m.send(integer, b"fromWord:", &[Operand::word(index)])?)?;
                        m.scoped.borrow_mut().push(index);
                        result = object(m.send(result, b"add:", &[Operand::object(index)])?)?;
                        m.scoped.borrow_mut().push(result);
                    }
                    Ok(Operand::object(result))
                })
            })
        })
    })())
}

unsafe fn language_slot(c: *mut c_void, r: Oop, a: *const Operand, n: usize, put: bool) -> Result<Operand, Outcome> {
    unsafe {
        let m = model(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != if put { 3 } else { 2 } { return Err(invalid()); }
        let values = [r, object(args[0])?, object(args[1])?,
            if put { object(args[2])? } else { ptr::null_mut() }];
        m.rooted(&values, || {
            let integer = named(m, b"Integer")?;
            m.rooted(&[integer], || {
                let index = m.send(integer, b"wordOf:", &args[..1])?.as_word().ok_or_else(invalid)?;
                let raw = [Operand::word(index), args[1], if put { args[2] } else { Operand::unit() }];
                let value = m.send(r, if put { b"slotAt:in:put:" } else { b"slotAt:in:" }, &raw[..n])?;
                if value.kind() == native::UNIT { Ok(Operand::object(named(m, b"nil")?)) } else { Ok(value) }
            })
        })
    }
}
unsafe extern "C" fn boxed_slot(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome { done(unsafe { language_slot(c, r, a, n, false) }) }
unsafe extern "C" fn boxed_slot_put(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome { done(unsafe { language_slot(c, r, a, n, true) }) }

unsafe extern "C" fn slot_at(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([index, value]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let index = index.as_word().ok_or_else(invalid)?;
        let value = object(*value)?;
        if index >= m.extent(value)?.div_ceil(size_of::<Oop>()) {
            return Err(invalid());
        }
        if m.layout(m.behavior(value)?)?
            .as_ref()
            .is_some_and(|l| l.kind(index) != OOP)
        {
            return Err(invalid());
        }
        let word = value.cast::<usize>().add(index).read();
        if word == 0 {
            return Ok(Operand::unit());
        }
        let value = m.send(m.id.receiver, b"allocationAt:", &[Operand::word(word)])?;
        object(value)?;
        Ok(value)
    })())
}
unsafe extern "C" fn slot_put(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(context)?;
        let Some([index, value, edge]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let index = index.as_word().ok_or_else(invalid)?;
        let value = object(*value)?;
        let edge = if edge.kind() == native::UNIT {
            ptr::null_mut()
        } else {
            object(*edge)?
        };
        m.rooted(&[value, edge], || {
            m.send(m.gc, b"prepareAllocation", &[])?;
            if index >= m.extent(value)?.div_ceil(size_of::<Oop>()) {
                return Err(invalid());
            }
            if m.layout(m.behavior(value)?)?
                .as_ref()
                .is_some_and(|l| l.kind(index) != OOP)
            {
                return Err(invalid());
            }
            if !edge.is_null() {
                m.identity(edge)?;
            }
            value.cast::<Oop>().add(index).write(edge);
            Ok(Operand::object(value))
        })
    })())
}
// A generic owner-thread conditional replacement of one reference slot. All
// collecting/reentrant sends finish before comparing and publishing the edge.
// No Globals, Store, scheduler, or transaction policy belongs in this primitive.
unsafe extern "C" fn replace_slot(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let Some([index, value, expected, replacement]) = borrowed_slice(a, n) else { return Err(invalid()) };
        let [index, value, expected, replacement] = [object(*index)?, object(*value)?, object(*expected)?, object(*replacement)?];
        m.rooted(&[index, value, expected, replacement], || {
            let integer = named(m, b"Integer")?;
            m.rooted(&[integer], || {
                let index = m.send(integer, b"wordOf:", &[Operand::object(index)])?.as_word().ok_or_else(invalid)?;
                let yes = named(m, b"true")?;
                m.rooted(&[yes], || {
                    let no = named(m, b"false")?;
                    m.rooted(&[no], || {
                        m.send(m.gc, b"prepareAllocation", &[])?;
                        if index >= m.extent(value)?.div_ceil(size_of::<Oop>()) || m.image_layout(value)?.kind(index) != OOP {
                            return Err(invalid());
                        }
                        m.identity(expected)?;
                        m.identity(replacement)?;
                        let slot = value.cast::<Oop>().add(index);
                        let matches = slot.read() == expected;
                        if matches { slot.write(replacement); }
                        Ok(Operand::object(if matches { yes } else { no }))
                    })
                })
            })
        })
    })())
}
// Image transports typed slots; Object Model alone reads its owned storage.
unsafe extern "C" fn image_size(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        Ok(Operand::word(m.extent(object(*v)?)?))
    })())
}
unsafe extern "C" fn image_slot(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let Some([i, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let i = i.as_word().ok_or_else(invalid)?;
        let v = object(*v)?;
        m.rooted(&[v], || {
            if i >= m.extent(v)? / size_of::<usize>() {
                return Err(invalid());
            }
            let kind = m.image_layout(v)?.kind(i);
            let word = v.cast::<usize>().add(i).read();
            match kind {
                1 if word == 0 => Ok(Operand::unit()),
                1 => {
                    let value = m.send(m.id.receiver, b"allocationAt:", &[Operand::word(word)])?;
                    object(value)?;
                    Ok(value)
                }
                0 => Ok(Operand::word(word)),
                _ => Err(invalid()),
            }
        })
    })())
}
unsafe extern "C" fn image_put(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let Some([i, v, x]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let i = i.as_word().ok_or_else(invalid)?;
        let v = object(*v)?;
        m.rooted(&[v, x.as_object().unwrap_or(ptr::null_mut())], || {
            m.send(m.gc, b"prepareAllocation", &[])?;
            if i >= m.extent(v)? / size_of::<usize>() {
                return Err(invalid());
            }
            let kind = m.image_layout(v)?.kind(i);
            let word = match (kind, x.kind()) {
                (1, native::OBJECT) => object(*x)?.addr(),
                (1, native::UNIT) => 0,
                (0, native::WORD) => x.as_word().ok_or_else(invalid)?,
                _ => return Err(invalid()),
            };
            v.cast::<usize>().add(i).write(word);
            Ok(Operand::object(v))
        })
    })())
}
const NAMES: &[&[u8]] = &[
    b"Integer", b"nil", b"true", b"false", b"Vector", b"at:", b"wordOf:", b"fromWord:", b"add:",
    b"referenceSlotsOf:",
    b"replaceSlot:in:expected:with:",
    b"newSlots:parent:", b"slot:in:", b"slot:in:put:",
    b"canDiscardClone:of:",
    b"canOmitAllocationFor:",
    b"usesDefaultLookup:",
    b"behaviorEpoch",
    b"bind:to:",
    b"nativeBindingsMatch:",
    b"nativeBindingQuerySemantics:for:",
    b"collectionEquals:",
    b"collectionHash",
    b"symbolBytesOf:",
    b"identityOf:",
    b"behaviorOf:",
    b"isBehavior:",
    b"allocationSizeOf:",
    b"tryRepresentationIdentityOf:behavior:bytes:",
    b"allocationAt:",
    b"retain:",
    b"release:",
    b"prepareAllocation",
    b"allocate",
    b"delegated",
    b"parentOf:",
    b"ownerOf:",
    b"ownerOf:put:",
    b"prepareBehaviorMutationOf:",
    b"reassociateBehaviorOf:with:",
    b"ensurePrivateVtableFor:",
    b"rootsDo:",
    b"referencesOf:do:",
    b"retire:",
    b"allocate:bytes:",
    b"setLayoutOf:to:rest:",
    b"slotKindOf:at:",
    b"allOopLayoutOf:words:",
    b"imageSizeOf:",
    b"imageSlotAt:in:",
    b"restoreImageSlotAt:in:put:",
    b"slotAt:in:",
    b"slotAt:in:put:",
    b"clone",
    b"new",
    b"addMethod",
    b"register:",
    b"trace:with:",
    b"precisePayloads",
    b"registerRetirement:",
];
unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| -> Result<Oop, Outcome> {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let retain = id.intern_name(b"retain:")?;
        let release = id.intern_name(b"release:")?;
        let mut tokens = Vec::new();
        let mut keep = |v: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send(roots, retain, &[Operand::object(v)])?
                    .as_word()
                    .ok_or_else(invalid)?,
            );
            Ok(v)
        };
        let mut names = HashMap::new();
        for name in NAMES {
            names.insert(*name, keep(id.intern_name(name)?)?);
        }
        let prototype = object(id.send_named(id.receiver, b"objectPrototype", &[])?)?;
        let object_behavior =
            object(id.send_named(id.receiver, b"behaviorOf:", &[Operand::object(prototype)])?)?;
        let behavior = keep(object(id.send_named(
            object_behavior,
            b"delegated",
            &[],
        )?)?)?;
        let receiver = keep(object(id.send_named(
            behavior,
            b"allocate",
            &[Operand::word(0)],
        )?)?)?;
        let binding_guard = birnam_abi::binding_guard::BindingGuard::discover(id,
            birnam_abi::binding_guard::SERVICES.map(|name| names[name]),
            names[b"nativeBindingQuerySemantics:for:".as_slice()]);
        let pointer = Box::into_raw(Box::new(Model {
            id,
            binding_guard,
            namespace,
            roots,
            gc,
            receiver,
            truth: named(b"true")?,
            falsity: named(b"false")?,
            thread: std::thread::current().id(),
            representation_query: names[b"tryRepresentationIdentityOf:behavior:bytes:".as_slice()],
            names,
            owners: RefCell::new(HashMap::new()),
            instances: RefCell::new(HashMap::new()),
            layouts: RefCell::new(HashMap::new()),
            copies: RefCell::new(Vec::new()),
            scoped: RefCell::new(Vec::new()),
        }));
        for (target, name, method) in [
            (behavior, b"rootsDo:".as_slice(), enumerate_roots as Method),
            (behavior, b"referencesOf:do:", references),
            (behavior, b"retire:", retire),
            (behavior, b"allocate:bytes:", allocate),
            (behavior, b"canDiscardClone:of:", can_discard_clone),
            (behavior, b"canOmitAllocationFor:", can_omit_allocation),
            (behavior, b"setLayoutOf:to:rest:", set_layout),
            (behavior, b"slotAt:in:", slot_at),
            (behavior, b"newSlots:parent:", new_slots),
            (behavior, b"slot:in:", boxed_slot),
            (behavior, b"referenceSlotsOf:", reference_slots),
            (behavior, b"slot:in:put:", boxed_slot_put),
            (behavior, b"replaceSlot:in:expected:with:", replace_slot),
            (behavior, b"imageSizeOf:", image_size),
            (behavior, b"imageSlotAt:in:", image_slot),
            (behavior, b"restoreImageSlotAt:in:put:", image_put),
            (behavior, b"slotAt:in:put:", slot_put),
            (behavior, b"slotKindOf:at:", slot_kind),
            (behavior, b"allOopLayoutOf:words:", all_oop),
            (id.receiver, b"ownerOf:", query_owner),
            (id.receiver, b"ownerOf:put:", assign_owner),
            (id.receiver, b"prepareBehaviorMutationOf:", prepare),
            (id.receiver, b"ensurePrivateVtableFor:", ensure),
            (object_behavior, b"collectionEquals:", collection_equal),
            (object_behavior, b"collectionHash", collection_hash),
            (
                (*pointer).behavior(keep(id.intern_name(b"Symbol")?)?)?,
                b"collectionHash",
                symbol_hash,
            ),
            (object_behavior, b"clone", clone_object),
            (object_behavior, b"new", clone_object),
        ] {
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object((&(*pointer).names)[name]),
                    Operand::entry(NativeEntry::new(method, pointer.cast())),
                ],
            )?;
        }
        // Canonical associations are made only from completed, already rooted objects.
        for value in [
            prototype,
            keep(id.intern_name(b"Symbol")?)?,
            keep(named(b"nil")?)?,
            keep(named(b"true")?)?,
            namespace,
        ] {
            let b = (*pointer).behavior(value)?;
            let identity = (*pointer).identity(b)?;
            (*pointer)
                .owners
                .borrow_mut()
                .insert(b.addr(), Record { identity, value });
        }
        id.send_named(roots, b"register:", &[Operand::object(receiver)])?;
        id.send_named(gc, b"registerRetirement:", &[Operand::object(receiver)])?;
        for target in [object_behavior, id.receiver] {
            id.send_named(
                gc,
                b"trace:with:",
                &[Operand::object(target), Operand::object(receiver)],
            )?;
        }
        id.send_named(
            namespace,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"ObjectModel")?),
                Operand::object(receiver),
            ],
        )?;
        for token in tokens {
            id.send(roots, release, &[Operand::word(token)])?;
        }
        Ok(Operand::object(receiver))
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
            &[Operand::object(id.intern_name(b"ObjectModel")?)],
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

unsafe extern "C" fn collection_equal(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let Some([other]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        Ok(Operand::object(if r == object(*other)? {
            m.truth
        } else {
            m.falsity
        }))
    })())
}
unsafe extern "C" fn collection_hash(
    c: *mut c_void,
    r: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        model(c)?;
        if count != 0 {
            return Err(invalid());
        }
        Ok(Operand::word(mix(r.addr() as u64)))
    })())
}
unsafe extern "C" fn symbol_hash(
    c: *mut c_void,
    r: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        if count != 0 {
            return Err(invalid());
        }
        // The caller keeps the Symbol alive. Consume its owner's byte borrow
        // before any other message; no registry cleanup is needed for an input.
        let raw = m.send(m.id.receiver, b"symbolBytesOf:", &[Operand::object(r)])?
            .as_bytes().ok_or_else(invalid)?;
        let mut hash = 2_166_136_261u32;
        for byte in borrowed_bytes(raw.pointer, raw.length).ok_or_else(invalid)?
            .iter().take_while(|b| **b != 0)
        { hash = (hash ^ u32::from(*byte)).wrapping_mul(16_777_619); }
        Ok(Operand::word(hash as usize))
    })())
}
fn mix(mut value: u64) -> usize {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u32 as usize
}

impl Model {
    unsafe fn can_omit_allocation(
        &self,
        behavior: Oop,
        context: *mut c_void,
    ) -> Result<bool, Outcome> {
        unsafe {
            let admitted = self.binding_guard.admits(self.id, &[(
                self.receiver, self.names[b"allocate:bytes:".as_slice()],
                NativeEntry::new(allocate, context),
            )])?;
            if !admitted && (self
                .send(
                    self.id.receiver,
                    b"usesDefaultLookup:",
                    &[Operand::object(self.receiver)],
                )?
                .as_word()
                != Some(1)
                || self
                    .send(
                        self.id.receiver,
                        b"bind:to:",
                        &[
                            Operand::object(self.names[b"allocate:bytes:".as_slice()]),
                            Operand::object(self.receiver),
                        ],
                    )?
                    .as_entry()
                    != Some(NativeEntry::new(allocate, context)))
            {
                return Ok(false);
            }
            Ok(self
                .send(
                    self.gc,
                    b"canOmitAllocationFor:",
                    &[Operand::object(behavior)],
                )?
                .as_word()
                == Some(1))
        }
    }
}
unsafe extern "C" fn can_omit_allocation(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let Some([behavior]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let behavior = object(*behavior)?;
        // This fixed permission query only uses its caller-covered argument.
        Ok(Operand::word(usize::from(m.can_omit_allocation(behavior, c)?)))
    })())
}
unsafe extern "C" fn can_discard_clone(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let m = model(c)?;
        let Some([selected, receiver]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let selected = selected.as_entry().ok_or_else(invalid)?;
        let receiver = object(*receiver)?;
        if selected != NativeEntry::new(clone_object, c) {
            return Ok(Operand::word(0));
        }
        m.rooted(&[receiver], || {
            let epoch = m
                .send(m.id.receiver, b"behaviorEpoch", &[])?
                .as_word()
                .ok_or_else(invalid)?;
            if epoch == usize::MAX
                || m.send(
                    m.id.receiver,
                    b"usesDefaultLookup:",
                    &[Operand::object(receiver)],
                )?
                .as_word()
                    != Some(1)
            {
                return Ok(Operand::word(0));
            }
            // Restrict this first consumer to ordinary zero-slot instances.
            // Unsupported shapes decline; the actual clone owns their errors.
            if m.extent(receiver).ok() != Some(0) {
                return Ok(Operand::word(0));
            }
            let behavior = m.behavior(receiver)?;
            let permitted = m.can_omit_allocation(behavior, c)?;
            Ok(Operand::word(usize::from(
                permitted && m.send(m.id.receiver, b"behaviorEpoch", &[])?.as_word() == Some(epoch),
            )))
        })
    })())
}
