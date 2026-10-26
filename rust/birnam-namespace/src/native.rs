//! Ordered bindings, nullable lookup and source-global resolution through Oops.
use super::state::Environment;
use birnam_abi::{self as abi, Oop, id_protocol::{Literal, MethodSpec, Module, Protocol}};
use core::{cell::{Cell, RefCell}, ptr};
use std::{collections::HashMap, sync::atomic::{AtomicPtr, Ordering}};

struct Namespace {
    id: Protocol,
    receiver: Oop,
    registry: Oop,
    storage: RefCell<Environment>,
    active: RefCell<Vec<Oop>>,
    names: HashMap<&'static [u8], Oop>,
    lookup_method: Cell<Oop>,
    ready: Cell<bool>,
}
struct Scope<'a> { namespace: &'a Namespace, start: usize }
impl Scope<'_> {
    fn keep(&self, value: Oop) -> Oop { self.namespace.active.borrow_mut().push(value); value }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) { self.namespace.active.borrow_mut().truncate(self.start); }
}
impl Namespace {
    fn invalid(&self) -> Oop { self.id.invalid }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    unsafe fn key(&self, key: Oop) -> Result<Oop, Oop> {
        if unsafe { self.send(self.id.root, b"isSymbol:", &[key]) }? == abi::size(1).unwrap() {
            Ok(key)
        } else { Err(self.invalid()) }
    }
    fn scope(&self) -> Scope<'_> { Scope { namespace: self, start: self.active.borrow().len() } }
    unsafe fn perform(&self, op: Op, r: Oop, a: &[Oop]) -> Result<Oop, Oop> {
        let one = || if let [v] = a { Ok(*v) } else { Err(self.invalid()) };
        let nil = ptr::null_mut();
        unsafe { match op {
            Op::Find => { let key = self.key(one()?)?; Ok(self.storage.borrow().import(key)) },
            Op::Put => {
                let [key, value] = a else { return Err(self.invalid()); };
                let key = self.key(*key)?;
                Ok(self.storage.borrow_mut().export(key, *value))
            }
            Op::ValueAt => {
                let result = self.send(r, b"at:", &[one()?])?;
                if result.is_null() { self.send(r, b"at:", &[self.names[b"nil".as_slice()]]) }
                else { Ok(result) }
            }
            Op::ResolveGlobal => {
                let key = one()?;
                let scope = self.scope();
                let nil = scope.keep(self.send(r, b"at:", &[self.names[b"nil".as_slice()]])?);
                let globals = scope.keep(self.send(r, b"at:", &[self.names[b"Globals".as_slice()]])?);
                if !globals.is_null() {
                    let found = self.send(globals, b"at:", &[key])?;
                    if found != nil { return Ok(found); }
                }
                let fallback = self.send(r, b"at:", &[key])?;
                Ok(if fallback.is_null() { nil } else { fallback })
            }
            Op::Includes => {
                let found = self.send(r, b"at:", &[one()?])?;
                let key = if found.is_null() { b"false".as_slice() } else { b"true".as_slice() };
                self.send(self.receiver, b"at:", &[self.names[key]])
            }
            Op::Count => {
                if !a.is_empty() { return Err(self.invalid()); }
                abi::size(self.storage.borrow().count()).ok_or(self.invalid())
            }
            Op::NameAt => Ok(self.storage.borrow().name_at(abi::as_size(one()?).ok_or(self.invalid())?)),
            Op::Reverse => Ok(self.storage.borrow().exported_name_of(one()?)),
            Op::Canonical => Ok(abi::size(usize::from(one()? == self.lookup_method.get())).unwrap()),
            Op::Roots => {
                let visitor = one()?;
                let mut values = self.storage.borrow().roots();
                values.extend([self.receiver, self.registry, self.lookup_method.get()]);
                values.extend(self.names.values().copied());
                values.extend(self.active.borrow().iter().copied());
                for value in values { self.send(visitor, b"visit:", &[value])?; }
                Ok(nil)
            }
            Op::Bindings | Op::LiveBindings => {
                let visitor = one()?;
                let scope = self.scope();
                let snapshot = self.storage.borrow().bindings().to_vec();
                for binding in &snapshot { scope.keep(binding.name); scope.keep(binding.value); }
                for binding in snapshot {
                    if matches!(op, Op::LiveBindings) && binding.value.is_null() { continue; }
                    self.send(visitor, b"binding:value:", &[binding.name, binding.value])?;
                }
                Ok(nil)
            }
        }}
    }
}
static INSTANCE: AtomicPtr<Namespace> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
unsafe fn state<'a>(c: Oop) -> Result<&'a Namespace, Oop> {
    let raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        let api = API.load(Ordering::Acquire);
        if api.is_null() { std::process::abort(); }
        return Err(unsafe { (*api).invalid });
    }
    let s = unsafe { &*raw };
    if c == s.receiver { Ok(s) } else { Err(s.invalid()) }
}
macro_rules! operations { ($(($name:literal, $op:ident)),* $(,)?) => {
    #[derive(Clone, Copy)] enum Op { $($op),* }
    #[allow(non_snake_case)] mod entries {
        use super::*;
        $(pub unsafe extern "C" fn $op(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
            let s = match unsafe { state(c) } { Ok(s) => s, Err(e) => return abi::fault_reference(e) };
            let Some(a) = (unsafe { abi::borrowed_slice(a,n) }) else { return s.invalid(); };
            unsafe { s.perform(Op::$op,r,a) }.unwrap_or_else(abi::fault_reference)
        })*
    }
    const METHODS: &[MethodSpec] = &[$(MethodSpec { name: index($name), entry: Some(entries::$op) }),*];
}; }
macro_rules! literals { ($($name:literal),* $(,)?) => {
    const NAMES: &[&[u8]] = &[$($name),*];
    static LITERALS: &[Literal] = &[$(Literal::new($name)),*];
}; }
literals![b"objectBehavior", b"delegated", b"allocate", b"methodAt:", b"withContext:",
    b"addMethod", b"register:", b"isSymbol:", b"visit:", b"binding:value:",
    b"at:", b"at:put:", b"valueAt:", b"resolveGlobal:", b"includesKey:", b"count",
    b"nameAt:", b"nameOf:", b"rootsDo:", b"bindingsDo:", b"liveBindingsDo:",
    b"isCanonicalLookup:", b"nil", b"true", b"false", b"Globals", b"Namespace", b"Roots",
    b"retain:", b"release:"];
