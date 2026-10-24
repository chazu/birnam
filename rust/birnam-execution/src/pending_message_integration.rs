//! Pending Oop migration source; not part of the compiled module.
//! Retained to migrate the remaining explicit features, not as a compatibility path.
//! Execution-owned callable objects. See the local protocol and migration ledger.
mod control;
mod names;
mod binding_proof;
mod discarded_clone;
mod entry;
mod entry_plan;
mod execution;
mod image;
mod machine;
mod native_tier;
mod native_abi;
mod native_message;
mod word_helper;
mod numeric;
mod numeric_operator;
mod numeric_predicate;
mod word_index;
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct PortableBinding {
    code: Oop,
    parameters: usize,
    captures: usize,
}

struct Callable {
    portable: Option<PortableBinding>,
    resumable: Option<machine::Registered>,
    inline: Option<recovery::Inline>,
    candidate: Option<Rc<native_tier::Candidate>>,
    execution: *const Execution,
    object: Oop,
    entry: NativeEntry,
    // Only direct observations of foreign native bindings use this entry.
    // Ordinary methodFor: captures and withImplementation: remain independent.
    direct_native_binding: bool,
    owner: Oop,
    implementation: Option<Oop>,
    image: Option<Rc<image::Image>>,
    native_signature: Option<birnam_abi::native_signature::Signature>,
    word_expression: Option<Rc<word_helper::Plan>>,
    word_helper_adapter: bool,
}
struct Execution {
    namespace: Oop,
    native_return_bytes: RefCell<Vec<u8>>,
    native_capture_bytes: RefCell<Vec<u8>>,
    machines: RefCell<machine::State>,
    binding_guard: birnam_abi::binding_guard::BindingGuard,
    numeric: RefCell<numeric::Registry>,
    requests: RefCell<Vec<recovery::Request>>,
    id: Protocol,
    receiver: Oop,
    roots: Oop,
    gc: Oop,
    model: Oop,
    word: Oop,
    nil: Oop,
    method_behavior: Oop,
    prototype: Oop,
    thread: std::thread::ThreadId,
    names: names::Names,
    methods: RefCell<HashMap<usize, Box<Callable>>>,
    entries: RefCell<HashMap<usize, Oop>>,
    scoped: RefCell<Vec<Oop>>,
    binding_visitors: RefCell<Vec<Oop>>,
    depth: Cell<usize>,
    preserve_native_failures: Cell<bool>,
    draining: Cell<bool>,
    retired_images: RefCell<Vec<Rc<image::Image>>>,
    retired_calls: RefCell<Vec<Box<Callable>>>,
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
unsafe fn visit(s: &Execution, visitor: Oop, value: Oop) -> Result<(), Outcome> {
    unsafe { s.send(visitor, b"visit:", &[Operand::object(value)]) }?;
    Ok(())
}
fn native_entry(value: Operand) -> Result<NativeEntry, Outcome> {
    value.as_entry().filter(|e| e.method.is_some()).ok_or_else(invalid)
}
unsafe extern "C" fn method_of_entry(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([entry]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        Ok(s.selected(native_entry(*entry)?)
            .map_or(Operand::unit(), Operand::object))
    })())
}
unsafe extern "C" fn implementation_of_entry(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([entry]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let entry = native_entry(*entry)?;
        let value = s.selected(entry).and_then(|method| {
            s.methods
                .borrow()
                .get(&method.addr())
                .and_then(|m| m.implementation)
        });
        Ok(value.map_or(Operand::unit(), Operand::object))
    })())
}
impl Execution {
    fn value_factory(&self,name:&[u8])->Result<Oop,Outcome> {
        unsafe {
            let key = match self.names.get(name) { Some(&key) => key, None => self.id.intern_name(name)? };
            object(self.send(self.namespace,b"at:",&[Operand::object(key)])?)
        }
    }
    fn integer_factory(&self)->Result<Oop,Outcome> {self.value_factory(b"Integer")}
    fn float_factory(&self)->Result<Oop,Outcome> {self.value_factory(b"Float")}
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
        if address != call as Method as usize && address != lookup as Method as usize && address != call_dnu as Method as usize {
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
    fn message_entry(&self, method: Oop, selector: Oop) -> Result<NativeEntry, Outcome> {
        {
            let methods = self.methods.borrow();
            let callable = methods.get(&method.addr()).ok_or_else(invalid)?;
            if callable.direct_native_binding {
                return Ok(callable.entry);
            }
        }
        let mut entry = self.entry(method, selector == self.names[b"lookup".as_slice()])?;
        if selector == self.names[b"doesNotUnderstand:".as_slice()] && self.methods.borrow().get(&method.addr()).is_some_and(|m| m.implementation.is_some()) {
            entry.method = Some(call_dnu);
        }
        Ok(entry)
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
            portable: None,
            resumable: None,
            inline: None,
            candidate: None,
            implementation: None,
            native_signature: None,
            word_expression: None,
            word_helper_adapter: false,
            execution: self,
            object: value,
            entry,
            direct_native_binding: false,
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
            let (entry, owner, image, resumable, inline, implementation, native_signature, word_expression, word_helper_adapter, portable) = {
                let values = self.methods.borrow();
                let value = values.get(&method.addr()).ok_or_else(invalid)?;
                (
                    value.entry,
                    value.owner,
                    value.image.clone(),
                    value.resumable.clone(),
                    value.inline,
                    value.implementation,
                    value.native_signature.clone(),
                    value.word_expression.clone(),
                    value.word_helper_adapter,
                    value.portable,
                )
            };
            let value = unsafe { self.make(entry, owner, image) }?;
            let mut methods = self.methods.borrow_mut();
            let captured = methods.get_mut(&object(value)?.addr()).unwrap();
            captured.resumable = resumable;
            captured.inline = inline;
            captured.implementation = implementation;
            captured.native_signature = native_signature;
            captured.word_expression = word_expression;
            captured.word_helper_adapter = word_helper_adapter;
            captured.portable = portable;
            return Ok(value);
        }
        let image = image::Image::retain(entry.method.ok_or_else(invalid)? as *const c_void)
            .ok_or_else(invalid)?;
        unsafe { self.make(entry, self.nil, Some(Rc::new(image))) }
    }
    unsafe fn language_result(&self, result: Result<Operand, Outcome>) -> Result<Operand, Outcome> {
        unsafe {
            if self.preserve_native_failures.get() && result.is_err() { return result; }
            let kind: &'static [u8] = match result {
                Ok(value) if value.kind() == native::OBJECT => return Ok(value),
                Ok(value) if value.kind() == native::UNIT => return Ok(Operand::object(self.nil)),
                Ok(value) if value.kind() == native::NATIVE_ENTRY => {
                    return self.capture(native_entry(value)?);
                }
                Ok(_) => b"nativeResultRequiresWrapper",
                Err(error) if error.status == native::INVALID_REQUEST => b"nativeInvalidRequest",
                Err(_) => b"nativeFailure",
            };
            self.send(self.error_factory()?, b"kind:", &[Operand::object(self.names[kind])])
        }
    }
    unsafe fn invoke(
        &self,
        method: Oop,
        receiver: Oop,
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        let signature=self.methods.borrow().get(&method.addr()).and_then(|m|m.native_signature.clone());
        if let Some(signature)=signature {
            let result = unsafe { native_abi::incoming(self,method,receiver,args,&signature) };
            if signature.observes_failure() {
                if let Err(error) = result { unsafe { native_message::notify_failure(self, receiver, error) }; }
            }
            return result;
        }
        unsafe {self.invoke_body(method,receiver,args)}
    }
    unsafe fn invoke_body(&self,method:Oop,receiver:Oop,args:&[Operand])->Result<Operand,Outcome> {
        if self.machines.borrow().image_pending(receiver) {
            return Err(invalid());
        }
        // Native nullable Objects become language nil at the source boundary.
        // Captured native implementations retain their original carrier contract.
        let normalized;
        let args = if self.methods.borrow().get(&method.addr()).is_some_and(|m| m.implementation.is_some()) {
            normalized = args.iter().map(|a| if a.as_object().is_some_and(|o| o.is_null()) { Operand::object(self.nil) } else { *a }).collect::<Vec<_>>();
            normalized.as_slice()
        } else { args };
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
        if let Some(result) = unsafe {word_helper::try_result(self, method, receiver, args)}? {
            return Ok(result);
        }
        unsafe {
            entry.method.ok_or_else(invalid)?(entry.context, receiver, args.as_ptr(), args.len())
        }
        .into_result()
    }
}
impl Execution {
    // Optional admission queries must not enter doesNotUnderstand when absent:
    // authored error construction can itself need the operation being admitted.
    // Resolve once through Id and invoke that selected call under Execution's
    // ordinary code/input roots. No entry is cached for a future query, and an
    // entered UNBOUND failure is propagated rather than mistaken for absence.
    unsafe fn optional_query(&self, receiver: Oop, name: &[u8], args: &[Operand]) -> Result<Operand, Outcome> {
        let roots = self.scope(&[receiver]);
        for arg in args { if let Some(value) = arg.as_object() { roots.keep(value); } }
        let Some(entry) = (unsafe { machine::lookup_target(self, receiver, self.names[name]) })? else {
            return Ok(Operand::unit());
        };
        if let Some(method) = self.selected(entry) { roots.keep(method); }
        unsafe { entry.method.ok_or_else(invalid)?(entry.context, receiver, args.as_ptr(), args.len()) }.into_result()
    }
    unsafe fn error_factory(&self) -> Result<Oop, Outcome> {
        unsafe { object(self.id.send_named(self.namespace, b"at:", &[Operand::object(self.id.intern_name(b"Error")?)])?) }
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
// Id's DNU convention includes the original native operands after the selector.
// Source handlers receive the declared selector; extra Object operands remain
// rooted while the handler runs, without decoding Word/Bytes/Entry payloads.
unsafe extern "C" fn call_dnu(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let callable = &*c.cast::<Callable>();
        let s = state(callable.execution.cast_mut().cast())?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        let selector = args.first().ok_or_else(invalid)?.as_object().filter(|o| !o.is_null()).unwrap_or(s.nil);
        let roots = s.scope(&[callable.object, r]);
        for arg in args { if let Some(value) = arg.as_object() { roots.keep(value); } }
        s.invoke(callable.object, r, &[Operand::object(selector)])
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
            native_entry(result)?;
            return Ok(result);
        }
        let selected = object(result)?;
        if selected == s.nil {
            return Ok(Operand::unit());
        }
        // No send, allocation or shared lookup slot between the owned result
        // and its exact entry. Nested lookups cannot overwrite this selection.
        let selector = borrowed_slice(a, n).ok_or_else(invalid)?.first().and_then(|v| v.as_object()).unwrap_or(ptr::null_mut());
        Ok(Operand::entry(s.message_entry(selected, selector)?))
    })())
}
unsafe extern "C" fn from_native(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([entry, owner]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        s.make(native_entry(*entry)?, object(*owner)?, None)
    })())
}
unsafe extern "C" fn from_native_implementation(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s=state(c)?;
        let Some([entry,owner,implementation])=borrowed_slice(a,n) else { return Err(invalid()); };
        let implementation=object(*implementation)?;
        let roots=s.scope(&[implementation,object(*owner)?]);
        let method=s.make(native_entry(*entry)?,object(*owner)?,None)?;
        let value=object(method)?;
        roots.keep(value);
        let signature=native_abi::implementation_signature(s,implementation)?;
        let mut methods=s.methods.borrow_mut();let record=methods.get_mut(&value.addr()).ok_or_else(invalid)?;
        record.implementation=Some(implementation);record.native_signature=signature;
        Ok(method)
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
            .send(s.integer_factory()?, b"wordOf:", &[Operand::object(size)])?
            .as_word()
            .ok_or_else(invalid)? as i64;
        let count = usize::try_from(size).map_err(|_| invalid())?;
        let mut args = Vec::new();
        for index in 0..count {
            let index = object(s.send(s.integer_factory()?, b"fromWord:", &[Operand::word(index)])?)?;
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
        s.capture(native_entry(selected)?)
    })())
}
// Direct observation is distinct from lookup/capture: it neither follows
// delegation nor substitutes a new wrapper for an already-owned Method.
unsafe extern "C" fn direct_method(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s=state(c)?;
        let [selector,behavior]=borrowed_slice(a,n).ok_or_else(invalid)? else{return Err(invalid());};
        let (selector,behavior)=(object(*selector)?,object(*behavior)?);
        let roots=s.scope(&[r,selector,behavior]);
        let raw=s.id.send_named(s.id.receiver,b"bindingOf:in:",&[Operand::object(selector),Operand::object(behavior)])?;
        if raw.kind()==native::UNIT{return Ok(Operand::object(s.nil));}
        let entry=native_entry(raw)?;
        if let Some(method)=s.selected(entry){return Ok(Operand::object(method));}
        let value=s.capture(entry)?;let method=object(value)?;roots.keep(method);
        s.methods.borrow_mut().get_mut(&method.addr()).ok_or_else(invalid)?.direct_native_binding=true;
        Ok(value)
    })())
}
unsafe extern "C" fn binding_matches(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s=state(c)?;
        let [behavior,selector]=borrowed_slice(a,n).ok_or_else(invalid)? else{return Err(invalid());};
        let (behavior,selector)=(object(*behavior)?,object(*selector)?);
        let _roots=s.scope(&[r,behavior,selector]);
        let expected=s.message_entry(r,selector)?;
        let actual=s.id.send_named(s.id.receiver,b"bindingOf:in:",&[Operand::object(selector),Operand::object(behavior)])?;
        s.id.send_named(s.namespace,b"at:",&[Operand::object(s.id.intern_name(if actual.as_entry()==Some(expected){b"true"}else{b"false"})?)])
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
        let entry = s.message_entry(method, selector)?;
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
        let v = object(*v)?;
        let mut values = vec![
            s.receiver,
            s.roots,
            s.gc,
            s.model,
            s.word,
            s.nil,
            s.method_behavior,
            s.prototype,
        ];
        values.extend(s.names.values().copied());
        values.push(s.namespace);
        values.extend(s.machines.borrow().roots());
        values.extend(s.scoped.borrow().iter().copied());
        for value in values {
            visit(s, v, value)?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn binding(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([_, value, _]) = borrowed_slice(a, n) else { return Err(invalid()); };
        let visitor = s.binding_visitors.borrow().last().copied().ok_or_else(invalid)?;
        if let Some(entry) = value.as_entry() {
            if let Some(method) = s.selected(entry) {
                visit(s, visitor, method)?;
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
        let v = object(*v)?;
        // Copy the owned edge before visiting: a callback may reenter this owner.
        let candidate = s
            .methods
            .borrow()
            .get(&target.addr())
            .and_then(|m| m.candidate.clone());
        if let Some(candidate) = candidate {
            for &method in candidate.references() {
                visit(s, v, method)?;
            }
        }
        let owner = s.methods.borrow().get(&target.addr()).map(|m| {
            (
                m.owner,
                m.implementation,
                m.resumable.as_ref().map(|r| r.origin),
                m.inline,
                m.portable.map(|p| p.code),
            )
        });
        if let Some((owner, implementation, origin, inline_owner, code)) = owner {
            if let Some(code) = code { visit(s, v, code)?; }
            if let Some(value) = implementation {
                visit(s, v, value)?;
            }
            if let Some(inline) = inline_owner {
                visit(s, v, inline.owner)?;
                match inline.body {
                    recovery::Body::DiscardedClone(plan) => {
                        visit(s, v, plan.selector)?;
                        visit(s, v, plan.namespace)?;
                    }
                    recovery::Body::NumericPredicate(plan) => {
                        visit(s, v, plan.first)?;
                        visit(s, v, plan.then)?;
                        visit(s, v, plan.namespace)?;
                    }
                    recovery::Body::Native(_) => (),
                }
            }
            visit(s, v, owner)?;
            if let Some(origin) = origin {
                visit(s, v, origin)?;
            }
        } else if s.machines.borrow().storage.family_of(target).is_some() {
            let precise =
                s.id.send_named(s.gc, b"precisePayloads", &[])?
                    .as_word()
                    .ok_or_else(invalid)?
                    != 0;
            if !precise {
                let count = s
                    .machines
                    .borrow()
                    .storage
                    .payload_size(target)
                    .ok_or_else(invalid)?
                    / size_of::<usize>();
                for i in 0..count {
                    let bits = s
                        .machines
                        .borrow()
                        .storage
                        .read_payload_word(target, i)
                        .ok_or_else(invalid)?;
                    let edge =
                        s.id.send_named(s.id.receiver, b"allocationAt:", &[Operand::word(bits)])?;
                    if let Some(edge) = edge.as_object() {
                        visit(s, v, edge)?;
                    }
                }
            }
            // Required frame/continuation roots survive in both modes. Copy after
            // candidate sends, since those sends may mutate this owner's state.
            let edges = { s.machines.borrow().references(target).ok_or_else(invalid)? };
            for edge in edges {
                visit(s, v, edge)?;
            }
        } else if target != s.prototype {
            s.send(
                s.model,
                b"referencesOf:do:",
                &[Operand::object(target), Operand::object(v)],
            )?;
        }
        if s.send(s.id.receiver, b"isBehavior:", &[Operand::object(target)])?
            .as_word()
            == Some(1)
        {
            let _roots = s.scope(&[target, v]);
            s.binding_visitors.borrow_mut().push(v);
            let result = s.send(
                s.id.receiver, b"bindingsIn:do:",
                &[Operand::object(target), Operand::object(s.receiver)],
            );
            s.binding_visitors.borrow_mut().pop();
            result?;
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
            if let Some(mut record) = record {
                let registered_origin = record
                    .resumable
                    .as_ref()
                    .is_some_and(|r| r.origin == record.object);
                if let Some(ref registered) = record.resumable {
                    if registered.origin == record.object {
                        s.machines.borrow_mut().retire_method(registered)?;
                    }
                }
                s.entries
                    .borrow_mut()
                    .remove(&ptr::from_ref(&*record).addr());
                if let Some(image) = record.image.take() {
                    s.retired_images.borrow_mut().push(image);
                }
                // Its function/context key remains in the retiring engine until
                // checkpoint. Keep the native allocation from being reused as a
                // new callable context; these dead language edges are not roots.
                if registered_origin {
                    s.retired_calls.borrow_mut().push(record);
                }
            }
        }
        Ok(Operand::unit())
    })())
}
// Owner-local observation: TCC may drain detached native mappings only after
// the engine checkpoint and outside every entered Method/driver extent.
unsafe extern "C" fn retirement_ready(
    c: *mut c_void,
    _: Oop,
    _: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        Ok(Operand::word(usize::from(
            s.depth.get() == 0 && !s.draining.get(),
        )))
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
        let admission = (|| {
            s.send(s.gc, b"prepareAllocation", &[])?;
            s.machines.borrow_mut().checkpoint()?;
            s.retired_calls.borrow_mut().retain(|call| {
                s.machines.borrow().generation_pending(
                    call.resumable
                        .as_ref()
                        .unwrap()
                        .descriptor
                        .identity()
                        .generation,
                )
            });
            let retired = core::mem::take(&mut *s.retired_images.borrow_mut());
            drop(retired);
            Ok::<_, Outcome>(())
        })();
        s.draining.set(false);
        admission?;
        Ok(Operand::unit())
    })())
}

