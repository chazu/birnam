//! Execution-owned callable objects. See the local protocol and migration ledger.
mod control;
mod entry;
mod entry_plan;
mod execution;
mod image;
mod machine;
mod objects;
mod recovery;
mod recovery_graph;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::{collections::HashMap, rc::Rc};

struct Callable {
    resumable: Option<machine::Registered>,
    inline: Option<recovery::Inline>,
    execution: *const Execution,
    object: Oop,
    entry: NativeEntry,
    owner: Oop,
    image: Option<Rc<image::Image>>,
}
struct Execution {
    machines: RefCell<machine::State>,
    requests: RefCell<Vec<recovery::Request>>,
    id: Protocol,
    receiver: Oop,
    roots: Oop,
    gc: Oop,
    model: Oop,
    integer: Oop,
    word: Oop,
    float: Oop,
    error: Oop,
    nil: Oop,
    method_behavior: Oop,
    prototype: Oop,
    thread: std::thread::ThreadId,
    names: HashMap<&'static [u8], Oop>,
    methods: RefCell<HashMap<usize, Box<Callable>>>,
    entries: RefCell<HashMap<usize, Oop>>,
    scoped: RefCell<Vec<Oop>>,
    depth: Cell<usize>,
    draining: Cell<bool>,
    retired_images: RefCell<Vec<Rc<image::Image>>>,
}

