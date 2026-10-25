//! Source compilation through current Lang, NativeBackend and TCC owners.
//! Explicit source compilation and reflected Methods reuse the retained lowering.
//! Artifact publication and default compiler orchestration remain pending.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{
        ContinuationMethodSpec as MethodSpec, ContinuationModule as Module, ContinuationProtocol,
        Control, Literal, Protocol,
    },
};
use birnam_codegen::OwnedCst;
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    control: Control,
    reflect: Oop,
    implementation_entry: Oop,
    ns: Oop,
    receiver: Oop,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
    thread: std::thread::ThreadId,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
struct Constructing;
impl Drop for Constructing {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
fn invalid() -> Oop {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        std::process::abort();
    }
    unsafe { abi::object_reference((*p).invalid) }
}
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let s = unsafe { &*p };
    if c != s.ns || s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl Scope<'_> {
    fn keep(&self, v: Oop) -> Oop {
        self.s.scoped.borrow_mut().push(v);
        v
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    fn scope(&self, values: &[Oop]) -> Scope<'_> {
        let start = self.scoped.borrow().len();
        self.scoped.borrow_mut().extend_from_slice(values);
        Scope { s: self, start }
    }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let v = self.send(self.ns, b"at:", &[self.names[n]])?;
            if v.is_null() { Err(invalid()) } else { Ok(v) }
        }
    }
    unsafe fn bytes(&self, v: Oop) -> Result<Vec<u8>, Oop> {
        unsafe {
            let q = self.scope(&[v]);
            let buffer = q.keep(self.send(v, b"bytes", &[])?);
            self.id.read_bytes(
                buffer,
                self.names[b"sizeWord".as_slice()],
                self.names[b"atWord:".as_slice()],
            )
        }
    }
    unsafe fn text(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let factory = q.keep(self.named(b"String")?);
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let data = self.send(self.id.root, b"sequence", &values)?;
            self.send(factory, b"fromBytes:", &[data])
        }
    }
    unsafe fn error(&self, message: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let message = q.keep(self.text(message)?);
            self.send(
                self.named(b"Error")?,
                b"kind:message:",
                &[self.names[b"compileError".as_slice()], message],
            )
        }
    }
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![s.ns, s.receiver, s.reflect, s.implementation_entry];
        values.extend_from_slice(&s.scoped.borrow());
        for value in values {
            s.send(*v, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}

impl State {
    unsafe fn is_error(&self, value: Oop) -> Result<bool, Oop> {
        unsafe {
            let error = self.send(self.ns, b"at:", &[self.names[b"Error".as_slice()]])?;
            if error.is_null() {
                return Ok(false);
            }
            Ok(self.send(value, b"isError", &[])? == self.named(b"true")?)
        }
    }
    unsafe fn index(&self, value: Oop) -> Result<usize, Oop> {
        unsafe {
            if let Some(v) = abi::as_size(value) {
                return Ok(v);
            }
            let q = self.scope(&[value]);
            let word = q.keep(self.send(self.named(b"Integer")?, b"wordOf:", &[value])?);
            if let Some(v) = abi::as_size(word) {
                return Ok(v);
            }
            let lo = abi::as_size(self.send(word, b"low32", &[])?).ok_or_else(invalid)?;
            let hi = abi::as_size(self.send(word, b"high32", &[])?).ok_or_else(invalid)?;
            if lo > u32::MAX as usize || hi > u32::MAX as usize {
                return Err(invalid());
            }
            usize::try_from((lo as u64) | ((hi as u64) << 32)).map_err(|_| invalid())
        }
    }
    unsafe fn project(
        &self,
        node: Oop,
        length: usize,
        depth: usize,
        budget: &mut usize,
    ) -> Result<OwnedCst, Oop> {
        unsafe {
            if depth > 1024 || *budget == 0 {
                return Err(invalid());
            }
            *budget -= 1;
            let q = self.scope(&[node]);
            let rule = q.keep(self.send(node, b"ruleName", &[])?);
            let rule = self.bytes(rule)?;
            let start = q.keep(self.send(node, b"start", &[])?);
            let start = self.index(start)?;
            let end = q.keep(self.send(node, b"end", &[])?);
            let end = self.index(end)?;
            if start > end || end > length {
                return Err(invalid());
            }
            let count = q.keep(self.send(node, b"childCount", &[])?);
            let count = self.index(count)?;
            if count > *budget {
                return Err(invalid());
            }
            let mut children = Vec::new();
            for i in 0..count {
                let ix = q.keep(self.send(
                    self.named(b"Integer")?,
                    b"fromWord:",
                    &[abi::size(i).ok_or_else(invalid)?],
                )?);
                let child = q.keep(self.send(node, b"childAt:", &[ix])?);
                children.push(self.project(child, length, depth + 1, budget)?);
            }
            Ok(OwnedCst::new(&rule, start, end, children))
        }
    }
}
unsafe fn compile_source(c: Oop, a: *const Oop, n: usize, resumable: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*source]);
        let bytes = s.bytes(*source)?;
        let tree = q.keep(s.send(s.named(b"Lang")?, b"parse:", &[*source])?);
        if tree.is_null() || tree == s.named(b"nil")? {
            return s.error(b"source parse failed");
        }
        if s.is_error(tree)? {
            return Ok(tree);
        }
        let tree = s.project(tree, bytes.len(), 0, &mut 1_000_000)?;
        let program = match birnam_codegen::lower_program(&tree, &bytes) {
            Ok(v) => v,
            Err(e) => return s.error(format!("source lowering: {e:?}").as_bytes()),
        };
        let block = birnam_codegen::Block {
            params: Vec::new(),
            body: program.body,
        };
        let portable = match birnam_codegen::portable::encode_block(&block, &[]) {
            Ok(v) => v,
            Err(e) => return s.error(format!("portable lowering: {e:?}").as_bytes()),
        };
        let encoded = q.keep(s.text(&portable)?);
        let native = q.keep(if resumable {
            let source_snapshot = q.keep(s.text(&bytes)?);
            s.send(
                s.named(b"NativeBackend")?,
                b"emitResumable:source:",
                &[encoded, source_snapshot],
            )?
        } else {
            s.send(s.named(b"NativeBackend")?, b"emitSynchronous:", &[encoded])?
        });
        if s.is_error(native)? {
            return Ok(native);
        }
        let unit = q.keep(s.send(s.named(b"TCC")?, b"compile:", &[native])?);
        if s.is_error(unit)? {
            return Ok(unit);
        }
        s.send(unit, b"initialize", &[])
    })()
    .unwrap_or_else(abi::fault_reference)
}

