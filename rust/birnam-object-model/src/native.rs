//! Ordinary object storage. Layouts and allocation ownership belong to this owner.
use crate::layout::{Layout, OOP, UNKNOWN};
use birnam_abi::{
    self as abi, Oop,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::atomic::{AtomicPtr, Ordering},
};
#[derive(Clone)]
struct Record<T> {
    identity: usize,
    value: T,
}
struct Model {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    gc: Oop,
    receiver: Oop,
    image: Oop,
    meta: Oop,
    meta_owner: Oop,
    truth: Oop,
    falsity: Oop,
    names: HashMap<&'static [u8], Oop>,
    allocate_method: Cell<Oop>,
    clone_method: Cell<Oop>,
    owners: RefCell<HashMap<usize, Record<Oop>>>,
    instances: RefCell<HashMap<usize, Record<usize>>>,
    layouts: RefCell<HashMap<usize, Record<Layout>>>,
    copies: RefCell<Vec<Rc<[u8]>>>,
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
static INSTANCE: AtomicPtr<Model> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
unsafe fn model<'a>(c: Oop) -> Result<&'a Model, Oop> {
    let raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        let api = API.load(Ordering::Acquire);
        if api.is_null() {
            std::process::abort();
        }
        return Err(unsafe { (*api).invalid });
    }
    let m = unsafe { &*raw };
    if c == m.receiver {
        Ok(m)
    } else {
        Err(m.id.invalid)
    }
}
impl Model {
    fn invalid(&self) -> Oop {
        abi::object_reference(self.id.invalid)
    }
    unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[name], a) }
    }
    unsafe fn named(&self, name: &[u8]) -> Result<Oop, Oop> {
        let v = unsafe { self.send(self.namespace, b"at:", &[self.names[name]]) }?;
        if v.is_null() {
            Err(self.invalid())
        } else {
            Ok(v)
        }
    }
    unsafe fn identity(&self, v: Oop) -> Result<usize, Oop> {
        abi::as_size(unsafe { self.send(self.id.root, b"identityOf:", &[v]) }?)
            .filter(|n| *n != 0)
            .ok_or(self.invalid())
    }
    unsafe fn behavior(&self, v: Oop) -> Result<Oop, Oop> {
        unsafe { self.send(self.id.root, b"behaviorOf:", &[v]) }
    }
    unsafe fn is_behavior(&self, v: Oop) -> Result<bool, Oop> {
        Ok(unsafe { self.send(self.id.root, b"isBehavior:", &[v]) }? == abi::size(1).unwrap())
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
    unsafe fn owner(&self, b: Oop) -> Result<Option<Oop>, Oop> {
        let row = self.owners.borrow().get(&b.addr()).cloned();
        match row {
            Some(row) if unsafe { self.identity(b) }? == row.identity => Ok(Some(row.value)),
            Some(_) => Err(self.invalid()),
            None => Ok(None),
        }
    }
    unsafe fn layout(&self, b: Oop) -> Result<Option<Layout>, Oop> {
        let row = self.layouts.borrow().get(&b.addr()).cloned();
        match row {
            Some(row) if unsafe { self.identity(b) }? == row.identity => Ok(Some(row.value)),
            Some(_) => Err(self.invalid()),
            None => Ok(None),
        }
    }
    unsafe fn inherited_layout(&self, v: Oop) -> Result<Option<Layout>, Oop> {
        unsafe {
            let _scope = self.scope();
            let mut b = self.behavior(v)?;
            let mut seen = HashSet::new();
            while !b.is_null() {
                self.keep(b);
                if !seen.insert(b) {
                    return Ok(None);
                }
                if let Some(layout) = self.layout(b)? {
                    return Ok(Some(layout));
                }
                b = self.send(self.id.root, b"parentOf:", &[b])?;
            }
            Ok(None)
        }
    }
    unsafe fn extent(&self, v: Oop) -> Result<usize, Oop> {
        let row = self.instances.borrow().get(&v.addr()).cloned();
        if let Some(row) = row {
            return if unsafe { self.identity(v) }? == row.identity {
                Ok(row.value)
            } else {
                Err(self.invalid())
            };
        }
        if unsafe { self.is_behavior(v) }? {
            return Err(self.invalid());
        }
        match abi::as_size(unsafe { self.send(self.id.root, b"allocationSizeOf:", &[v]) }?) {
            Some(0) => Ok(0),
            _ => Err(self.invalid()),
        }
    }
    unsafe fn allocate(&self, behavior: Oop, bytes: usize) -> Result<Oop, Oop> {
        unsafe {
            let _scope = self.scope();
            let size = abi::size(bytes).ok_or(self.invalid())?;
            let value = self.keep(self.send(behavior, b"allocate", &[size])?);
            let query = self.send(
                self.id.root,
                b"tryRepresentationIdentityOf:behavior:bytes:",
                &[value, behavior, size],
            )?;
            let identity = if query.is_null() {
                let identity = self.identity(value)?;
                if self.behavior(value)? != behavior
                    || self.send(self.id.root, b"allocationSizeOf:", &[value])? != size
                {
                    return Err(self.invalid());
                }
                identity
            } else {
                abi::as_size(query)
                    .filter(|n| *n != 0)
                    .ok_or(self.invalid())?
            };
            self.instances.borrow_mut().insert(
                value.addr(),
                Record {
                    identity,
                    value: bytes,
                },
            );
            Ok(value)
        }
    }
    unsafe fn number(&self, value: Oop) -> Result<usize, Oop> {
        if let Some(n) = abi::as_size(value) {
            return Ok(n);
        }
        unsafe {
            let lo = abi::as_size(self.send(value, b"low32", &[])?)
                .filter(|n| *n <= u32::MAX as usize)
                .ok_or(self.invalid())?;
            let hi = abi::as_size(self.send(value, b"high32", &[])?)
                .filter(|n| *n <= u32::MAX as usize)
                .ok_or(self.invalid())?;
            Ok(lo | (hi << 32))
        }
    }
    unsafe fn word(&self, bits: usize) -> Result<Oop, Oop> {
        unsafe {
            let word = self.named(b"Word")?;
            self.send(
                word,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(bits & u32::MAX as usize).unwrap(),
                    abi::size(bits >> 32).unwrap(),
                ],
            )
        }
    }
    unsafe fn reference_word(&self, word: usize, precise: bool) -> Result<Oop, Oop> {
        let value: Oop = ptr::without_provenance_mut(word);
        if value.is_null() || abi::as_size(value).is_some() {
            return Ok(value);
        }
        let Some(address) = abi::size(word) else {
            return if precise {
                Err(self.invalid())
            } else {
                Ok(ptr::null_mut())
            };
        };
        let object = unsafe { self.send(self.id.root, b"allocationAt:", &[address]) }?;
        if precise && object.is_null() {
            Err(self.invalid())
        } else {
            Ok(object)
        }
    }
    unsafe fn check_slot(&self, value: Oop, index: usize) -> Result<(), Oop> {
        unsafe {
            if index >= self.extent(value)?.div_ceil(size_of::<Oop>()) {
                return Err(self.invalid());
            }
            let layout = self.inherited_layout(value)?;
            if layout.as_ref().is_some_and(|l| l.kind(index) != OOP) {
                return Err(self.invalid());
            }
            Ok(())
        }
    }
    unsafe fn can_omit(&self, behavior: Oop) -> Result<bool, Oop> {
        unsafe {
            let witness = self.allocate_method.get();
            if self.send(
                self.id.root,
                b"nativeBindingsMatch:",
                &[
                    self.receiver,
                    self.names[b"allocate:bytes:".as_slice()],
                    witness,
                ],
            )? != abi::size(1).unwrap()
            {
                return Ok(false);
            }
            Ok(self.send(self.gc, b"canOmitAllocationFor:", &[behavior])? == abi::size(1).unwrap())
        }
    }
    unsafe fn perform(&self, op: Op, r: Oop, a: &[Oop]) -> Result<Oop, Oop> {
        let invalid = self.invalid();
        let nil = ptr::null_mut();
        let one = || if let [v] = a { Ok(*v) } else { Err(invalid) };
        unsafe {
            match op {
                Op::Roots => {
                    let visitor = one()?;
                    let mut values = vec![
                        self.receiver,
                        self.namespace,
                        self.roots,
                        self.gc,
                        self.image,
                        self.meta,
                        self.meta_owner,
                        self.truth,
                        self.falsity,
                    ];
                    values.extend(self.names.values().copied());
                    values.extend([self.allocate_method.get(), self.clone_method.get()]);
                    values.extend(self.scoped.borrow().iter().copied());
                    for v in values {
                        self.send(visitor, b"visit:", &[v])?;
                    }
                    let copies = self.copies.borrow().clone();
                    for copy in copies {
                        for word in copy.chunks_exact(size_of::<usize>()) {
                            let word = usize::from_ne_bytes(word.try_into().unwrap());
                            let value = self.reference_word(word, false)?;
                            if !value.is_null() {
                                self.send(visitor, b"visit:", &[value])?;
                            }
                        }
                    }
                    Ok(nil)
                }
                Op::Retire => {
                    let mut owners = self.owners.borrow_mut();
                    let mut instances = self.instances.borrow_mut();
                    let mut layouts = self.layouts.borrow_mut();
                    for value in a {
                        owners.remove(&value.addr());
                        instances.remove(&value.addr());
                        layouts.remove(&value.addr());
                    }
                    Ok(nil)
                }
                Op::References => {
                    let [value, visitor] = a else {
                        return Err(invalid);
                    };
                    if self.is_behavior(*value)? {
                        // Native tables retain responsibility for their own payload.
                        self.send(self.meta_owner, b"referencesOf:do:", &[*value, *visitor])?;
                        if let Some(owner) = self.owner(*value)? {
                            self.send(*visitor, b"visit:", &[owner])?;
                        }
                    } else {
                        let bytes = self.extent(*value)?;
                        let precise =
                            self.number(self.send(self.gc, b"precisePayloads", &[])?)? != 0;
                        let layout = if precise {
                            self.inherited_layout(*value)?
                        } else {
                            None
                        };
                        for i in 0..bytes.div_ceil(size_of::<Oop>()) {
                            if layout.as_ref().is_some_and(|l| l.kind(i) != OOP) {
                                continue;
                            }
                            let word = value.cast::<usize>().add(i).read();
                            let edge = self.reference_word(word, layout.is_some())?;
                            if !edge.is_null() {
                                self.send(*visitor, b"visit:", &[edge])?;
                            }
                        }
                    }
                    Ok(nil)
                }
                Op::Owner => Ok(self.owner(one()?)?.unwrap_or(nil)),
                Op::OwnerPut => {
                    let [behavior, value] = a else {
                        return Err(invalid);
                    };
                    let _scope = self.scope();
                    self.keep(*behavior);
                    self.keep(*value);
                    self.send(self.gc, b"prepareAllocation", &[])?;
                    if !self.is_behavior(*behavior)? {
                        return Err(invalid);
                    }
                    let identity = self.identity(*behavior)?;
                    if value.is_null() {
                        self.owners.borrow_mut().remove(&behavior.addr());
                    } else {
                        self.identity(*value)?;
                        self.owners.borrow_mut().insert(
                            behavior.addr(),
                            Record {
                                identity,
                                value: *value,
                            },
                        );
                    }
                    Ok(*behavior)
                }
                Op::Prepare => {
                    let value = one()?;
                    let _scope = self.scope();
                    self.keep(value);
                    self.send(self.gc, b"prepareAllocation", &[])?;
                    self.behavior(value)
                }
                Op::Ensure => {
                    let value = one()?;
                    let _scope = self.scope();
                    self.keep(value);
                    let behavior = self.keep(self.send(
                        self.id.root,
                        b"prepareBehaviorMutationOf:",
                        &[value],
                    )?);
                    if behavior == self.meta {
                        return Ok(value);
                    }
                    if self.send(self.id.root, b"ownerOf:", &[behavior])? == value {
                        return Ok(behavior);
                    }
                    let private = self.keep(self.send(behavior, b"delegated", &[])?);
                    self.send(self.id.root, b"ownerOf:put:", &[private, value])?;
                    self.send(
                        self.id.root,
                        b"reassociateBehaviorOf:with:",
                        &[value, private],
                    )?;
                    Ok(private)
                }
                Op::Clone => {
                    if !a.is_empty() {
                        return Err(invalid);
                    }
                    let _scope = self.scope();
                    self.keep(r);
                    self.send(self.gc, b"prepareAllocation", &[])?;
                    let bytes = self.extent(r)?.div_ceil(size_of::<Oop>()) * size_of::<Oop>();
                    let behavior = self.behavior(r)?;
                    let payload: Rc<[u8]> =
                        core::slice::from_raw_parts(r.cast::<u8>(), bytes).into();
                    self.copies.borrow_mut().push(payload.clone());
                    let answer = (|| {
                        let copy = self.allocate(behavior, bytes)?;
                        ptr::copy_nonoverlapping(payload.as_ptr(), copy.cast(), bytes);
                        Ok(copy)
                    })();
                    self.copies.borrow_mut().pop();
                    answer
                }
                Op::Allocate => {
                    let [behavior, bytes] = a else {
                        return Err(invalid);
                    };
                    self.allocate(*behavior, self.number(*bytes)?)
                }
                Op::SetLayout => {
                    let [behavior, kinds, rest] = a else {
                        return Err(invalid);
                    };
                    let _scope = self.scope();
                    self.keep(*behavior);
                    self.keep(*kinds);
                    let count = self.number(self.send(*kinds, b"size", &[])?)?;
                    if count > 1_048_576 {
                        return Err(invalid);
                    }
                    let mut bytes = Vec::with_capacity(count);
                    for i in 0..count {
                        let value = self.send(*kinds, b"at:", &[abi::size(i).unwrap()])?;
                        bytes.push(u8::try_from(self.number(value)?).map_err(|_| invalid)?);
                    }
                    let rest = u8::try_from(self.number(*rest)?).map_err(|_| invalid)?;
                    if !self.is_behavior(*behavior)? {
                        return Err(invalid);
                    }
                    let identity = self.identity(*behavior)?;
                    self.layouts.borrow_mut().insert(
                        behavior.addr(),
                        Record {
                            identity,
                            value: Layout::new(&bytes, rest),
                        },
                    );
                    Ok(nil)
                }
                Op::SlotKind => {
                    let [behavior, index] = a else {
                        return Err(invalid);
                    };
                    if !self.is_behavior(*behavior)? {
                        return Err(invalid);
                    }
                    let i = self.number(*index)?;
                    Ok(abi::size(
                        self.layout(*behavior)?
                            .as_ref()
                            .map_or(UNKNOWN, |l| l.kind(i)) as usize,
                    )
                    .unwrap())
                }
                Op::AllOop => {
                    let [behavior, count] = a else {
                        return Err(invalid);
                    };
                    if !self.is_behavior(*behavior)? {
                        return Err(invalid);
                    }
                    let n = self.number(*count)?;
                    Ok(abi::size(usize::from(
                        self.layout(*behavior)?
                            .as_ref()
                            .is_some_and(|l| l.all_oop(n)),
                    ))
                    .unwrap())
                }
                Op::SlotAt => {
                    let [index, value] = a else {
                        return Err(invalid);
                    };
                    let i = self.number(*index)?;
                    self.check_slot(*value, i)?;
                    self.reference_word(value.cast::<usize>().add(i).read(), true)
                }
                Op::SlotPut => {
                    let [index, value, edge] = a else {
                        return Err(invalid);
                    };
                    let i = self.number(*index)?;
                    let _scope = self.scope();
                    self.keep(*value);
                    self.keep(*edge);
                    self.send(self.gc, b"prepareAllocation", &[])?;
                    self.check_slot(*value, i)?;
                    if !edge.is_null() && abi::as_size(*edge).is_none() {
                        self.identity(*edge)?;
                    }
                    value.cast::<Oop>().add(i).write(*edge);
                    Ok(*value)
                }
                Op::ImageSize => Ok(abi::size(self.extent(one()?)?).ok_or(invalid)?),
                Op::ImageSlotKind => {
                    let [index, value] = a else {
                        return Err(invalid);
                    };
                    let i = self.number(*index)?;
                    let _scope = self.scope();
                    self.keep(*value);
                    if i >= self.extent(*value)? / size_of::<usize>() {
                        return Err(invalid);
                    }
                    let layout = self.inherited_layout(*value)?.ok_or(invalid)?;
                    Ok(abi::size(layout.kind(i) as usize).unwrap())
                }
                Op::ImageSlot => {
                    let [index, value] = a else {
                        return Err(invalid);
                    };
                    let i = self.number(*index)?;
                    let _scope = self.scope();
                    self.keep(*value);
                    if i >= self.extent(*value)? / size_of::<usize>() {
                        return Err(invalid);
                    }
                    let layout = self.inherited_layout(*value)?.ok_or(invalid)?;
                    let word = value.cast::<usize>().add(i).read();
                    match layout.kind(i) {
                        OOP => self.reference_word(word, true),
                        UNKNOWN => self.word(word),
                        _ => Err(invalid),
                    }
                }
                Op::ImagePut => {
                    let [index, value, x] = a else {
                        return Err(invalid);
                    };
                    let i = self.number(*index)?;
                    let _scope = self.scope();
                    self.keep(*value);
                    self.keep(*x);
                    self.send(self.gc, b"prepareAllocation", &[])?;
                    if i >= self.extent(*value)? / size_of::<usize>() {
                        return Err(invalid);
                    }
                    let layout = self.inherited_layout(*value)?.ok_or(invalid)?;
                    let word = match layout.kind(i) {
                        OOP => {
                            if !x.is_null() && abi::as_size(*x).is_none() {
                                self.identity(*x)?;
                            }
                            x.addr()
                        }
                        UNKNOWN => self.number(*x)?,
                        _ => return Err(invalid),
                    };
                    value.cast::<usize>().add(i).write(word);
                    Ok(*value)
                }
                Op::CollectionEqual => Ok(if r == one()? {
                    self.truth
                } else {
                    self.falsity
                }),
                Op::CollectionHash => {
                    if !a.is_empty() {
                        return Err(invalid);
                    }
                    Ok(abi::size(mix(r.addr() as u64)).unwrap())
                }
                Op::SymbolHash => {
                    if !a.is_empty() {
                        return Err(invalid);
                    }
                    self.send(r, b"nameHash", &[])
                }
                Op::CanOmit => Ok(abi::size(usize::from(self.can_omit(one()?)?)).unwrap()),
                Op::CanDiscard => {
                    let [selected, value] = a else {
                        return Err(invalid);
                    };
                    let no = abi::size(0).unwrap();
                    let clone = self.clone_method.get();
                    if self.send(self.id.root, b"sameNativeMethod:as:", &[*selected, clone])?
                        != abi::size(1).unwrap()
                    {
                        return Ok(no);
                    }
                    let _scope = self.scope();
                    self.keep(*value);
                    let epoch = self.send(self.id.root, b"behaviorEpoch", &[])?;
                    if self.send(self.id.root, b"usesDefaultLookup:", &[*value])?
                        != abi::size(1).unwrap()
                        || self.extent(*value).ok() != Some(0)
                    {
                        return Ok(no);
                    }
                    let permitted = self.can_omit(self.behavior(*value)?)?;
                    Ok(abi::size(usize::from(
                        permitted && self.send(self.id.root, b"behaviorEpoch", &[])? == epoch,
                    ))
                    .unwrap())
                }
                Op::NewSlots
                | Op::ReferenceSlots
                | Op::LanguageSlot
                | Op::LanguagePut
                | Op::Replace => self.language(op, r, a),
            }
        }
    }
    unsafe fn language(&self, op: Op, r: Oop, a: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            let invalid = self.invalid();
            let _scope = self.scope();
            self.keep(r);
            for &v in a {
                self.keep(v);
            }
            match op {
                Op::NewSlots => {
                    let [values, parent] = a else {
                        return Err(invalid);
                    };
                    let integer = self.keep(self.named(b"Integer")?);
                    let size = self.keep(self.send(*values, b"size", &[])?);
                    let count = self.number(self.send(integer, b"wordOf:", &[size])?)?;
                    if count > 1_048_576 {
                        return Err(invalid);
                    }
                    let behavior = self.keep(self.send(*parent, b"delegated", &[])?);
                    let kinds = self.keep(self.send(
                        self.id.root,
                        b"sequence",
                        &vec![abi::size(OOP as usize).unwrap(); count],
                    )?);
                    self.send(
                        self.receiver,
                        b"setLayoutOf:to:rest:",
                        &[behavior, kinds, abi::size(UNKNOWN as usize).unwrap()],
                    )?;
                    let result = self.keep(self.allocate(
                        behavior,
                        count.checked_mul(size_of::<Oop>()).ok_or(invalid)?,
                    )?);
                    for i in 0..count {
                        let _element = self.scope();
                        let index = self.keep(self.send(
                            integer,
                            b"fromWord:",
                            &[abi::size(i).unwrap()],
                        )?);
                        let value = self.keep(self.send(*values, b"at:", &[index])?);
                        self.send(
                            self.receiver,
                            b"slotAt:in:put:",
                            &[abi::size(i).unwrap(), result, value],
                        )?;
                    }
                    self.send(self.id.root, b"ownerOf:put:", &[behavior, result])?;
                    Ok(result)
                }
                Op::ReferenceSlots => {
                    let [value] = a else {
                        return Err(invalid);
                    };
                    let owned = self.instances.borrow().contains_key(&value.addr());
                    if !owned
                        && (self.is_behavior(*value)?
                            || self.number(self.send(
                                self.id.root,
                                b"allocationSizeOf:",
                                &[*value],
                            )?)? != 0)
                    {
                        return self.named(b"nil");
                    }
                    let words = self.extent(*value)?.div_ceil(size_of::<Oop>());
                    let indices = if words == 0 {
                        Vec::new()
                    } else {
                        let Some(layout) = self.inherited_layout(*value)? else {
                            return self.named(b"nil");
                        };
                        (0..words)
                            .filter(|&i| layout.kind(i) == OOP)
                            .collect::<Vec<_>>()
                    };
                    let vector = self.keep(self.named(b"Vector")?);
                    let integer = self.keep(self.named(b"Integer")?);
                    let mut result = self.keep(self.send(vector, b"new", &[])?);
                    for i in indices {
                        let i = self.keep(self.send(
                            integer,
                            b"fromWord:",
                            &[abi::size(i).unwrap()],
                        )?);
                        result = self.keep(self.send(result, b"add:", &[i])?);
                    }
                    Ok(result)
                }
                Op::LanguageSlot | Op::LanguagePut => {
                    let put = matches!(op, Op::LanguagePut);
                    if a.len() != if put { 3 } else { 2 } {
                        return Err(invalid);
                    }
                    let integer = self.keep(self.named(b"Integer")?);
                    let index = self.number(self.send(integer, b"wordOf:", &a[..1])?)?;
                    let args = [
                        abi::size(index).ok_or(invalid)?,
                        a[1],
                        if put { a[2] } else { ptr::null_mut() },
                    ];
                    let value = self.send(
                        r,
                        if put {
                            b"slotAt:in:put:"
                        } else {
                            b"slotAt:in:"
                        },
                        &args[..a.len()],
                    )?;
                    if value.is_null() {
                        self.named(b"nil")
                    } else {
                        Ok(value)
                    }
                }
                Op::Replace => {
                    let [index, value, expected, replacement] = a else {
                        return Err(invalid);
                    };
                    if expected.is_null() || replacement.is_null() {
                        return Err(invalid);
                    }
                    let integer = self.keep(self.named(b"Integer")?);
                    let i = self.number(self.send(integer, b"wordOf:", &[*index])?)?;
                    let yes = self.keep(self.named(b"true")?);
                    let no = self.keep(self.named(b"false")?);
                    self.send(self.gc, b"prepareAllocation", &[])?;
                    self.check_slot(*value, i)?;
                    for &edge in [expected, replacement] {
                        if abi::as_size(edge).is_none() {
                            self.identity(edge)?;
                        }
                    }
                    let slot = value.cast::<Oop>().add(i);
                    let matches = slot.read() == *expected;
                    if matches {
                        slot.write(*replacement);
                    }
                    Ok(if matches { yes } else { no })
                }
                _ => Err(invalid),
            }
        }
    }
}
fn mix(mut value: u64) -> usize {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u32 as usize
}
macro_rules! operations{($(($name:literal,$op:ident)),*$(,)?)=>{
    #[derive(Clone,Copy)]enum Op{$($op),*}
    #[allow(non_snake_case)]mod entries{use super::*;$(pub unsafe extern "C" fn $op(_id:Oop,c:Oop,r:Oop,a:*const Oop,n:usize)->Oop{
        (||unsafe{let m=model(c)?;let a=abi::borrowed_slice(a,n).ok_or(m.invalid())?;m.perform(Op::$op,r,a)})().unwrap_or_else(abi::fault_reference)
    })*}
    static METHODS:&[MethodSpec]=&[$(MethodSpec{name:index($name),entry:Some(entries::$op)}),*];
};}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"Integer",
    b"Word",
    b"Symbol",
    b"sequence",
    b"nil",
    b"true",
    b"false",
    b"Vector",
    b"at:",
    b"at:put:",
    b"wordOf:",
    b"fromWord:",
    b"add:",
    b"size",
    b"low32",
    b"high32",
    b"fromUnsignedLow:high:",
    b"referenceSlotsOf:",
    b"replaceSlot:in:expected:with:",
    b"newSlots:parent:",
    b"slot:in:",
    b"slot:in:put:",
    b"canDiscardClone:of:",
    b"canOmitAllocationFor:",
    b"usesDefaultLookup:",
    b"behaviorEpoch",
    b"nativeBindingsMatch:",
    b"sameNativeMethod:as:",
    b"collectionEquals:",
    b"collectionHash",
    b"nameHash",
    b"symbolHash",
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
    b"imageSlotKindAt:in:",
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
    b"referenceOwnerOf:",
    b"objectBehavior",
    b"objectPrototype",
    b"vtableBehavior",
    b"symbolBehavior",
    b"methodAt:",
    b"withContext:",
    b"Roots",
    b"GC",
    b"ObjectModel",
    b"Object",
    b"ObjectVt",
    b"Vtable",
    b"visit:"
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
    (b"rootsDo:", Roots),
    (b"referencesOf:do:", References),
    (b"retire:", Retire),
    (b"allocate:bytes:", Allocate),
    (b"canDiscardClone:of:", CanDiscard),
    (b"canOmitAllocationFor:", CanOmit),
    (b"setLayoutOf:to:rest:", SetLayout),
    (b"slotAt:in:", SlotAt),
    (b"newSlots:parent:", NewSlots),
    (b"slot:in:", LanguageSlot),
    (b"referenceSlotsOf:", ReferenceSlots),
    (b"slot:in:put:", LanguagePut),
    (b"replaceSlot:in:expected:with:", Replace),
    (b"imageSizeOf:", ImageSize),
    (b"imageSlotAt:in:", ImageSlot),
    (b"imageSlotKindAt:in:", ImageSlotKind),
    (b"restoreImageSlotAt:in:put:", ImagePut),
    (b"slotAt:in:put:", SlotPut),
    (b"slotKindOf:at:", SlotKind),
    (b"allOopLayoutOf:words:", AllOop),
    (b"ownerOf:", Owner),
    (b"ownerOf:put:", OwnerPut),
    (b"prepareBehaviorMutationOf:", Prepare),
    (b"ensurePrivateVtableFor:", Ensure),
    (b"collectionEquals:", CollectionEqual),
    (b"collectionHash", CollectionHash),
    (b"symbolHash", SymbolHash),
    (b"clone", Clone)
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
        let m = unsafe { &*old };
        return if m.ready.get() && m.namespace == *namespace {
            m.receiver
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
        let truth = named(b"true")?;
        let falsity = named(b"false")?;
        if [roots, gc, truth, falsity].iter().any(|o| o.is_null()) {
            return Err(abi::object_reference(id.invalid));
        }
        let object = send(id.root, b"objectPrototype", &[])?;
        let base = send(id.root, b"objectBehavior", &[])?;
        let meta = send(id.root, b"vtableBehavior", &[])?;
        let symbol_type = send(id.root, b"symbolBehavior", &[])?;
        let meta_owner = send(gc, b"referenceOwnerOf:", &[meta])?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, b"retain:", &[v])?);
                Ok(v)
            };
            let behavior = keep(send(base, b"delegated", &[])?)?;
            let receiver = keep(send(behavior, b"allocate", &[abi::size(0).unwrap()])?)?;
            let m = Box::into_raw(Box::new(Model {
                id,
                namespace: *namespace,
                roots,
                gc,
                receiver,
                image,
                meta,
                meta_owner,
                truth,
                falsity,
                names: names.clone(),
                allocate_method: Cell::new(ptr::null_mut()),
                clone_method: Cell::new(ptr::null_mut()),
                owners: RefCell::new(HashMap::new()),
                instances: RefCell::new(HashMap::new()),
                layouts: RefCell::new(HashMap::new()),
                copies: RefCell::new(Vec::new()),
                scoped: RefCell::new(Vec::new()),
                ready: Cell::new(false),
            }));
            INSTANCE.store(m, Ordering::Release);
            let m = &*m;
            for spec in METHODS {
                let name = NAMES[spec.name];
                let method = m.send(image, b"methodAt:", &[symbols[spec.name]])?;
                let method = m.send(method, b"withContext:", &[receiver])?;
                let target = match name {
                    b"ownerOf:"
                    | b"ownerOf:put:"
                    | b"prepareBehaviorMutationOf:"
                    | b"ensurePrivateVtableFor:" => m.behavior(id.root)?,
                    b"collectionEquals:" | b"collectionHash" | b"clone" => base,
                    b"symbolHash" => symbol_type,
                    _ => behavior,
                };
                let selector = if name == b"symbolHash" {
                    m.names[b"collectionHash".as_slice()]
                } else {
                    symbols[spec.name]
                };
                m.send(target, b"addMethod", &[selector, method])?;
                if name == b"allocate:bytes:" {
                    m.allocate_method.set(method);
                }
                if name == b"clone" {
                    m.clone_method.set(method);
                }
                if name == b"clone" {
                    m.send(target, b"addMethod", &[m.names[b"new".as_slice()], method])?;
                }
            }
            for value in [object, truth, named(b"nil")?, *namespace] {
                let b = m.behavior(value)?;
                let identity = m.identity(b)?;
                m.owners
                    .borrow_mut()
                    .insert(b.addr(), Record { identity, value });
            }
            let identity = m.identity(symbol_type)?;
            m.owners.borrow_mut().insert(
                symbol_type.addr(),
                Record {
                    identity,
                    value: symbols[index(b"Symbol")],
                },
            );
            m.send(roots, b"register:", &[receiver])?;
            m.send(gc, b"registerRetirement:", &[receiver])?;
            for target in [base, meta] {
                m.send(gc, b"trace:with:", &[target, receiver])?;
            }
            for (name, value) in [
                (b"ObjectModel".as_slice(), receiver),
                (b"Object", object),
                (b"ObjectVt", base),
                (b"Vtable", meta),
            ] {
                m.send(*namespace, b"at:put:", &[m.names[name], value])?;
            }
            m.ready.set(true);
            Ok(receiver)
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