const fn index(name: &[u8]) -> usize {
    let mut i=0;
    while i<NAMES.len() {
        if NAMES[i].len()==name.len() {
            let mut j=0; while j<name.len() && NAMES[i][j]==name[j] { j+=1; }
            if j==name.len() { return i; }
        }
        i+=1;
    }
    panic!("missing literal")
}
operations![(b"at:", Find), (b"at:put:", Put), (b"valueAt:", ValueAt),
    (b"resolveGlobal:", ResolveGlobal), (b"includesKey:", Includes), (b"count", Count),
    (b"nameAt:", NameAt), (b"nameOf:", Reverse), (b"rootsDo:", Roots),
    (b"bindingsDo:", Bindings), (b"liveBindingsDo:", LiveBindings),
    (b"isCanonicalLookup:", Canonical)];
unsafe extern "C" fn initialize(p: *const Protocol, image: Oop, l: *const Oop, n: usize, a: *const Oop, argc: usize) -> Oop {
    API.store(p.cast_mut(),Ordering::Release);
    let id = unsafe { *p };
    let Some([registry]) = (unsafe { abi::borrowed_slice(a,argc) }) else { return id.invalid; };
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() { return if unsafe { (*old).ready.get() && (*old).registry == *registry } { unsafe { (*old).receiver } } else { id.invalid }; }
    let Some(symbols) = (unsafe { abi::borrowed_slice(l,n) }) else { return id.invalid; };
    if symbols.len()!=NAMES.len() { return id.invalid; }
    (|| unsafe {
        let names: HashMap<_,_> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r,n:&[u8],a:&[Oop]| id.send(r,names[n],a);
        let base = send(id.root,b"objectBehavior",&[])?;
        let behavior = send(base,b"delegated",&[])?;
        let receiver = send(behavior,b"allocate",&[abi::size(0).unwrap()])?;
        let token = send(*registry,b"retain:",&[receiver])?;
        let s = Box::new(Namespace { id,receiver,registry:*registry,storage:RefCell::new(Environment::new()),
            active:RefCell::new(Vec::new()),names,lookup_method:Cell::new(ptr::null_mut()),ready:Cell::new(false) });
        let s = Box::into_raw(s);
        INSTANCE.store(s,Ordering::Release);
        let s = &*s;
        let result = (|| {
            for spec in METHODS {
                let method = s.send(image,b"methodAt:",&[symbols[spec.name]])?;
                let method = s.send(method,b"withContext:",&[receiver])?;
                s.send(behavior,b"addMethod",&[symbols[spec.name],method])?;
                if spec.name==index(b"at:") { s.lookup_method.set(method); }
            }
            s.send(*registry,b"register:",&[receiver])?;
            for (name,value) in [(b"Namespace".as_slice(),receiver),(b"Roots",*registry)] {
                s.send(receiver,b"at:put:",&[s.names[name],value])?;
            }
            s.ready.set(true);
            Ok(receiver)
        })();
        s.send(*registry,b"release:",&[token])?;
        result
    })().unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v2: Module = Module { version: 2, byte_size:size_of::<Module>() as u32,
    literals:LITERALS.as_ptr(),literal_count:LITERALS.len(),methods:METHODS.as_ptr(),method_count:METHODS.len(),initialize:Some(initialize) };