const NAMES: &[&[u8]] = &[
    b"CodeObject",
    b"Word",
    b"value:",
    b"value:value:",
    b"value:value:value:",

    b"Bytes",
    b"Codegen",
    b"Vector",
    b"add:",
    b"bits",
    b"bytes",
    b"canonicalBytesOf:",
    b"fromBytes:",
    b"materializeSynchronous:captures:",
    b"nativeSignature",
    b"withNativeSignature:",

    b"nativeBindingsMatch:",
    b"nativeBindingQuerySemantics:for:",
    b"intern:",
    b"symbolNamed:",
    b"optionalWordResultOf:selector:argument:resultFactory:",
    b"objectResultOf:selector:wordArgument:",
    b"discardResultOf:selector:wordArgument:",
    b"objectResultOf:selector:byteArgument:",
    b"bytesResultOf:selector:resultFactory:",
    b"wordResultOf:selector:",
    b"tryPrimitiveExpressionAtEpoch:",
    b"block:withNativeSignature:",
    b"failNative:detail:",
    b"bytesResultOf:selector:",
    b"numericOperation:left:right:",
    b"commitNumericPredicate:",
    b"visit:",
    b"binding:value:weak:",
    b"rootsDo:",
    b"referencesOf:do:",
    b"retire:",
    b"methodOfEntry:",
    b"implementationOfEntry:",
    b"fromNative:owner:",
    b"admitCodeObject:",
    b"fromNative:owner:implementation:",
    b"clone",
    b"invokeOn:",
    b"invokeOn:arguments:",
    b"methodFor:",
    b"directMethodFor:in:",
    b"bindingMatchesIn:as:",
    b"withWordExpression:bindings:parameters:",
    b"withWordHelperAdapter",
    b"addMethod:as:",
    b"checkpoint",
    b"retirementReady",
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
            namespace,
            native_return_bytes: RefCell::new(Vec::new()),
            native_capture_bytes: RefCell::new(Vec::new()),
            machines: RefCell::new(machine::State::new()),
            binding_guard: birnam_abi::binding_guard::BindingGuard::discover(id,
                birnam_abi::binding_guard::SERVICES.map(|name| names[name]),
                names[b"nativeBindingQuerySemantics:for:".as_slice()]),
            numeric: RefCell::new(numeric::Registry::default()),
            requests: RefCell::new(Vec::new()),
            id,
            receiver,
            roots: roots_object,
            gc,
            model: named(b"ObjectModel")?,
            word: named(b"Word")?,
            nil: named(b"nil")?,
            method_behavior,
            prototype,
            thread: std::thread::current().id(),
            names: names::Names::new(names),
            methods: RefCell::new(HashMap::new()),
            entries: RefCell::new(HashMap::new()),
            scoped: RefCell::new(Vec::new()),
            binding_visitors: RefCell::new(Vec::new()),
            depth: Cell::new(0),
            preserve_native_failures: Cell::new(false),
            draining: Cell::new(false),
            retired_images: RefCell::new(Vec::new()),
            retired_calls: RefCell::new(Vec::new()),
        }));
        for (target, name, method) in [
            (behavior, b"rootsDo:".as_slice(), roots as Method),
            (behavior, b"referencesOf:do:", references),
            (behavior, b"implementationOfEntry:", implementation_of_entry),
            (behavior, b"methodOfEntry:", method_of_entry),
            (behavior, b"binding:value:weak:", binding),
            (behavior, b"optionalWordResultOf:selector:argument:resultFactory:", native_abi::optional_word_argument),
            (behavior, b"objectResultOf:selector:wordArgument:", native_abi::object_word_argument),
            (behavior, b"discardResultOf:selector:wordArgument:", native_abi::discard_word_argument),
            (behavior, b"objectResultOf:selector:byteArgument:", native_abi::object_byte_argument),
            (behavior, b"bytesResultOf:selector:resultFactory:", native_abi::byte_query_factory),
            (behavior, b"block:withNativeSignature:", native_abi::block_with_signature),
            (behavior, b"failNative:detail:", native_message::fail),
            (behavior, b"wordResultOf:selector:", native_abi::word_result),
            (behavior, b"bytesResultOf:selector:", native_abi::bytes_result),
            (behavior, b"numericOperation:left:right:", numeric::evaluate),
            (behavior, b"commitNumericPredicate:", numeric_predicate::commit),
            (behavior, b"retire:", retire),
            (behavior, b"checkpoint", checkpoint),
            (behavior, b"retirementReady", retirement_ready),
            (method_behavior, b"fromNative:owner:", from_native),
            (method_behavior, b"admitCodeObject:", machine::admit_code_object),
            (method_behavior,b"fromNative:owner:implementation:",from_native_implementation),
            (method_behavior,b"withWordExpression:bindings:parameters:",word_helper::attach),
            (method_behavior,b"withWordHelperAdapter",word_helper::adapter),
            (method_behavior, b"clone", identity),
            (behavior, b"directMethodFor:in:", direct_method),
            (method_behavior, b"bindingMatchesIn:as:", binding_matches),
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
