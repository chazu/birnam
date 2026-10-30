//! TCC owns compilation, declared generated entries and mapping retirement.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{
        self as native, ContinuationMethodSpec as MethodSpec, ContinuationModule as Module,
        ContinuationProtocol, Literal, Protocol,
    },
};
use birnam_native_compiler::Unit;
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::HashMap,
    ffi::CString,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
enum GeneratedInitializer {
    Synchronous(native::Initialize),
    Continuing(native::ContinuationInitialize),
}
struct Compiled {
    native: Unit,
    identity: usize,
    refs: Vec<Oop>,
    result: Option<Oop>,
    initializing: bool,
    implementation: Option<Oop>,
    methods: Vec<Oop>,
    declarations: Vec<(native::Method, bool)>,
}
#[derive(Clone, Copy)]
struct Entry {
    unit: Oop,
    code: native::Method,
    context: Oop,
    method: Oop,
}
struct State {
    pub(crate) byte_reader: abi::byte_transfer::Reader,
    id: Protocol,
    continuing: ContinuationProtocol,
    ns: Oop,
    gc: Oop,
    model: Oop,
    unit_vt: Oop,
    entry_vt: Oop,
    receiver: Oop,
    template: Oop,
    continuation_template: Oop,
    names: HashMap<&'static [u8], Oop>,
    permanent: RefCell<Vec<Oop>>,
    scoped: RefCell<Vec<Oop>>,
    units: RefCell<HashMap<usize, Compiled>>,
    entries: RefCell<HashMap<usize, Entry>>,
    retired: RefCell<Vec<(usize, Unit)>>,
    next_identity: Cell<usize>,
    depth: Cell<usize>,
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
fn object(v: Oop) -> Result<Oop, Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
fn number(v: usize) -> Result<Oop, Oop> {
    abi::size(v).ok_or_else(invalid)
}
fn size(v: Oop) -> Result<usize, Oop> {
    abi::as_size(v).ok_or_else(invalid)
}
unsafe fn instance<'a>() -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let s = unsafe { &*p };
    if s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let s = unsafe { instance() }?;
    if c == s.ns { Ok(s) } else { Err(invalid()) }
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
struct Entered<'a>(&'a State);
impl Drop for Entered<'_> {
    fn drop(&mut self) {
        self.0.depth.set(self.0.depth.get() - 1);
    }
}
impl State {
    fn scope(&self, values: &[Oop]) -> Scope<'_> {
        let scope = Scope {
            s: self,
            start: self.scoped.borrow().len(),
        };
        self.scoped.borrow_mut().extend_from_slice(values);
        scope
    }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Oop> {
        unsafe { object(self.send(self.ns, b"at:", &[self.names[n]])?) }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Oop> {
        unsafe {
            let scope = self.scope(&[o]);
            let bytes = scope.keep(self.send(o, b"bytes", &[])?);
            self.byte_reader.read(bytes)
        }
    }
    unsafe fn make_bytes(&self, bytes: &[u8], factory: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let scope = self.scope(&[]);
            let factory = scope.keep(self.named(factory)?);
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let source = self.send(self.id.root, b"sequence", &values)?;
            self.send(factory, b"fromBytes:", &[source])
        }
    }
    unsafe fn error(&self, text: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let scope = self.scope(&[]);
            let message = scope.keep(self.make_bytes(text, b"String")?);
            self.send(
                self.named(b"Error")?,
                b"kind:message:",
                &[self.names[b"compileError".as_slice()], message],
            )
        }
    }
    unsafe fn unit(&self, native: Unit) -> Result<Oop, Oop> {
        unsafe {
            let identity = self
                .next_identity
                .get()
                .checked_add(1)
                .filter(|v| *v <= usize::MAX >> 1)
                .ok_or_else(invalid)?;
            let unit = self.send(self.model, b"allocate:bytes:", &[self.unit_vt, number(0)?])?;
            self.next_identity.set(identity);
            self.units.borrow_mut().insert(
                unit.addr(),
                Compiled {
                    native,
                    identity,
                    refs: vec![self.ns],
                    result: None,
                    initializing: false,
                    implementation: None,
                    methods: Vec::new(),
                    declarations: Vec::new(),
                },
            );
            Ok(unit)
        }
    }
}
unsafe extern "C" fn compile(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let source = CString::new(s.bytes(object(*source)?)?).map_err(|_| invalid())?;
        match Unit::compile(&source, &[]) {
            Some(unit) => s.unit(unit),
            None => s.error(b"native C compilation or relocation failed"),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn compile_object(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let source = CString::new(s.bytes(object(*source)?)?).map_err(|_| invalid())?;
        match Unit::compile_object(&source) {
            Some(bytes) => s.make_bytes(&bytes, b"Bytes"),
            None => s.error(b"native object compilation failed"),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn load_object(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        match Unit::load_object(&s.bytes(object(*source)?)?) {
            Some(unit) => s.unit(unit),
            None => s.error(b"native object relocation failed"),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
// This is a declared native implementation, entered directly by Id. Its own
// context identifies a generated entry and keeps the mapping alive; no Method
// selector lookup, callback protocol or executable address crosses an Id send.
unsafe extern "C" fn generated(root: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = instance()?;
        let entry = *s.entries.borrow().get(&c.addr()).ok_or_else(invalid)?;
        let _scope = s.scope(&[c, entry.unit, entry.context]);
        s.depth
            .set(s.depth.get().checked_add(1).ok_or_else(invalid)?);
        let _entered = Entered(s);
        let result = (entry.code)(root, entry.context, r, a, n);
        if abi::is_fault(result) {
            Err(abi::object_reference(result))
        } else {
            Ok(result)
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn initialize_unit(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n > 1 {
            return Err(invalid());
        }
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        let scope = s.scope(&[r]);
        for &arg in args {
            scope.keep(object(arg)?);
        }
        let (initialize, literals, declarations) = {
            let units = s.units.borrow();
            let unit = units.get(&r.addr()).ok_or_else(invalid)?;
            if let Some(v) = unit.result {
                if unit.implementation != args.first().copied() {
                    return Err(invalid());
                }
                return Ok(v);
            }
            if unit.initializing {
                return Err(invalid());
            }
            let continuing = unit.native.symbol(c"birnam_native_module_v3");
            let (initialize, literals, declarations) = if let Some(d) = continuing {
                let d = &*d.as_ptr().cast::<Module>();
                if d.version != 3
                    || d.byte_size as usize != size_of::<Module>()
                    || d.literal_count > 65536
                    || d.method_count > 65536
                {
                    return Err(invalid());
                }
                let literals = borrowed_slice(d.literals, d.literal_count).ok_or_else(invalid)?;
                let declarations = borrowed_slice(d.methods, d.method_count).ok_or_else(invalid)?;
                let mut methods = Vec::new();
                for m in declarations {
                    if m.convention > 1 {
                        return Err(invalid());
                    }
                    methods.push((m.name, m.entry.ok_or_else(invalid)?, m.convention == 1));
                }
                (
                    GeneratedInitializer::Continuing(d.initialize.ok_or_else(invalid)?),
                    literals,
                    methods,
                )
            } else {
                let d = unit
                    .native
                    .symbol(c"birnam_native_module_v2")
                    .ok_or_else(invalid)?;
                let d = &*d.as_ptr().cast::<native::Module>();
                if d.version != 2
                    || d.byte_size as usize != size_of::<native::Module>()
                    || d.literal_count > 65536
                    || d.method_count > 65536
                {
                    return Err(invalid());
                }
                let literals = borrowed_slice(d.literals, d.literal_count).ok_or_else(invalid)?;
                let declarations = borrowed_slice(d.methods, d.method_count).ok_or_else(invalid)?;
                let mut methods = Vec::new();
                for m in declarations {
                    methods.push((m.name, m.entry.ok_or_else(invalid)?, false));
                }
                (
                    GeneratedInitializer::Synchronous(d.initialize.ok_or_else(invalid)?),
                    literals,
                    methods,
                )
            };
            let mut names = Vec::new();
            for literal in literals {
                if literal.length > 16 * 1024 * 1024 {
                    return Err(invalid());
                }
                names.push(
                    borrowed_slice(literal.bytes, literal.length)
                        .ok_or_else(invalid)?
                        .to_vec(),
                );
            }
            let mut methods = Vec::new();
            for (name, code, continuing) in declarations {
                if name >= names.len() {
                    return Err(invalid());
                }
                methods.push((code, continuing));
            }
            (initialize, names, methods)
        };
        {
            let mut units = s.units.borrow_mut();
            let unit = units.get_mut(&r.addr()).unwrap();
            unit.initializing = true;
            unit.declarations = declarations.clone();
        }
        // Failed initialization can publish Methods. Keep every constructed context
        // and declared entry in the Unit; retry must not invalidate earlier Methods.
        let result = (|| {
            let mut symbols = Vec::new();
            for bytes in literals {
                let values: Vec<_> = bytes
                    .iter()
                    .map(|&b| abi::size(b as usize).unwrap())
                    .collect();
                let name = s.send(s.id.root, b"nameWithBytes", &values)?;
                let symbol = s.id.send(name, s.id.intern, &[])?;
                scope.keep(symbol);
                symbols.push(symbol);
            }
            let mut methods = Vec::new();
            for (code, continuing) in declarations {
                methods.push(scope.keep(bind_entry(s, r, code, continuing, r)?));
            }
            {
                let mut units = s.units.borrow_mut();
                let unit = units.get_mut(&r.addr()).unwrap();
                unit.refs.extend_from_slice(&symbols);
                unit.refs.extend_from_slice(&methods);
                unit.methods = methods;
            }
            let mut inputs = vec![s.ns];
            inputs.extend_from_slice(args);
            let result = match initialize {
                GeneratedInitializer::Synchronous(f) => f(
                    &s.id,
                    r,
                    symbols.as_ptr(),
                    symbols.len(),
                    inputs.as_ptr(),
                    inputs.len(),
                ),
                GeneratedInitializer::Continuing(f) => f(
                    &s.continuing,
                    r,
                    symbols.as_ptr(),
                    symbols.len(),
                    inputs.as_ptr(),
                    inputs.len(),
                ),
            };
            if abi::is_fault(result) {
                Err(abi::object_reference(result))
            } else {
                Ok(result)
            }
        })();
        let mut units = s.units.borrow_mut();
        let unit = units.get_mut(&r.addr()).ok_or_else(invalid)?;
        unit.initializing = false;
        if let Ok(value) = result {
            unit.result = Some(value);
            unit.refs.push(value);
            unit.implementation = args.first().copied();
            unit.refs.extend_from_slice(args);
        }
        result
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Bind data to declared code within its owner. The supplied context never
// replaces the bookkeeping that retains the actual executable and mapping.
unsafe fn bind_entry(
    s: &State,
    unit: Oop,
    code: native::Method,
    continuing: bool,
    environment: Oop,
) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[unit, environment]);
        let context = q.keep(s.send(s.model, b"allocate:bytes:", &[s.entry_vt, number(0)?])?);
        s.entries.borrow_mut().insert(
            context.addr(),
            Entry {
                unit,
                code,
                context: environment,
                method: ptr::null_mut(),
            },
        );
        let method = q.keep(s.send(
            if continuing {
                s.continuation_template
            } else {
                s.template
            },
            b"withContext:",
            &[context],
        )?);
        s.entries
            .borrow_mut()
            .get_mut(&context.addr())
            .ok_or_else(invalid)?
            .method = method;
        Ok(method)
    }
}
// Reflection belongs to the code provider. This scans existing executable
// records only; normal calls do not query or classify a Method through it.
unsafe extern "C" fn context_of_method(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*method]);
        let entries: Vec<_> = s.entries.borrow().values().copied().collect();
        for entry in &entries {
            if !entry.method.is_null() {
                q.keep(entry.method);
            }
        }
        for entry in entries {
            if entry.method.is_null() {
                continue;
            }
            if entry.method == *method
                || s.send(s.id.root, b"sameNativeMethod:as:", &[*method, entry.method])?
                    == number(1)?
            {
                return Ok(entry.context);
            }
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn method_with_context(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [index, context] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let (code, continuing) = s
            .units
            .borrow()
            .get(&r.addr())
            .filter(|unit| {
                unit.result.is_some()
                    || (unit.initializing && unit.methods.len() == unit.declarations.len())
            })
            .and_then(|unit| unit.declarations.get(abi::as_size(*index)?))
            .copied()
            .ok_or_else(invalid)?;
        bind_entry(s, r, code, continuing, *context)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn method_at(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [index] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.units
            .borrow()
            .get(&r.addr())
            .and_then(|u| u.methods.get(abi::as_size(*index)?))
            .copied()
            .ok_or_else(invalid)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn keep(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.send(s.gc, b"prepareAllocation", &[])?;
        s.units
            .borrow_mut()
            .get_mut(&r.addr())
            .ok_or_else(invalid)?
            .refs
            .push(*value);
        Ok(*value)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn references(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [value, visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let refs = if let Some(unit) = s.units.borrow().get(&value.addr()) {
            unit.refs.clone()
        } else {
            s.entries
                .borrow()
                .get(&value.addr())
                .map(|e| vec![e.unit, e.context, e.method])
                .unwrap_or_default()
        };
        for value in refs {
            s.send(*visitor, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = s.permanent.borrow().clone();
        values.extend_from_slice(&s.scoped.borrow());
        for value in values {
            s.send(*visitor, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn retire(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let values = borrowed_slice(a, n).ok_or_else(invalid)?;
        for value in values {
            s.entries.borrow_mut().remove(&value.addr());
            if let Some(unit) = s.units.borrow_mut().remove(&value.addr()) {
                s.retired.borrow_mut().push((unit.identity, unit.native));
            }
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn checkpoint(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        s.send(s.gc, b"prepareAllocation", &[])?;
        let execution = s.named(b"Execution")?;
        s.send(execution, b"checkpoint", &[])?;
        if s.depth.get() == 0 && size(s.send(execution, b"retirementReady", &[])?)? == 1 {
            drop(std::mem::take(&mut *s.retired.borrow_mut()));
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn mapping_identity(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        number(
            s.units
                .borrow()
                .get(&value.addr())
                .ok_or_else(invalid)?
                .identity,
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn mapping_bytes(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let identity = size(*value)?;
        if identity == 0 {
            return Err(invalid());
        }
        let units = s.units.borrow();
        if let Some(unit) = units.values().find(|u| u.identity == identity) {
            return number(unit.native.relocated_allocation_bytes());
        }
        number(
            s.retired
                .borrow()
                .iter()
                .find(|(id, _)| *id == identity)
                .map_or(0, |(_, u)| u.relocated_allocation_bytes()),
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn live_units(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        number(s.units.borrow().len() + s.retired.borrow().len())
    })()
    .unwrap_or_else(abi::fault_reference)
}
#[cfg(feature = "resource-metrics")]
unsafe extern "C" fn live_bytes(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        number(Unit::resource_totals().1)
    })()
    .unwrap_or_else(abi::fault_reference)
}

const NAMES: &[&[u8]] = &[
    b"Bytes",
    b"Error",
    b"Execution",
    b"GC",
    b"ObjectModel",
    b"ObjectVt",
    b"Roots",
    b"String",
    b"TCC",
    b"TCCUnitVt",
    b"TCCVt",
    b"addMethod",
    b"allocate:bytes:",
    b"at:",
    b"at:put:",
    b"atWord:",
    b"bytes",
    b"checkpoint",
    b"compile:",
    b"compileError",
    b"compileObject:",
    b"declaredMethodAt:",
    b"delegated",
    b"fromBytes:",
    b"generatedEntry",
    b"initialize",
    b"initializeWith:",
    b"kind:message:",
    b"liveNativeBytes",
    b"liveNativeUnits",
    b"loadObject:",
    b"mappingBytes:",
    b"mappingIdentityOf:",
    b"methodAt:",
    b"nameWithBytes",
    b"prepareAllocation",
    b"referencesOf:do:",
    b"register:",
    b"registerRetirement:",
    b"release:",
    b"retain:",
    b"retire:",
    b"retirementReady",
    b"rootsDo:",
    b"sequence",
    b"sizeWord",
    b"trace:with:",
    b"visit:",
    b"withContext:",
    b"generatedContinuationEntry",
    b"declaredMethodAt:context:",
    b"contextOfMethod:",
    b"sameNativeMethod:as:",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"objectBehavior",
    b"allocate",
    b"releaseTokensProtecting",
    b"receiveByteValues",
];
static LITERALS: &[Literal] = &[
    Literal::new(b"Bytes"),
    Literal::new(b"Error"),
    Literal::new(b"Execution"),
    Literal::new(b"GC"),
    Literal::new(b"ObjectModel"),
    Literal::new(b"ObjectVt"),
    Literal::new(b"Roots"),
    Literal::new(b"String"),
    Literal::new(b"TCC"),
    Literal::new(b"TCCUnitVt"),
    Literal::new(b"TCCVt"),
    Literal::new(b"addMethod"),
    Literal::new(b"allocate:bytes:"),
    Literal::new(b"at:"),
    Literal::new(b"at:put:"),
    Literal::new(b"atWord:"),
    Literal::new(b"bytes"),
    Literal::new(b"checkpoint"),
    Literal::new(b"compile:"),
    Literal::new(b"compileError"),
    Literal::new(b"compileObject:"),
    Literal::new(b"declaredMethodAt:"),
    Literal::new(b"delegated"),
    Literal::new(b"fromBytes:"),
    Literal::new(b"generatedEntry"),
    Literal::new(b"initialize"),
    Literal::new(b"initializeWith:"),
    Literal::new(b"kind:message:"),
    Literal::new(b"liveNativeBytes"),
    Literal::new(b"liveNativeUnits"),
    Literal::new(b"loadObject:"),
    Literal::new(b"mappingBytes:"),
    Literal::new(b"mappingIdentityOf:"),
    Literal::new(b"methodAt:"),
    Literal::new(b"nameWithBytes"),
    Literal::new(b"prepareAllocation"),
    Literal::new(b"referencesOf:do:"),
    Literal::new(b"register:"),
    Literal::new(b"registerRetirement:"),
    Literal::new(b"release:"),
    Literal::new(b"retain:"),
    Literal::new(b"retire:"),
    Literal::new(b"retirementReady"),
    Literal::new(b"rootsDo:"),
    Literal::new(b"sequence"),
    Literal::new(b"sizeWord"),
    Literal::new(b"trace:with:"),
    Literal::new(b"visit:"),
    Literal::new(b"withContext:"),
    Literal::new(b"generatedContinuationEntry"),
    Literal::new(b"declaredMethodAt:context:"),
    Literal::new(b"contextOfMethod:"),
    Literal::new(b"sameNativeMethod:as:"),
    Literal::new(b"copyByteValuesFrom:count:to:"),
    Literal::new(b"appendByteValues"),
    Literal::new(b"objectBehavior"),
    Literal::new(b"allocate"),
    Literal::new(b"releaseTokensProtecting"),
    Literal::new(b"receiveByteValues"),
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 18,
        entry: Some(compile),
        convention: 0,
    },
    MethodSpec {
        name: 20,
        entry: Some(compile_object),
        convention: 0,
    },
    MethodSpec {
        name: 30,
        entry: Some(load_object),
        convention: 0,
    },
    MethodSpec {
        name: 17,
        entry: Some(checkpoint),
        convention: 0,
    },
    MethodSpec {
        name: 29,
        entry: Some(live_units),
        convention: 0,
    },
    MethodSpec {
        name: 32,
        entry: Some(mapping_identity),
        convention: 0,
    },
    MethodSpec {
        name: 31,
        entry: Some(mapping_bytes),
        convention: 0,
    },
    MethodSpec {
        name: 43,
        entry: Some(roots),
        convention: 0,
    },
    MethodSpec {
        name: 36,
        entry: Some(references),
        convention: 0,
    },
    MethodSpec {
        name: 41,
        entry: Some(retire),
        convention: 0,
    },
    MethodSpec {
        name: 25,
        entry: Some(initialize_unit),
        convention: 0,
    },
    MethodSpec {
        name: 26,
        entry: Some(initialize_unit),
        convention: 0,
    },
    MethodSpec {
        name: 40,
        entry: Some(keep),
        convention: 0,
    },
    MethodSpec {
        name: 21,
        entry: Some(method_at),
        convention: 0,
    },
    MethodSpec {
        name: 24,
        entry: Some(generated),
        convention: 0,
    },
    #[cfg(feature = "resource-metrics")]
    MethodSpec {
        name: 28,
        entry: Some(live_bytes),
        convention: 0,
    },
    MethodSpec {
        name: 49,
        entry: Some(generated),
        convention: 1,
    },
    MethodSpec {
        name: 50,
        entry: Some(method_with_context),
        convention: 0,
    },
    MethodSpec {
        name: 51,
        entry: Some(context_of_method),
        convention: 0,
    },

    MethodSpec { name: NAMES.len()-1, entry: Some(abi::byte_transfer::receive_entry), convention: 0 },
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
        unsafe { &(*p).common as *const Protocol }.cast_mut(),
        Ordering::Release,
    );
    let continuing = unsafe { *p };
    let mut id = continuing.common;
    id.version = 2;
    id.byte_size = size_of::<Protocol>() as u32;
    let Some([ns]) = (unsafe { borrowed_slice(a, count) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if n != NAMES.len() || ns.is_null() {
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
    let _constructing = Constructing;
    (|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| object(send(*ns, b"at:", &[names[n]])?);
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let model = named(b"ObjectModel")?;
        let mut tokens = Vec::new();
        let mut pin = |v| -> Result<Oop, Oop> {
            tokens.push(send(roots, b"retain:", &[v])?);
            Ok(v)
        };
        let base = named(b"ObjectVt")?;
        let vt = pin(send(base, b"delegated", &[])?)?;
        let unit_vt = pin(send(base, b"delegated", &[])?)?;
        let entry_vt = pin(send(base, b"delegated", &[])?)?;
        let receiver = pin(send(model, b"allocate:bytes:", &[vt, number(0)?])?)?;
        let template = send(image, b"methodAt:", &[names[b"generatedEntry".as_slice()]])?;
        let s = Box::into_raw(Box::new(State {
                byte_reader: abi::byte_transfer::Reader::new(id, image, roots, |n| names[n])?,
            id,
            continuing,
            ns: *ns,
            gc,
            model,
            unit_vt,
            entry_vt,
            receiver,
            template,
            continuation_template: send(
                image,
                b"methodAt:",
                &[names[b"generatedContinuationEntry".as_slice()]],
            )?,
            names: names.clone(),
            permanent: RefCell::new(vec![
                *ns, roots, gc, model, vt, unit_vt, entry_vt, receiver, template,
            ]),
            scoped: RefCell::new(Vec::new()),
            units: RefCell::new(HashMap::new()),
            entries: RefCell::new(HashMap::new()),
            retired: RefCell::new(Vec::new()),
            next_identity: Cell::new(0),
            depth: Cell::new(0),
            ready: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for name in NAMES.iter().copied() {
            let target = match name {
                b"compile:"
                | b"compileObject:"
                | b"loadObject:"
                | b"checkpoint"
                | b"liveNativeUnits"
                | b"contextOfMethod:"
                | b"mappingIdentityOf:"
                | b"mappingBytes:"
                | b"rootsDo:"
                | b"referencesOf:do:"
                | b"retire:" => vt,
                b"initialize"
                | b"initializeWith:"
                | b"retain:"
                | b"declaredMethodAt:"
                | b"declaredMethodAt:context:" => unit_vt,
                #[cfg(feature = "resource-metrics")]
                b"liveNativeBytes" => vt,
                _ => continue,
            };
            let method = send(image, b"methodAt:", &[names[name]])?;
            let method = send(method, b"withContext:", &[*ns])?;
            send(target, b"addMethod", &[names[name], method])?;
        }
        send(roots, b"register:", &[receiver])?;
        send(gc, b"registerRetirement:", &[receiver])?;
        for behavior in [unit_vt, entry_vt] {
            send(gc, b"trace:with:", &[behavior, receiver])?;
        }
        for (name, value) in [
            (b"TCC".as_slice(), receiver),
            (b"TCCVt", vt),
            (b"TCCUnitVt", unit_vt),
        ] {
            send(*ns, b"at:put:", &[names[name], value])?;
        }
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        (*s).ready.set(true);
        Ok(receiver)
    })()
    .unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v3: Module = Module {
    version: native::CONTINUATION_MODULE_VERSION,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