// These are the existing scoped execution roots: install them synchronously,
// before the first dependency send, and remove them without another callback.
struct Scope<'a> {
    execution: &'a Execution,
    start: usize,
}
impl Scope<'_> {
    fn keep(&self, object: Oop) {
        if !object.is_null() {
            self.execution.scoped.borrow_mut().push(object);
        }
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.execution.scoped.borrow_mut().truncate(self.start);
    }
}
struct Invocation<'a>(&'a Execution);
impl Drop for Invocation<'_> {
    fn drop(&mut self) {
        self.0.depth.set(self.0.depth.get() - 1);
    }
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(value: Operand) -> Result<Oop, Outcome> {
    value
        .as_object()
        .filter(|v| !v.is_null())
        .ok_or_else(invalid)
}
fn done(value: Result<Operand, Outcome>) -> Outcome {
    value.map_or_else(|e| e, Outcome::loaded)
}
unsafe fn state<'a>(context: *mut c_void) -> Result<&'a Execution, Outcome> {
    let s = unsafe { &*context.cast::<Execution>() };
    if s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
unsafe fn visit(visitor: NativeEntry, value: Oop) -> Result<(), Outcome> {
    unsafe {
        visitor.method.ok_or_else(invalid)?(
            visitor.context,
            ptr::null_mut(),
            [Operand::object(value)].as_ptr(),
            1,
        )
    }
    .into_result()?;
    Ok(())
}
fn visitor(value: Operand) -> Result<NativeEntry, Outcome> {
    value
        .as_entry()
        .filter(|e| e.method.is_some())
        .ok_or_else(invalid)
}
impl Execution {
    fn scope(&self, values: &[Oop]) -> Scope<'_> {
        let scope = Scope {
            execution: self,
            start: self.scoped.borrow().len(),
        };
        for &value in values {
            scope.keep(value);
        }
        scope
    }
    unsafe fn send(
        &self,
        receiver: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { self.id.send(receiver, self.names[name], args) }
    }
    fn selected(&self, entry: NativeEntry) -> Option<Oop> {
        let address = entry.method? as usize;
        if address != call as Method as usize && address != lookup as Method as usize {
            return None;
        }
        self.entries.borrow().get(&entry.context.addr()).copied()
    }
    fn entry(&self, method: Oop, adapt_lookup: bool) -> Result<NativeEntry, Outcome> {
        let values = self.methods.borrow();
        let callable = values.get(&method.addr()).ok_or_else(invalid)?;
        Ok(NativeEntry::new(
            if adapt_lookup { lookup } else { call },
            ptr::from_ref(&**callable).cast_mut().cast(),
        ))
    }
    unsafe fn make(
        &self,
        entry: NativeEntry,
        owner: Oop,
        image: Option<Rc<image::Image>>,
    ) -> Result<Operand, Outcome> {
        if entry.method.is_none() {
            return Err(invalid());
        }
        let roots = self.scope(&[owner]);
        let value =
            object(unsafe { self.send(self.method_behavior, b"allocate", &[Operand::word(0)]) }?)?;
        roots.keep(value);
        let record = Box::new(Callable {
            resumable: None,
            inline: None,
            execution: self,
            object: value,
            entry,
            owner,
            image,
        });
        let address = ptr::from_ref(&*record).addr();
        self.methods.borrow_mut().insert(value.addr(), record);
        self.entries.borrow_mut().insert(address, value);
        Ok(Operand::object(value))
    }
    unsafe fn capture(&self, entry: NativeEntry) -> Result<Operand, Outcome> {
        if let Some(method) = self.selected(entry) {
            let _roots = self.scope(&[method]);
            let (entry, owner, image, resumable, inline) = {
                let values = self.methods.borrow();
                let value = values.get(&method.addr()).ok_or_else(invalid)?;
                (
                    value.entry,
                    value.owner,
                    value.image.clone(),
                    value.resumable.clone(),
                    value.inline,
                )
            };
            let value = unsafe { self.make(entry, owner, image) }?;
            let mut methods = self.methods.borrow_mut();
            let captured = methods.get_mut(&object(value)?.addr()).unwrap();
            captured.resumable = resumable;
            captured.inline = inline;
            return Ok(value);
        }
        let image = image::Image::retain(entry.method.ok_or_else(invalid)? as *const c_void)
            .ok_or_else(invalid)?;
        unsafe { self.make(entry, self.nil, Some(Rc::new(image))) }
    }
    unsafe fn language_result(&self, result: Result<Operand, Outcome>) -> Result<Operand, Outcome> {
        unsafe {
            let kind: &'static [u8] = match result {
                Ok(value) if value.kind() == native::OBJECT => return Ok(value),
                Ok(value) if value.kind() == native::UNIT => return Ok(Operand::object(self.nil)),
                Ok(value) if value.kind() == native::NATIVE_ENTRY => {
                    return self.capture(visitor(value)?);
                }
                Ok(_) => b"nativeResultRequiresWrapper",
                Err(error) if error.status == native::INVALID_REQUEST => b"nativeInvalidRequest",
                Err(_) => b"nativeFailure",
            };
            self.send(self.error, b"kind:", &[Operand::object(self.names[kind])])
        }
    }
    unsafe fn invoke(
        &self,
        method: Oop,
        receiver: Oop,
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        let roots = self.scope(&[method, receiver]);
        for argument in args {
            if let Some(value) = argument.as_object() {
                roots.keep(value);
            }
        }
        let registered = self
            .methods
            .borrow()
            .get(&method.addr())
            .ok_or_else(invalid)?
            .resumable
            .clone();
        if let Some(registered) = registered {
            return unsafe { machine::invoke_registered(self, &registered, receiver, args) };
        }
        let entry = self
            .methods
            .borrow()
            .get(&method.addr())
            .ok_or_else(invalid)?
            .entry;
        self.depth
            .set(self.depth.get().checked_add(1).ok_or_else(invalid)?);
        let _invocation = Invocation(self);
        unsafe {
            entry.method.ok_or_else(invalid)?(entry.context, receiver, args.as_ptr(), args.len())
        }
        .into_result()
    }
}

