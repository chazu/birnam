//! Source compilation through current Lang, NativeBackend and TCC owners.
//! Explicit source compilation and reflected Methods reuse the retained lowering.
//! Source records and artifact publication use those same owner messages.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{
        ContinuationMethodSpec as MethodSpec, ContinuationModule as Module, ContinuationProtocol,
        Control, Literal, Protocol,
    },
};
mod artifact;
mod candidate_inventory;
mod compiled_source;
mod embedding;
mod foreign;
mod generation_projection;
mod image;
mod materialization;
mod reflection;
mod source;
mod source_evaluation;
mod source_metadata;
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
    synchronous_implementation_entry: Oop,
    binding_entry: Oop,
    implementation_getter: Oop,
    dnu_adapter: Oop,
    ns: Oop,
    receiver: Oop,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    sources: source::Sources,
    image_busy: Cell<bool>,
    materializations: RefCell<Vec<materialization::Cached>>,
    image_stage: RefCell<Option<image::Stage>>,
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
    unsafe fn transfer(&self, operation: &[u8], args: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            let value = (self.control)(
                self.id.root,
                self.names[operation],
                args.as_ptr(),
                args.len(),
            );
            if abi::is_fault(value) {
                Err(abi::object_reference(value))
            } else {
                Ok(value)
            }
        }
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
        let mut values = vec![
            s.ns,
            s.receiver,
            s.reflect,
            s.implementation_entry,
            s.synchronous_implementation_entry,
            s.binding_entry,
            s.implementation_getter,
            s.dnu_adapter,
        ];
        values.extend_from_slice(&s.scoped.borrow());
        values.extend(
            s.image_stage
                .borrow()
                .iter()
                .flat_map(|stage| stage.roots()),
        );
        values.extend(s.materializations.borrow().iter().map(|entry| entry.method));
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
unsafe fn emit_source(s: &State, source: Oop, resumable: bool) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[source]);
        let bytes = s.bytes(source)?;
        let tree = q.keep(s.send(s.named(b"Lang")?, b"parse:", &[source])?);
        if tree.is_null() || tree == s.named(b"nil")? {
            return s.error(b"source parse failed");
        }
        if s.is_error(tree)? {
            return Ok(tree);
        }
        let tree = s.project(tree, bytes.len(), 0, &mut 1_000_000)?;
        let mut program = match birnam_codegen::lower_program(&tree, &bytes) {
            Ok(v) => v,
            Err(e) => return s.error(format!("source lowering: {e:?}").as_bytes()),
        };
        if let Some(error) = s.embed_program(&q, &mut program, b"<compiler>")? {
            return Ok(error);
        }
        emit_program(s, program, &bytes, None, resumable)
    }
}
unsafe fn emit_program(
    s: &State,
    program: birnam_codegen::Program<'_>,
    source: &[u8],
    filename: Option<&[u8]>,
    resumable: bool,
) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[]);
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
            let source_snapshot = q.keep(s.text(source)?);
            if let Some(filename) = filename {
                let filename = q.keep(s.text(filename)?);
                let truth = q.keep(s.named(b"true")?);
                let falsity = q.keep(s.named(b"false")?);
                s.send(
                    s.named(b"NativeBackend")?,
                    b"emit:source:filename:persistent:synchronous:",
                    &[encoded, source_snapshot, filename, truth, falsity],
                )?
            } else {
                s.send(
                    s.named(b"NativeBackend")?,
                    b"emitResumable:source:",
                    &[encoded, source_snapshot],
                )?
            }
        } else {
            s.send(s.named(b"NativeBackend")?, b"emitSynchronous:", &[encoded])?
        });
        Ok(native)
    }
}
unsafe fn compile_source(c: Oop, a: *const Oop, n: usize, resumable: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*source]);
        let native = q.keep(emit_source(s, *source, resumable)?);
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
unsafe extern "C" fn emit(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        emit_source(s, *source, true)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn evaluate(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [k, source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[r, *k, *source]);
        let method = q.keep(s.send(r, b"compile:", &[*source])?);
        if s.is_error(method)? {
            return s.transfer(b"answerTo:with:", &[*k, method]);
        }
        let block = q.keep(s.send(
            s.named(b"Block")?,
            b"fromMethod:captureOperands:",
            &[method],
        )?);
        s.transfer(b"prepareSend", &[block, s.names[b"value".as_slice()], *k])
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
        let policy = abi::as_size(arity).ok_or_else(invalid)?;
        let arity = policy & 0xff;
        if policy & 0x200 == 0
            && (args.len() < arity || (policy & 0x100 == 0 && arity != args.len()))
        {
            return s.transfer(b"answerTo:with:", &[*k, s.named(b"nil")?]);
        }
        let mut values = vec![implementation, selector, *k, r];
        values.extend_from_slice(if policy & 0x100 != 0 {
            &args[..args.len().min(arity)]
        } else {
            args
        });
        s.transfer(b"prepareSend", &values)
    })()
    .unwrap_or_else(abi::fault_reference)
}
impl State {
    unsafe fn value_selector(&self, q: &Scope<'_>, arity: usize) -> Result<Oop, Oop> {
        unsafe {
            if arity > 16 {
                return Err(invalid());
            }
            let words: Vec<_> = b"value:"
                .repeat(arity + 1)
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let name = q.keep(self.send(self.id.root, b"nameWithBytes", &words)?);
            Ok(q.keep(self.id.send(name, self.id.intern, &[])?))
        }
    }
    unsafe fn implementation(
        &self,
        q: &Scope<'_>,
        template: Oop,
        implementation: Oop,
        selector: Oop,
        arity: Oop,
    ) -> Result<Oop, Oop> {
        unsafe {
            // Execution owns Block storage and explicit incoming adapters.
            // Query that owner at construction; never inspect a foreign payload
            // or probe an arbitrary implementation's optional methods.
            let signature = q.keep(self.send(
                self.named(b"Execution")?,
                b"nativeSignatureOf:",
                &[implementation],
            )?);
            let native = signature != self.named(b"nil")?;
            let policy = abi::as_size(arity).ok_or_else(invalid)? & !0x200;
            let arity = abi::size(policy | if native { 0x200 } else { 0 }).ok_or_else(invalid)?;
            let template = if native {
                self.implementation_entry
            } else {
                template
            };
            let context = q.keep(self.send(
                self.id.root,
                b"sequence",
                &[implementation, selector, arity],
            )?);
            let method = q.keep(self.send(template, b"withContext:", &[context])?);
            let base = q.keep(self.send(self.id.root, b"methodBehavior", &[])?);
            let behavior = q.keep(self.send(base, b"delegated", &[])?);
            let getter = q.keep(self.send(
                self.implementation_getter,
                b"withContext:",
                &[implementation],
            )?);
            self.send(
                behavior,
                b"addMethod",
                &[self.names[b"implementation".as_slice()], getter],
            )?;
            let sync = q.keep(self.send(template, b"isSynchronous", &[])?);
            let getter = q.keep(self.send(self.implementation_getter, b"withContext:", &[sync])?);
            self.send(
                behavior,
                b"addMethod",
                &[self.names[b"isSynchronous".as_slice()], getter],
            )?;
            let binding = context;
            let binder = q.keep(self.send(self.binding_entry, b"withContext:", &[binding])?);
            self.send(
                behavior,
                b"addMethod",
                &[self.names[b"withImplementation:".as_slice()], binder],
            )?;
            let dnu = q.keep(self.send(self.dnu_adapter, b"withContext:", &[context])?);
            self.send(
                behavior,
                b"addMethod",
                &[self.names[b"forDoesNotUnderstand".as_slice()], dnu],
            )?;
            self.send(
                self.id.root,
                b"reassociateBehaviorOf:with:",
                &[method, behavior],
            )?;
            Ok(method)
        }
    }
}
unsafe extern "C" fn construct_method(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [arity, implementation] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*arity, *implementation]);
        let arity = s.index(*arity)?;
        let selector = s.value_selector(&q, arity)?;
        s.implementation(
            &q,
            s.implementation_entry,
            *implementation,
            selector,
            abi::size(arity).unwrap(),
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Neutral templates share the declared entry. The construction context contains
// only selector and arity; it never retains the first bound implementation.
unsafe fn template(c: Oop, a: *const Oop, n: usize, selected: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if args.len() != if selected { 2 } else { 1 } {
            return Err(invalid());
        }
        let arity = &args[0];
        let synchronous = if selected {
            if args[1] == s.named(b"true")? {
                true
            } else if args[1] == s.named(b"false")? {
                false
            } else {
                return Err(invalid());
            }
        } else {
            false
        };
        let q = s.scope(&[*arity]);
        let arity = s.index(*arity)?;
        let selector = s.value_selector(&q, arity)?;
        let arity = abi::size(arity).unwrap();
        let nil = q.keep(s.named(b"nil")?);
        let template = q.keep(s.implementation(
            &q,
            if synchronous {
                s.synchronous_implementation_entry
            } else {
                s.implementation_entry
            },
            nil,
            selector,
            arity,
        )?);
        Ok(template)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn method_template(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { template(c, a, n, false) }
}
unsafe extern "C" fn method_template_selected(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { template(c, a, n, true) }
}
unsafe extern "C" fn enter_implementation_sync(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = state((*p).ns)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        let q = s.scope(&[c, r]);
        let implementation = q.keep(s.send(c, b"at:", &[abi::size(0).unwrap()])?);
        let selector = q.keep(s.send(c, b"at:", &[abi::size(1).unwrap()])?);
        let arity =
            abi::as_size(s.send(c, b"at:", &[abi::size(2).unwrap()])?).ok_or_else(invalid)?;
        let policy = arity;
        let arity = policy & 0xff;
        if policy & 0x200 == 0 && (n < arity || (policy & 0x100 == 0 && n != arity)) {
            return s.named(b"nil");
        }
        let mut values = vec![r];
        values.extend_from_slice(if policy & 0x100 != 0 {
            &args[..args.len().min(arity)]
        } else {
            args
        });
        s.id.send(implementation, selector, &values)
    })()
    .unwrap_or_else(abi::fault_reference)
}
// This describes the owner's shared continuing entry, not a Machine frame.
unsafe extern "C" fn method_descriptor(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [name] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*name]);
        let name = s.bytes(*name)?;
        let first = *name.first().ok_or_else(invalid)?;
        let colons = name.iter().filter(|&&b| b == b':').count();
        let arity = if colons > 0 {
            colons
        } else {
            usize::from(!first.is_ascii_alphabetic() && first != b'_')
        };
        if arity > 16 {
            return s.error(b"unsupported reflected Method arity");
        }
        let mut bytes = b"birnam.oop-reflected-method.v1\0".to_vec();
        bytes.extend_from_slice(&(arity as u32).to_le_bytes());
        s.source_text(&q, &bytes)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn bind_implementation(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = state((*p).ns)?;
        let [implementation] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[c, r, *implementation]);
        let selector = q.keep(s.send(c, b"at:", &[abi::size(1).unwrap()])?);
        let arity = q.keep(s.send(c, b"at:", &[abi::size(2).unwrap()])?);
        let method = q.keep(s.implementation(&q, r, *implementation, selector, arity)?);
        Ok(method)
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Id's DNU packet contains the selector followed by original arguments. Source
// Methods consume their declared prefix; native Methods retain their own ABI.
unsafe extern "C" fn dnu_adapter(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = state((*p).ns)?;
        if n != 0 {
            return Err(invalid());
        }
        let q = s.scope(&[c, r]);
        let policy =
            abi::as_size(s.send(c, b"at:", &[abi::size(2).unwrap()])?).ok_or_else(invalid)?;
        if policy & 0x100 != 0 {
            return Ok(r);
        }
        let implementation = q.keep(s.send(c, b"at:", &[abi::size(0).unwrap()])?);
        let selector = q.keep(s.send(c, b"at:", &[abi::size(1).unwrap()])?);
        s.implementation(
            &q,
            r,
            implementation,
            selector,
            abi::size(policy | 0x100).ok_or_else(invalid)?,
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn implementation_value(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    if n == 0 {
        c
    } else {
        abi::fault_reference(invalid())
    }
}
unsafe extern "C" fn implementation_of(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.send(*method, b"implementation", &[])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn method_from_template(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [template, implementation] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.send(*template, b"withImplementation:", &[*implementation])
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
    b"emit:",
    b"eval:",
    b"value",
    b"answerTo:with:",
    b"sourceTextOf:",
    b"compiledSourceFor:",
    b"retainedSourceMetadataOf:",
    b"compiledSource:metadata:",
    b"sourceRevisionOf:",
    b"moduleMetadataOf:",
    b"analyzeSourceMetadata:",
    b"compiledSourceMetadataFor:",
    b"embeddedFilesOf:",
    b"lastError",
    b"noteCompilationError:",
    b"clearCompilationError",
    b"recordModuleSource:named:",
    b"recordCompiledSource:named:",
    b"moduleSourceFor:",
    b"replaceCompiledSource:named:",
    b"Globals",
    b"SourceRegistry",
    b"StringVt",
    b"VectorVt",
    b"SymbolVt",
    b"BytesVt",
    b"Vector",
    b"Bytes",
    b"new",
    b"add:",
    b"size",
    b"compilationForSource:",
    b"false",
    b"recordedMethodSources",
    b"sourceRegistry",
    b"evalPersisting:",
    b"evalPersisting:from:",
    b"furthestFail",
    b"message",
    b"System",
    b"embedFile:",
    b"emit:source:filename:persistent:synchronous:",
    b"compile:output:",
    b"compile:output:from:",
    b"load:",
    b"compileObject:",
    b"loadObject:",
    b"readFile:",
    b"writeFile:contents:",
    b"Artifact",
    b"inspectBytes:from:",
    b"kind",
    b"capabilities",
    b"requires",
    b"nativeRequires",
    b"sourceDigest",
    b"revisionDigest",
    b"identity",
    b"Module",
    b"isLoaded:",
    b"methodBehavior",
    b"compile:signature:",
    b"compile:output:root:",
    b"compile:output:from:synchronousBodies:",
    b"compile:output:root:embeddedLibraries:",
    b"moduleFactory:source:dispositions:",
    b"Alien",
    b"provideLibraryPath:named:",
    b"provideLibrary:named:",
    b"methodTemplateForArity:",
    b"bindImplementation",
    b"withImplementation:",
    b"methodForTemplate:implementation:",
    b"reassociateBehaviorOf:with:",
    b"implementationValue",
    b"implementation",
    b"implementationOf:",
    b"parentOf:",
    b"enterImplementationSync",
    b"methodTemplateForArity:synchronous:",
    b"methodDescriptorFor:",
    b"initialGenerationProjection",
    b"candidateInventory:selection:",
    b"isSynchronous",
    b"admitCodeObject:synchronous:",
    b"portableImageBindings",
    b"compilePortableImageCode:bindings:selection:",
    b"beginPortableImageRebuild:bindings:selection:",
    b"beginPortableImageMapped:programs:bindings:selection:",
    b"portableImageBlockMethod:synchronous:",
    b"preparePortableImageBinding:implementation:",
    b"installPortableImageBinding:on:as:source:",
    b"finishPortableImageUnit",
    b"cancelPortableImageUnit",
    b"portableImageSources",
    b"preparePortableImageSources:",
    b"finishPortableImageSources",
    b"preparedPortableImageBinding:",
    b"beginPortableImageApplication",
    b"allocation:bytes:identity:",
    b"binding:value:weak:",
    b"Generation",
    b"beginImageApplication",
    b"finishImageApplication",
    b"validate",
    b"allocationsDo:",
    b"bindingsDo:",
    b"isBehavior:",
    b"ownerOf:",
    b"nameOf:",
    b"materializeSynchronous:captures:",
    b"materializeResumable:captures:",
    b"materializationCount",
    b"evictMaterializations",
    b"canonicalBytesOf:",
    b"fromMethod:codeObject:captures:",
    b"CodeObject",
    b"allMethodsOf:",
    b"directMethodsOf:",
    b"removeMethod:on:",
    b"removeMethod:",
    b"directMethodFor:in:",
    b"Execution",
    b"Map",
    b"kind:",
    b"forDoesNotUnderstand",
    b"clone",
    b"sliceFromWord:toWord:",
    b"fromByteValues",
    b"appendByteValues",
    b"globalNamed:",
    b"allGlobalNames",
    b"count",
    b"nameAt:",
    b"nativeSignatureOf:",
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
    Literal::new(b"emit:"),
    Literal::new(b"eval:"),
    Literal::new(b"value"),
    Literal::new(b"answerTo:with:"),
    Literal::new(b"sourceTextOf:"),
    Literal::new(b"compiledSourceFor:"),
    Literal::new(b"retainedSourceMetadataOf:"),
    Literal::new(b"compiledSource:metadata:"),
    Literal::new(b"sourceRevisionOf:"),
    Literal::new(b"moduleMetadataOf:"),
    Literal::new(b"analyzeSourceMetadata:"),
    Literal::new(b"compiledSourceMetadataFor:"),
    Literal::new(b"embeddedFilesOf:"),
    Literal::new(b"lastError"),
    Literal::new(b"noteCompilationError:"),
    Literal::new(b"clearCompilationError"),
    Literal::new(b"recordModuleSource:named:"),
    Literal::new(b"recordCompiledSource:named:"),
    Literal::new(b"moduleSourceFor:"),
    Literal::new(b"replaceCompiledSource:named:"),
    Literal::new(b"Globals"),
    Literal::new(b"SourceRegistry"),
    Literal::new(b"StringVt"),
    Literal::new(b"VectorVt"),
    Literal::new(b"SymbolVt"),
    Literal::new(b"BytesVt"),
    Literal::new(b"Vector"),
    Literal::new(b"Bytes"),
    Literal::new(b"new"),
    Literal::new(b"add:"),
    Literal::new(b"size"),
    Literal::new(b"compilationForSource:"),
    Literal::new(b"false"),
    Literal::new(b"recordedMethodSources"),
    Literal::new(b"sourceRegistry"),
    Literal::new(b"evalPersisting:"),
    Literal::new(b"evalPersisting:from:"),
    Literal::new(b"furthestFail"),
    Literal::new(b"message"),
    Literal::new(b"System"),
    Literal::new(b"embedFile:"),
    Literal::new(b"emit:source:filename:persistent:synchronous:"),
    Literal::new(b"compile:output:"),
    Literal::new(b"compile:output:from:"),
    Literal::new(b"load:"),
    Literal::new(b"compileObject:"),
    Literal::new(b"loadObject:"),
    Literal::new(b"readFile:"),
    Literal::new(b"writeFile:contents:"),
    Literal::new(b"Artifact"),
    Literal::new(b"inspectBytes:from:"),
    Literal::new(b"kind"),
    Literal::new(b"capabilities"),
    Literal::new(b"requires"),
    Literal::new(b"nativeRequires"),
    Literal::new(b"sourceDigest"),
    Literal::new(b"revisionDigest"),
    Literal::new(b"identity"),
    Literal::new(b"Module"),
    Literal::new(b"isLoaded:"),
    Literal::new(b"methodBehavior"),
    Literal::new(b"compile:signature:"),
    Literal::new(b"compile:output:root:"),
    Literal::new(b"compile:output:from:synchronousBodies:"),
    Literal::new(b"compile:output:root:embeddedLibraries:"),
    Literal::new(b"moduleFactory:source:dispositions:"),
    Literal::new(b"Alien"),
    Literal::new(b"provideLibraryPath:named:"),
    Literal::new(b"provideLibrary:named:"),
    Literal::new(b"methodTemplateForArity:"),
    Literal::new(b"bindImplementation"),
    Literal::new(b"withImplementation:"),
    Literal::new(b"methodForTemplate:implementation:"),
    Literal::new(b"reassociateBehaviorOf:with:"),
    Literal::new(b"implementationValue"),
    Literal::new(b"implementation"),
    Literal::new(b"implementationOf:"),
    Literal::new(b"parentOf:"),
    Literal::new(b"enterImplementationSync"),
    Literal::new(b"methodTemplateForArity:synchronous:"),
    Literal::new(b"methodDescriptorFor:"),
    Literal::new(b"initialGenerationProjection"),
    Literal::new(b"candidateInventory:selection:"),
    Literal::new(b"isSynchronous"),
    Literal::new(b"admitCodeObject:synchronous:"),
    Literal::new(b"portableImageBindings"),
    Literal::new(b"compilePortableImageCode:bindings:selection:"),
    Literal::new(b"beginPortableImageRebuild:bindings:selection:"),
    Literal::new(b"beginPortableImageMapped:programs:bindings:selection:"),
    Literal::new(b"portableImageBlockMethod:synchronous:"),
    Literal::new(b"preparePortableImageBinding:implementation:"),
    Literal::new(b"installPortableImageBinding:on:as:source:"),
    Literal::new(b"finishPortableImageUnit"),
    Literal::new(b"cancelPortableImageUnit"),
    Literal::new(b"portableImageSources"),
    Literal::new(b"preparePortableImageSources:"),
    Literal::new(b"finishPortableImageSources"),
    Literal::new(b"preparedPortableImageBinding:"),
    Literal::new(b"beginPortableImageApplication"),
    Literal::new(b"allocation:bytes:identity:"),
    Literal::new(b"binding:value:weak:"),
    Literal::new(b"Generation"),
    Literal::new(b"beginImageApplication"),
    Literal::new(b"finishImageApplication"),
    Literal::new(b"validate"),
    Literal::new(b"allocationsDo:"),
    Literal::new(b"bindingsDo:"),
    Literal::new(b"isBehavior:"),
    Literal::new(b"ownerOf:"),
    Literal::new(b"nameOf:"),
    Literal::new(b"materializeSynchronous:captures:"),
    Literal::new(b"materializeResumable:captures:"),
    Literal::new(b"materializationCount"),
    Literal::new(b"evictMaterializations"),
    Literal::new(b"canonicalBytesOf:"),
    Literal::new(b"fromMethod:codeObject:captures:"),
    Literal::new(b"CodeObject"),
    Literal::new(b"allMethodsOf:"),
    Literal::new(b"directMethodsOf:"),
    Literal::new(b"removeMethod:on:"),
    Literal::new(b"removeMethod:"),
    Literal::new(b"directMethodFor:in:"),
    Literal::new(b"Execution"),
    Literal::new(b"Map"),
    Literal::new(b"kind:"),
    Literal::new(b"forDoesNotUnderstand"),
    Literal::new(b"clone"),
    Literal::new(b"sliceFromWord:toWord:"),
    Literal::new(b"fromByteValues"),
    Literal::new(b"appendByteValues"),
    Literal::new(b"globalNamed:"),
    Literal::new(b"allGlobalNames"),
    Literal::new(b"count"),
    Literal::new(b"nameAt:"),
    Literal::new(b"nativeSignatureOf:"),
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
    MethodSpec {
        name: 20,
        entry: Some(compile_resumable),
        convention: 0,
    },
    MethodSpec {
        name: 62,
        entry: Some(emit),
        convention: 0,
    },
    MethodSpec {
        name: 63,
        entry: Some(evaluate),
        convention: 1,
    },
    MethodSpec {
        name: 66,
        entry: Some(source::text),
        convention: 0,
    },
    MethodSpec {
        name: 67,
        entry: Some(source::compilation),
        convention: 0,
    },
    MethodSpec {
        name: 68,
        entry: Some(source::retained),
        convention: 0,
    },
    MethodSpec {
        name: 69,
        entry: Some(source::make),
        convention: 0,
    },
    MethodSpec {
        name: 70,
        entry: Some(source::revision),
        convention: 0,
    },
    MethodSpec {
        name: 71,
        entry: Some(source::metadata),
        convention: 0,
    },
    MethodSpec {
        name: 72,
        entry: Some(source::prepare),
        convention: 0,
    },
    MethodSpec {
        name: 73,
        entry: Some(source::encoded),
        convention: 0,
    },
    MethodSpec {
        name: 74,
        entry: Some(source::embedded),
        convention: 0,
    },
    MethodSpec {
        name: 75,
        entry: Some(source::last_error),
        convention: 0,
    },
    MethodSpec {
        name: 76,
        entry: Some(source::note),
        convention: 0,
    },
    MethodSpec {
        name: 77,
        entry: Some(source::clear),
        convention: 0,
    },
    MethodSpec {
        name: 78,
        entry: Some(source::record),
        convention: 1,
    },
    MethodSpec {
        name: 79,
        entry: Some(source::record_compiled),
        convention: 1,
    },
    MethodSpec {
        name: 80,
        entry: Some(source::find),
        convention: 1,
    },
    MethodSpec {
        name: 81,
        entry: Some(source::replace),
        convention: 1,
    },
    MethodSpec {
        name: 95,
        entry: Some(source::method_sources),
        convention: 0,
    },
    MethodSpec {
        name: 97,
        entry: Some(source_evaluation::eval),
        convention: 1,
    },
    MethodSpec {
        name: 98,
        entry: Some(source_evaluation::eval_from),
        convention: 1,
    },
    MethodSpec {
        name: 104,
        entry: Some(artifact::compile_output),
        convention: 0,
    },
    MethodSpec {
        name: 105,
        entry: Some(artifact::compile_from),
        convention: 0,
    },
    MethodSpec {
        name: 106,
        entry: Some(artifact::load),
        convention: 0,
    },
    MethodSpec {
        name: 124,
        entry: Some(artifact::compile_root),
        convention: 0,
    },
    MethodSpec {
        name: 125,
        entry: Some(artifact::compile_synchronous),
        convention: 0,
    },
    MethodSpec {
        name: 126,
        entry: Some(artifact::compile_embedded),
        convention: 0,
    },
    MethodSpec {
        name: 131,
        entry: Some(method_template),
        convention: 0,
    },
    MethodSpec {
        name: 132,
        entry: Some(bind_implementation),
        convention: 0,
    },
    MethodSpec {
        name: 134,
        entry: Some(method_from_template),
        convention: 0,
    },
    MethodSpec {
        name: 136,
        entry: Some(implementation_value),
        convention: 0,
    },
    MethodSpec {
        name: 138,
        entry: Some(implementation_of),
        convention: 0,
    },
    MethodSpec {
        name: 140,
        entry: Some(enter_implementation_sync),
        convention: 0,
    },
    MethodSpec {
        name: 141,
        entry: Some(method_template_selected),
        convention: 0,
    },
    MethodSpec {
        name: 142,
        entry: Some(method_descriptor),
        convention: 0,
    },
    MethodSpec {
        name: 143,
        entry: Some(generation_projection::initial),
        convention: 0,
    },
    MethodSpec {
        name: 144,
        entry: Some(candidate_inventory::inventory),
        convention: 0,
    },
    MethodSpec {
        name: 147,
        entry: Some(image::bindings_entry),
        convention: 0,
    },
    MethodSpec {
        name: 148,
        entry: Some(image::compile),
        convention: 0,
    },
    MethodSpec {
        name: 149,
        entry: Some(image::rebuild),
        convention: 0,
    },
    MethodSpec {
        name: 150,
        entry: Some(image::mapped),
        convention: 0,
    },
    MethodSpec {
        name: 151,
        entry: Some(image::method),
        convention: 0,
    },
    MethodSpec {
        name: 152,
        entry: Some(image::prepare),
        convention: 0,
    },
    MethodSpec {
        name: 153,
        entry: Some(image::install),
        convention: 0,
    },
    MethodSpec {
        name: 154,
        entry: Some(image::finish),
        convention: 0,
    },
    MethodSpec {
        name: 155,
        entry: Some(image::cancel),
        convention: 0,
    },
    MethodSpec {
        name: 156,
        entry: Some(image::sources),
        convention: 0,
    },
    MethodSpec {
        name: 157,
        entry: Some(image::prepare_sources),
        convention: 0,
    },
    MethodSpec {
        name: 158,
        entry: Some(image::finish_sources),
        convention: 0,
    },
    MethodSpec {
        name: 159,
        entry: Some(image::prepared),
        convention: 0,
    },
    MethodSpec {
        name: 160,
        entry: Some(image::begin_application),
        convention: 0,
    },
    MethodSpec {
        name: 161,
        entry: Some(image::allocation),
        convention: 0,
    },
    MethodSpec {
        name: 162,
        entry: Some(image::binding),
        convention: 0,
    },
    MethodSpec {
        name: 172,
        entry: Some(materialization::synchronous),
        convention: 0,
    },
    MethodSpec {
        name: 173,
        entry: Some(materialization::resumable),
        convention: 0,
    },
    MethodSpec {
        name: 174,
        entry: Some(materialization::count),
        convention: 0,
    },
    MethodSpec {
        name: 175,
        entry: Some(materialization::evict),
        convention: 0,
    },
    MethodSpec {
        name: 179,
        entry: Some(reflection::all_methods),
        convention: 0,
    },
    MethodSpec {
        name: 180,
        entry: Some(reflection::direct_methods),
        convention: 0,
    },
    MethodSpec {
        name: 181,
        entry: Some(reflection::remove),
        convention: 0,
    },
    MethodSpec {
        name: 187,
        entry: Some(dnu_adapter),
        convention: 0,
    },
    MethodSpec {
        name: 171,
        entry: Some(reflection::name_of),
        convention: 0,
    },
    MethodSpec {
        name: 192,
        entry: Some(reflection::global_named),
        convention: 0,
    },
    MethodSpec {
        name: 193,
        entry: Some(reflection::global_names),
        convention: 0,
    },
    MethodSpec {
        name: 139,
        entry: Some(reflection::parent),
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
            synchronous_implementation_entry: send(
                image,
                b"methodAt:",
                &[names[b"enterImplementationSync".as_slice()]],
            )?,
            binding_entry: send(
                image,
                b"methodAt:",
                &[names[b"bindImplementation".as_slice()]],
            )?,
            implementation_getter: send(
                image,
                b"methodAt:",
                &[names[b"implementationValue".as_slice()]],
            )?,
            dnu_adapter: send(
                image,
                b"methodAt:",
                &[names[b"forDoesNotUnderstand".as_slice()]],
            )?,
            ns: *ns,
            receiver,
            names: names.clone(),
            scoped: RefCell::new(Vec::new()),
            sources: source::Sources::default(),
            image_busy: Cell::new(false),
            materializations: RefCell::new(Vec::new()),
            image_stage: RefCell::new(None),
            ready: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for (entry, synchronous) in [
            (s.implementation_entry, false),
            (s.synchronous_implementation_entry, true),
        ] {
            send(
                entry,
                b"admitCodeObject:synchronous:",
                &[
                    named(b"nil")?,
                    named(if synchronous { b"true" } else { b"false" })?,
                ],
            )?;
        }
        for spec in METHODS {
            let name = NAMES[spec.name];
            if matches!(
                name,
                b"enterImplementation"
                    | b"enterImplementationSync"
                    | b"bindImplementation"
                    | b"implementationValue"
                    | b"forDoesNotUnderstand"
            ) {
                continue;
            }
            let target = if matches!(
                name,
                b"defineMethod:on:as:"
                    | b"allMethodsOf:"
                    | b"directMethodsOf:"
                    | b"removeMethod:on:"
                    | b"methodForArity:implementation:"
                    | b"methodForTemplate:implementation:"
                    | b"vtableOf:"
                    | b"nameOf:"
                    | b"globalNamed:"
                    | b"allGlobalNames"
                    | b"parentOf:"
            ) {
                reflect_vt
            } else {
                vt
            };
            let method = send(image, b"methodAt:", &[symbols[spec.name]])?;
            let method = send(method, b"withContext:", &[*ns])?;
            send(target, b"addMethod", &[symbols[spec.name], method])?;
        }
        let nil_getter = send(s.implementation_getter, b"withContext:", &[named(b"nil")?])?;
        send(
            send(id.root, b"methodBehavior", &[])?,
            b"addMethod",
            &[names[b"implementation".as_slice()], nil_getter],
        )?;
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
