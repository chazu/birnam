//! Native compilation owns mappings; Execution-owned Methods retain their Units.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use birnam_native_compiler::Unit;
use core::{cell::RefCell, ffi::c_void};
use std::{collections::HashMap, ffi::CString};
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|v| !v.is_null()).ok_or_else(invalid)
}
fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
struct Compiled {
    native: Unit,
    refs: Vec<Oop>,
    result: Option<Oop>,
    initializing: bool,
    implementation: Option<Oop>,
}
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    model: Oop,
    unit_vt: Oop,
    permanent: Vec<Oop>,
    units: RefCell<HashMap<usize, Compiled>>,
    retired: RefCell<Vec<Unit>>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    tokens: RefCell<Vec<usize>>,
}
impl Scope<'_> {
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Outcome> {
        let t = unsafe {
            self.s
                .id
                .send_named(self.s.roots, b"retain:", &[Operand::object(o)])
        }?
        .as_word()
        .ok_or_else(invalid)?;
        self.tokens.borrow_mut().push(t);
        Ok(o)
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        for t in self.tokens.get_mut().drain(..) {
            unsafe {
                self.s
                    .id
                    .send_named(self.s.roots, b"release:", &[Operand::word(t)])
                    .expect("release compiler root");
            }
        }
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { self.id.send_named(r, n, a) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Outcome> {
        unsafe { object(self.send(self.ns, b"at:", &[Operand::object(self.id.intern_name(n)?)])?) }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let b = self
                .send(o, b"bytes", &[])?
                .as_bytes()
                .ok_or_else(invalid)?;
            Ok(borrowed_bytes(b.pointer, b.length)
                .ok_or_else(invalid)?
                .to_vec())
        }
    }
    unsafe fn error(&self, scope: &Scope<'_>, text: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            let m = scope.keep(object(self.send(
                self.named(b"String")?,
                b"fromBytes:",
                &[Operand::bytes(text)],
            )?)?)?;
            self.send(
                self.named(b"Error")?,
                b"kind:message:",
                &[
                    Operand::object(self.id.intern_name(b"compileError")?),
                    Operand::object(m),
                ],
            )
        }
    }
}
unsafe extern "C" fn compile(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([source]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let scope = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        let source = scope.keep(object(*source)?)?;
        let source = CString::new(s.bytes(source)?).map_err(|_| invalid())?;
        let Some(native) = Unit::compile(&source, &[]) else {
            return s.error(&scope, b"native C compilation or relocation failed");
        };
        let unit = scope.keep(object(s.send(
            s.model,
            b"allocate:bytes:",
            &[Operand::object(s.unit_vt), Operand::word(0)],
        )?)?)?;
        s.units.borrow_mut().insert(
            unit as usize,
            Compiled {
                native,
                refs: vec![s.ns],
                result: None,
                initializing: false,
                implementation: None,
            },
        );
        Ok(Operand::object(unit))
    })())
}
unsafe extern "C" fn initialize_unit(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        if n > 1 {
            return Err(invalid());
        }
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let scope = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        scope.keep(r)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        for &v in args {
            scope.keep(object(v)?)?;
        }
        let entry = {
            let mut units = s.units.borrow_mut();
            let unit = units.get_mut(&(r as usize)).ok_or_else(invalid)?;
            if let Some(v) = unit.result {
                if unit.implementation != args.first().copied().map(object).transpose()? {
                    return Err(invalid());
                }
                return Ok(Operand::object(v));
            }
            if unit.initializing {
                return Err(invalid());
            }
            let module = unit
                .native
                .symbol(c"birnam_native_module_v1")
                .ok_or_else(invalid)?;
            let module = &*module.as_ptr().cast::<native::Module>();
            if module.version != native::VERSION
                || module.byte_size as usize != size_of::<native::Module>()
            {
                return Err(invalid());
            }
            let init = module.initialize.ok_or_else(invalid)?;
            unit.initializing = true;
            init
        };
        let mut inputs = vec![Operand::object(s.ns), Operand::object(r)];
        inputs.extend_from_slice(args);
        let result = entry(&s.id, inputs.as_ptr(), inputs.len())
            .into_result()
            .and_then(object);
        let mut units = s.units.borrow_mut();
        let unit = units.get_mut(&(r as usize)).ok_or_else(invalid)?;
        unit.initializing = false;
        if let Ok(value) = result {
            unit.result = Some(value);
            unit.refs.push(value);
            unit.implementation = args.first().copied().map(object).transpose()?;
            if let Some(implementation) = unit.implementation {
                unit.refs.push(implementation);
            }
        }
        result.map(Operand::object)
    })())
}
unsafe extern "C" fn compile_object(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([source]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let scope = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        let source = scope.keep(object(*source)?)?;
        let source = CString::new(s.bytes(source)?).map_err(|_| invalid())?;
        let Some(bytes) = Unit::compile_object(&source) else {
            return s.error(&scope, b"native object compilation failed");
        };
        s.send(s.named(b"Bytes")?, b"fromBytes:", &[Operand::bytes(&bytes)])
    })())
}
unsafe extern "C" fn load_object(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([bytes]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let scope = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        let bytes = scope.keep(object(*bytes)?)?;
        let bytes = s.bytes(bytes)?;
        let Some(native) = Unit::load_object(&bytes) else {
            return s.error(&scope, b"native object relocation failed");
        };
        let unit = scope.keep(object(s.send(
            s.model,
            b"allocate:bytes:",
            &[Operand::object(s.unit_vt), Operand::word(0)],
        )?)?)?;
        s.units.borrow_mut().insert(
            unit as usize,
            Compiled {
                native,
                refs: vec![s.ns],
                result: None,
                initializing: false,
                implementation: None,
            },
        );
        Ok(Operand::object(unit))
    })())
}
unsafe extern "C" fn keep(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        let scope = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        scope.keep(r)?;
        scope.keep(v)?;
        s.send(s.gc, b"prepareAllocation", &[])?;
        s.units
            .borrow_mut()
            .get_mut(&(r as usize))
            .ok_or_else(invalid)?
            .refs
            .push(v);
        Ok(Operand::object(v))
    })())
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([o, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        let refs = s
            .units
            .borrow()
            .get(&(object(*o)? as usize))
            .map(|u| u.refs.clone())
            .unwrap_or_default();
        for o in refs {
            s.id.send_named(v, b"visit:", &[Operand::object(o)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        for &o in &s.permanent {
            s.id.send_named(v, b"visit:", &[Operand::object(o)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn retire(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let b = v.as_bytes().ok_or_else(invalid)?;
        let bytes = borrowed_bytes(b.pointer, b.length).ok_or_else(invalid)?;
        if bytes.len() % size_of::<usize>() != 0 {
            return Err(invalid());
        }
        for b in bytes.chunks_exact(size_of::<usize>()) {
            let removed = s
                .units
                .borrow_mut()
                .remove(&usize::from_ne_bytes(b.try_into().unwrap()));
            if let Some(unit) = removed {
                s.retired.borrow_mut().push(unit.native);
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn checkpoint(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if n != 0 {
            return Err(invalid());
        }
        let s = &*c.cast::<State>();
        s.send(s.gc, b"prepareAllocation", &[])?;
        let execution = s.named(b"Execution")?;
        s.send(execution, b"checkpoint", &[])?;
        if s.send(execution, b"retirementReady", &[])?.as_word() != Some(1) {
            return Ok(Operand::unit());
        }
        let retired = std::mem::take(&mut *s.retired.borrow_mut());
        drop(retired);
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn live_units(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 {
        return invalid();
    }
    let s = unsafe { &*c.cast::<State>() };
    Outcome::loaded(Operand::word(
        s.units.borrow().len() + s.retired.borrow().len(),
    ))
}
#[cfg(feature = "resource-metrics")]
unsafe extern "C" fn live_bytes(_: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 {
        return invalid();
    }
    Outcome::loaded(Operand::word(Unit::resource_totals().1))
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing = id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"TCC")?)])?;
        if existing.kind() == native::OBJECT {
            return Ok(existing);
        }
        let root = named(b"Roots")?;
        let gc = named(b"GC")?;
        let model = named(b"ObjectModel")?;
        let mut tokens = Vec::new();
        let mut pin = |v: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send_named(root, b"retain:", &[Operand::object(v)])?
                    .as_word()
                    .ok_or_else(invalid)?,
            );
            Ok(v)
        };
        let vt = pin(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let unit_vt = pin(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let receiver = pin(object(id.send_named(
            model,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?)?)?;
        let s = Box::into_raw(Box::new(State {
            id,
            ns,
            roots: root,
            gc,
            model,
            unit_vt,
            permanent: vec![ns, root, gc, model, receiver, vt, unit_vt],
            units: RefCell::new(HashMap::new()),
            retired: RefCell::new(Vec::new()),
            thread: std::thread::current().id(),
        }));
        for (target, n, f) in [
            (vt, b"compile:".as_slice(), compile as Method),
            (vt, b"compileObject:", compile_object),
            (vt, b"loadObject:", load_object),
            (vt, b"checkpoint", checkpoint),
            (vt, b"liveNativeUnits", live_units),
            (vt, b"rootsDo:", roots),
            (vt, b"referencesOf:do:", references),
            (vt, b"retire:", retire),
            (unit_vt, b"initialize", initialize_unit),
            (unit_vt, b"initializeWith:", initialize_unit),
            (unit_vt, b"retain:", keep),
        ] {
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(n)?),
                    Operand::entry(NativeEntry::new(f, s.cast())),
                ],
            )?;
        }
        #[cfg(feature = "resource-metrics")]
        id.send_named(
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"liveNativeBytes")?),
                Operand::entry(NativeEntry::new(live_bytes, s.cast())),
            ],
        )?;
        id.send_named(root, b"register:", &[Operand::object(receiver)])?;
        id.send_named(gc, b"registerRetirement:", &[Operand::object(receiver)])?;
        id.send_named(
            gc,
            b"trace:with:",
            &[Operand::object(unit_vt), Operand::object(receiver)],
        )?;
        for (n, v) in [
            (b"TCC".as_slice(), receiver),
            (b"TCCVt", vt),
            (b"TCCUnitVt", unit_vt),
        ] {
            id.send_named(
                ns,
                b"at:put:",
                &[Operand::object(id.intern_name(n)?), Operand::object(v)],
            )?;
        }
        for t in tokens {
            id.send_named(root, b"release:", &[Operand::word(t)])?;
        }
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        construct(*id, object(*ns)?)
    })())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