unsafe extern "C" fn call(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let callable = &*c.cast::<Callable>();
        let s = state(callable.execution.cast_mut().cast())?;
        s.invoke(
            callable.object,
            r,
            borrowed_slice(a, n).ok_or_else(invalid)?,
        )
    })())
}
unsafe extern "C" fn lookup(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let callable = &*c.cast::<Callable>();
        let s = state(callable.execution.cast_mut().cast())?;
        let _roots = s.scope(&[callable.object, r]);
        let result = s.invoke(
            callable.object,
            r,
            borrowed_slice(a, n).ok_or_else(invalid)?,
        )?;
        // Capturing Id's own lookup as a Method preserves its native result
        // convention. Language-defined lookup is the Object-valued case below.
        if result.kind() == native::UNIT {
            return Ok(result);
        }
        if result.kind() == native::NATIVE_ENTRY {
            visitor(result)?;
            return Ok(result);
        }
        let selected = object(result)?;
        if selected == s.nil {
            return Ok(Operand::unit());
        }
        // No send, allocation or shared lookup slot between the owned result
        // and its exact entry. Nested lookups cannot overwrite this selection.
        Ok(Operand::entry(s.entry(selected, false)?))
    })())
}
unsafe extern "C" fn from_native(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([entry, owner]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        s.make(visitor(*entry)?, object(*owner)?, None)
    })())
}
unsafe extern "C" fn identity(c: *mut c_void, r: Oop, _: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        Ok(Operand::object(r))
    })())
}
unsafe extern "C" fn invoke(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([receiver]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let receiver = object(*receiver)?;
        let _roots = s.scope(&[r, receiver]);
        s.language_result(s.invoke(r, receiver, &[]))
    })())
}
unsafe extern "C" fn invoke_arguments(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([receiver, source]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let receiver = object(*receiver)?;
        let source = object(*source)?;
        let roots = s.scope(&[r, receiver, source]);
        let size = object(s.send(source, b"size", &[])?)?;
        roots.keep(size);
        let size = s
            .send(s.integer, b"wordOf:", &[Operand::object(size)])?
            .as_word()
            .ok_or_else(invalid)? as i64;
        let count = usize::try_from(size).map_err(|_| invalid())?;
        let mut args = Vec::new();
        for index in 0..count {
            let index = object(s.send(s.integer, b"fromWord:", &[Operand::word(index)])?)?;
            // Keep the index until its at: send returns; returned arguments are
            // retained before reading the next one, including allocating sources.
            let index_scope = s.scope(&[index]);
            let value = object(s.send(source, b"at:", &[Operand::object(index)])?)?;
            drop(index_scope);
            roots.keep(value);
            args.push(Operand::object(value));
        }
        s.language_result(s.invoke(r, receiver, &args))
    })())
}
unsafe extern "C" fn method_for(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([selector]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let selector = object(*selector)?;
        let roots = s.scope(&[r, selector]);
        let behavior = if s
            .send(s.id.receiver, b"isBehavior:", &[Operand::object(r)])?
            .as_word()
            == Some(1)
        {
            r
        } else {
            object(s.send(s.id.receiver, b"behaviorOf:", &[Operand::object(r)])?)?
        };
        roots.keep(behavior);
        let selected = s.send(behavior, b"lookup", &[Operand::object(selector)])?;
        if selected.kind() == native::UNIT {
            return Ok(Operand::object(s.nil));
        }
        s.capture(visitor(selected)?)
    })())
}
unsafe extern "C" fn install(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([method, selector]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let method = object(*method)?;
        let selector = object(*selector)?;
        let _roots = s.scope(&[r, method, selector]);
        s.send(s.gc, b"prepareAllocation", &[])?;
        let entry = s.entry(method, selector == s.names[b"lookup".as_slice()])?;
        s.send(
            r,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(entry)],
        )?;
        Ok(Operand::object(method))
    })())
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = visitor(*v)?;
        let mut values = vec![
            s.receiver,
            s.roots,
            s.gc,
            s.model,
            s.integer,
            s.word,
            s.float,
            s.error,
            s.nil,
            s.method_behavior,
            s.prototype,
        ];
        values.extend(s.names.values().copied());
        values.extend(s.machines.borrow().roots());
        values.extend(s.scoped.borrow().iter().copied());
        for value in values {
            visit(v, value)?;
        }
        Ok(Operand::unit())
    })())
}
struct Bindings<'a> {
    execution: &'a Execution,
    visitor: NativeEntry,
}
unsafe extern "C" fn binding(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let b = &*c.cast::<Bindings<'_>>();
        let Some([_, value, _]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        if let Some(entry) = value.as_entry() {
            if let Some(method) = b.execution.selected(entry) {
                visit(b.visitor, method)?;
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([target, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let target = object(*target)?;
        let v = visitor(*v)?;
        // Copy the owned edge before visiting: a callback may reenter this owner.
        let owner = s.methods.borrow().get(&target.addr()).map(|m| {
            (
                m.owner,
                m.resumable.as_ref().map(|r| r.origin),
                m.inline.map(|i| i.owner),
            )
        });
        if let Some((owner, origin, inline_owner)) = owner {
            if let Some(owner) = inline_owner {
                visit(v, owner)?;
            }
            visit(v, owner)?;
            if let Some(origin) = origin {
                visit(v, origin)?;
            }
        } else if let Some(edges) = { s.machines.borrow().references(target) } {
            for edge in edges {
                visit(v, edge)?;
            }
        } else if target != s.prototype {
            s.send(
                s.model,
                b"referencesOf:do:",
                &[Operand::object(target), Operand::entry(v)],
            )?;
        }
        if s.send(s.id.receiver, b"isBehavior:", &[Operand::object(target)])?
            .as_word()
            == Some(1)
        {
            let mut b = Bindings {
                execution: s,
                visitor: v,
            };
            s.send(
                s.id.receiver,
                b"bindingsIn:do:",
                &[
                    Operand::object(target),
                    Operand::entry(NativeEntry::new(binding, ptr::from_mut(&mut b).cast())),
                ],
            )?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn retire(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([dead]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let dead = dead.as_bytes().ok_or_else(invalid)?;
        let bytes = borrowed_bytes(dead.pointer, dead.length).ok_or_else(invalid)?;
        if !bytes.len().is_multiple_of(size_of::<usize>()) {
            return Err(invalid());
        }
        let dead: Vec<usize> = bytes
            .chunks_exact(size_of::<usize>())
            .map(|b| usize::from_ne_bytes(b.try_into().unwrap()))
            .collect();
        s.machines.borrow_mut().retire_objects(&dead);
        for address in bytes.chunks_exact(size_of::<usize>()) {
            let address = usize::from_ne_bytes(address.try_into().unwrap());
            let record = s.methods.borrow_mut().remove(&address);
            if let Some(record) = record {
                if let Some(ref registered) = record.resumable {
                    if registered.origin == record.object {
                        s.machines.borrow_mut().retire_method(registered)?;
                    }
                }
                s.entries
                    .borrow_mut()
                    .remove(&ptr::from_ref(&*record).addr());
                if let Some(image) = record.image {
                    s.retired_images.borrow_mut().push(image);
                }
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn checkpoint(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        if s.depth.get() != 0 || s.draining.replace(true) {
            return Ok(Operand::unit());
        }
        // Admission rejects a checkpoint called from marking/finalization. It
        // can itself collect; keep the drain fence up through that reentry.
        let admission = s.send(s.gc, b"prepareAllocation", &[]);
        if admission.is_ok() {
            s.machines.borrow_mut().checkpoint()?;
            let retired = core::mem::take(&mut *s.retired_images.borrow_mut());
            drop(retired);
        }
        s.draining.set(false);
        admission?;
        Ok(Operand::unit())
    })())
}

const NAMES: &[&[u8]] = &[
    b"rootsDo:",
    b"referencesOf:do:",
    b"retire:",
    b"fromNative:owner:",
    b"clone",
    b"invokeOn:",
    b"invokeOn:arguments:",
    b"methodFor:",
    b"addMethod:as:",
    b"checkpoint",
    b"allocate",
    b"size",
    b"at:",
    b"wordOf:",
    b"fromWord:",
    b"behaviorOf:",
    b"lookup",
    b"addMethod",
    b"prepareAllocation",
    b"isBehavior:",
    b"bindingsIn:do:",
    b"kind:",
    b"nativeResultRequiresWrapper",
    b"nativeInvalidRequest",
    b"nativeFailure",
];
unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| -> Result<Oop, Outcome> {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots_object = named(b"Roots")?;
        let gc = named(b"GC")?;
        let mut tokens = Vec::new();
        let mut keep = |value: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send_named(roots_object, b"retain:", &[Operand::object(value)])?
                    .as_word()
                    .ok_or_else(invalid)?,
            );
            Ok(value)
        };
        let mut names = HashMap::new();
        for &name in NAMES.iter().chain(machine::NAMES) {
            names.insert(name, keep(id.intern_name(name)?)?);
        }
        let object_behavior = object(id.send_named(
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(named(b"Object")?)],
        )?)?;
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
        let method_behavior = keep(object(id.send_named(
            object_behavior,
            b"delegated",
            &[],
        )?)?)?;
        let prototype = keep(object(id.send_named(
            method_behavior,
            b"allocate",
            &[Operand::word(0)],
        )?)?)?;
        let s = Box::into_raw(Box::new(Execution {
            machines: RefCell::new(machine::State::new()),
            requests: RefCell::new(Vec::new()),
            id,
            receiver,
            roots: roots_object,
            gc,
            model: named(b"ObjectModel")?,
            integer: named(b"Integer")?,
            word: named(b"Word")?,
            float: named(b"Float")?,
            error: named(b"Error")?,
            nil: named(b"nil")?,
            method_behavior,
            prototype,
            thread: std::thread::current().id(),
            names,
            methods: RefCell::new(HashMap::new()),
            entries: RefCell::new(HashMap::new()),
            scoped: RefCell::new(Vec::new()),
            depth: Cell::new(0),
            draining: Cell::new(false),
            retired_images: RefCell::new(Vec::new()),
        }));
        for (target, name, method) in [
            (behavior, b"rootsDo:".as_slice(), roots as Method),
            (behavior, b"referencesOf:do:", references),
            (behavior, b"retire:", retire),
            (behavior, b"checkpoint", checkpoint),
            (method_behavior, b"fromNative:owner:", from_native),
            (method_behavior, b"clone", identity),
            (method_behavior, b"invokeOn:", invoke),
            (method_behavior, b"invokeOn:arguments:", invoke_arguments),
            (object_behavior, b"methodFor:", method_for),
            (id.receiver, b"methodFor:", method_for),
            (id.receiver, b"addMethod:as:", install),
        ] {
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object((&(*s).names)[name]),
                    Operand::entry(NativeEntry::new(method, s.cast())),
                ],
            )?;
        }
        id.send_named(roots_object, b"register:", &[Operand::object(receiver)])?;
        id.send_named(gc, b"registerRetirement:", &[Operand::object(receiver)])?;
        for target in [method_behavior, id.receiver] {
            id.send_named(
                gc,
                b"trace:with:",
                &[Operand::object(target), Operand::object(receiver)],
            )?;
        }
        machine::install(&*s, namespace)?;
        for (name, value) in [
            (b"Execution".as_slice(), receiver),
            (b"Method", prototype),
            (b"MethodVt", method_behavior),
        ] {
            id.send_named(
                namespace,
                b"at:put:",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::object(value),
                ],
            )?;
        }
        for token in tokens {
            id.send_named(roots_object, b"release:", &[Operand::word(token)])?;
        }
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([namespace]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let namespace = object(*namespace)?;
        let id = *id;
        let current = id.send_named(
            namespace,
            b"at:",
            &[Operand::object(id.intern_name(b"Execution")?)],
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
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