unsafe extern "C" fn compile(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { compile_source(c, a, n, false) }
}
unsafe extern "C" fn compile_resumable(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { compile_source(c, a, n, true) }
}
// A reflected Method explicitly sends its implementation Block the receiver
// followed by the message arguments, preserving the original answer destination.
unsafe extern "C" fn enter_implementation(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = state((*p).ns)?;
        let [k, args @ ..] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[c, r, *k]);
        let implementation = q.keep(s.send(c, b"at:", &[abi::size(0).unwrap()])?);
        let selector = q.keep(s.send(c, b"at:", &[abi::size(1).unwrap()])?);
        let arity = s.send(c, b"at:", &[abi::size(2).unwrap()])?;
        if abi::as_size(arity) != Some(args.len()) {
            return Err(invalid());
        }
        let mut values = vec![implementation, selector, *k, r];
        values.extend_from_slice(args);
        let work = (s.control)(
            s.id.root,
            s.names[b"prepareSend".as_slice()],
            values.as_ptr(),
            values.len(),
        );
        if abi::is_fault(work) {
            Err(abi::object_reference(work))
        } else {
            Ok(work)
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn construct_method(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [arity, implementation] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*arity, *implementation]);
        let arity = s.index(*arity)?;
        if arity > 16 {
            return Err(invalid());
        }
        let bytes = b"value:".repeat(arity + 1);
        let words: Vec<_> = bytes
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let name = q.keep(s.send(s.id.root, b"nameWithBytes", &words)?);
        let selector = q.keep(s.id.send(name, s.id.intern, &[])?);
        let context = q.keep(s.send(
            s.id.root,
            b"sequence",
            &[*implementation, selector, abi::size(arity).unwrap()],
        )?);
        s.send(s.implementation_entry, b"withContext:", &[context])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn define_method(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [selector, target, implementation] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[r, *selector, *target, *implementation]);
        let bytes = s.id.read_bytes(
            *selector,
            s.names[b"sizeWord".as_slice()],
            s.names[b"atWord:".as_slice()],
        )?;
        let first = *bytes.first().ok_or_else(invalid)?;
        let colons = bytes.iter().filter(|&&b| b == b':').count();
        let arity = if colons > 0 {
            colons
        } else {
            usize::from(!first.is_ascii_alphabetic() && first != b'_')
        };
        if arity > 16 {
            return Err(invalid());
        }
        let arity = q.keep(s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(arity).unwrap()],
        )?);
        let method = q.keep(s.send(
            r,
            b"methodForArity:implementation:",
            &[arity, *implementation],
        )?);
        if s.is_error(method)? {
            return Ok(method);
        }
        let behavior = q.keep(s.send(s.id.root, b"ensurePrivateVtableFor:", &[*target])?);
        let result = s.send(behavior, b"addMethod:as:", &[method, *selector])?;
        Ok(if result == method { *target } else { result })
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn vtable_of(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != 1 {
            return Err(invalid());
        }
        s.send(s.id.root, b"behaviorOf:", args)
    })()
    .unwrap_or_else(abi::fault_reference)
}
const OWNER_NAME: &[u8] = b"Codegen";
const NAMES: &[&[u8]] = &[
    b"Block",
    b"Codegen",
    b"Error",
    b"Integer",
    b"Lang",
    b"NativeBackend",
    b"ObjectVt",
    b"PegCstVt",
    b"Roots",
    b"String",
    b"TCC",
    b"addMethod",
    b"allocate",
    b"at:",
    b"at:put:",
    b"atWord:",
    b"behaviorOf:",
    b"bytes",
    b"childAt:",
    b"childCount",
    b"compile:",
    b"compileError",
    b"delegated",
    b"emitSynchronous:",
    b"end",
    b"fromBytes:",
    b"fromMethod:captureOperands:",
    b"fromWord:",
    b"high32",
    b"initialize",
    b"kind:message:",
    b"low32",
    b"methodAt:",
    b"nil",
    b"parse:",
    b"register:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"ruleName",
    b"sequence",
    b"sizeWord",
    b"start",
    b"visit:",
    b"withContext:",
    b"wordOf:",
    b"TCCUnitVt",
    b"isError",
    b"true",
    b"compileSynchronous:",
    b"compileResumable:",
    b"emitResumable:source:",
    b"Reflect",
    b"ReflectVt",
    b"defineMethod:on:as:",
    b"methodForArity:implementation:",
    b"enterImplementation",
    b"prepareSend",
    b"nameWithBytes",
    b"ensurePrivateVtableFor:",
    b"addMethod:as:",
    b"vtableOf:",
];
static LITERALS: &[Literal] = &[
    Literal::new(b"Block"),
    Literal::new(b"Codegen"),
    Literal::new(b"Error"),
    Literal::new(b"Integer"),
    Literal::new(b"Lang"),
    Literal::new(b"NativeBackend"),
    Literal::new(b"ObjectVt"),
    Literal::new(b"PegCstVt"),
    Literal::new(b"Roots"),
    Literal::new(b"String"),
    Literal::new(b"TCC"),
    Literal::new(b"addMethod"),
    Literal::new(b"allocate"),
    Literal::new(b"at:"),
    Literal::new(b"at:put:"),
    Literal::new(b"atWord:"),
    Literal::new(b"behaviorOf:"),
    Literal::new(b"bytes"),
    Literal::new(b"childAt:"),
    Literal::new(b"childCount"),
    Literal::new(b"compile:"),
    Literal::new(b"compileError"),
    Literal::new(b"delegated"),
    Literal::new(b"emitSynchronous:"),
    Literal::new(b"end"),
    Literal::new(b"fromBytes:"),
    Literal::new(b"fromMethod:captureOperands:"),
    Literal::new(b"fromWord:"),
    Literal::new(b"high32"),
    Literal::new(b"initialize"),
    Literal::new(b"kind:message:"),
    Literal::new(b"low32"),
    Literal::new(b"methodAt:"),
    Literal::new(b"nil"),
    Literal::new(b"parse:"),
    Literal::new(b"register:"),
    Literal::new(b"release:"),
    Literal::new(b"retain:"),
    Literal::new(b"rootsDo:"),
    Literal::new(b"ruleName"),
    Literal::new(b"sequence"),
    Literal::new(b"sizeWord"),
    Literal::new(b"start"),
    Literal::new(b"visit:"),
    Literal::new(b"withContext:"),
    Literal::new(b"wordOf:"),
    Literal::new(b"TCCUnitVt"),
    Literal::new(b"isError"),
    Literal::new(b"true"),
    Literal::new(b"compileSynchronous:"),
    Literal::new(b"compileResumable:"),
    Literal::new(b"emitResumable:source:"),
    Literal::new(b"Reflect"),
    Literal::new(b"ReflectVt"),
    Literal::new(b"defineMethod:on:as:"),
    Literal::new(b"methodForArity:implementation:"),
    Literal::new(b"enterImplementation"),
    Literal::new(b"prepareSend"),
    Literal::new(b"nameWithBytes"),
    Literal::new(b"ensurePrivateVtableFor:"),
    Literal::new(b"addMethod:as:"),
    Literal::new(b"vtableOf:"),
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 49,
        entry: Some(compile),
        convention: 0,
    },
    MethodSpec {
        name: 38,
        entry: Some(roots),
        convention: 0,
    },
    MethodSpec {
        name: 50,
        entry: Some(compile_resumable),
        convention: 0,
    },
    MethodSpec {
        name: 54,
        entry: Some(define_method),
        convention: 0,
    },
    MethodSpec {
        name: 55,
        entry: Some(construct_method),
        convention: 0,
    },
    MethodSpec {
        name: 56,
        entry: Some(enter_implementation),
        convention: 1,
    },
    MethodSpec {
        name: 61,
        entry: Some(vtable_of),
        convention: 0,
    },
];
unsafe extern "C" fn initialize(
    p: *const ContinuationProtocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(
        unsafe { &raw mut (*p.cast_mut()).common },
        Ordering::Release,
    );
    let id = unsafe { (*p).common };
    let control = unsafe { (*p).control };
    let Some([ns]) = (unsafe { borrowed_slice(a, count) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if n != NAMES.len() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
            s.receiver
        } else {
            id.invalid
        };
    }
    if BUSY.swap(true, Ordering::AcqRel) {
        return id.invalid;
    }
    let _busy = Constructing;
    (|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| send(*ns, b"at:", &[names[n]]);
        let roots = named(b"Roots")?;
        let vt = send(named(b"ObjectVt")?, b"delegated", &[])?;
        let token = send(roots, b"retain:", &[vt])?;
        let receiver = send(vt, b"allocate", &[abi::size(0).unwrap()])?;
        let receiver_token = send(roots, b"retain:", &[receiver])?;
        let reflect_vt = send(named(b"ObjectVt")?, b"delegated", &[])?;
        let reflect_vt_token = send(roots, b"retain:", &[reflect_vt])?;
        let reflect = send(reflect_vt, b"allocate", &[abi::size(0).unwrap()])?;
        let reflect_token = send(roots, b"retain:", &[reflect])?;
        let implementation_entry = send(
            image,
            b"methodAt:",
            &[names[b"enterImplementation".as_slice()]],
        )?;
        let s = Box::leak(Box::new(State {
            id,
            control,
            reflect,
            implementation_entry,
            ns: *ns,
            receiver,
            names: names.clone(),
            scoped: RefCell::new(Vec::new()),
            ready: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for spec in METHODS {
            let name = NAMES[spec.name];
            if name == b"enterImplementation" {
                continue;
            }
            let target = if matches!(
                name,
                b"defineMethod:on:as:" | b"methodForArity:implementation:" | b"vtableOf:"
            ) {
                reflect_vt
            } else {
                vt
            };
            let method = send(image, b"methodAt:", &[symbols[spec.name]])?;
            let method = send(method, b"withContext:", &[*ns])?;
            send(target, b"addMethod", &[symbols[spec.name], method])?;
        }
        send(roots, b"register:", &[receiver])?;
        send(*ns, b"at:put:", &[names[OWNER_NAME], receiver])?;
        send(*ns, b"at:put:", &[names[b"Reflect".as_slice()], reflect])?;
        send(
            *ns,
            b"at:put:",
            &[names[b"ReflectVt".as_slice()], reflect_vt],
        )?;
        send(roots, b"release:", &[reflect_token])?;
        send(roots, b"release:", &[reflect_vt_token])?;
        send(roots, b"release:", &[token])?;
        send(roots, b"release:", &[receiver_token])?;
        s.ready.set(true);
        Ok(receiver)
    })()
    .unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v3: Module = Module {
    version: 3,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
