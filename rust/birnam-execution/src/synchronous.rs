//! Shared Execution owner, synchronous Blocks, and resumable driver connection.
#[path = "resumable_runtime.rs"]
mod resumable;
use crate::{execution::ExecutionState, objects::Objects};
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
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
    ns: Oop,
    gc: Oop,
    nil: Oop,
    receiver: Oop,
    block: Oop,
    block_vt: Oop,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    storage: RefCell<Objects>,
    engine: RefCell<ExecutionState>,
    resume: RefCell<resumable::Runtime>,
    depth: Cell<usize>,
    draining: Cell<bool>,
    preserve: Cell<bool>,
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
    if s.ns != c || s.thread != std::thread::current().id() {
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
struct Invocation<'a>(&'a State);
impl Drop for Invocation<'_> {
    fn drop(&mut self) {
        self.0
            .engine
            .borrow_mut()
            .end_managed_turn()
            .expect("entered managed turn");
        self.0.depth.set(self.0.depth.get() - 1);
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
    fn field(&self, block: Oop, index: usize) -> Result<Oop, Oop> {
        self.storage
            .borrow()
            .read_payload_word(block, index)
            .map(ptr::with_exposed_provenance_mut)
            .ok_or_else(invalid)
    }
    fn fields(&self, block: Oop) -> Result<Vec<Oop>, Oop> {
        let storage = self.storage.borrow();
        let bytes = storage.payload_size(block).ok_or_else(invalid)?;
        if bytes < size_of::<Oop>() {
            return Err(invalid());
        }
        (0..bytes / size_of::<Oop>())
            .map(|i| {
                storage
                    .read_payload_word(block, i)
                    .map(ptr::with_exposed_provenance_mut)
                    .ok_or_else(invalid)
            })
            .collect()
    }
    unsafe fn make_block(&self, behavior: Oop, values: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            if values.is_empty() || values[0].is_null() {
                return Err(invalid());
            }
            let scope = self.scope(values);
            scope.keep(behavior);
            let bytes = values
                .len()
                .checked_mul(size_of::<Oop>())
                .and_then(abi::size)
                .ok_or_else(invalid)?;
            let block = self.send(behavior, b"allocate", &[bytes])?;
            let mut storage = self.storage.borrow_mut();
            storage.insert(block, behavior, values.len());
            storage.set_family(block, self.block_vt);
            for (i, v) in values.iter().enumerate() {
                assert!(storage.write_payload_word(block, i, v.expose_provenance()));
            }
            Ok(block)
        }
    }
    unsafe fn invoke(&self, method: Oop, receiver: Oop, args: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            let scope = self.scope(&[method, receiver]);
            for &arg in args {
                scope.keep(arg);
            }
            let sequence = self.send(self.id.root, b"sequence", args)?;
            self.engine
                .borrow_mut()
                .begin_managed_turn()
                .map_err(|_| invalid())?;
            self.depth.set(self.depth.get() + 1);
            let _entered = Invocation(self);
            // Explicit captured invocation is part of Block evaluation. It is not an
            // extra protocol on the Method selected by an ordinary Id send.
            self.send(method, b"invokeOn:arguments:", &[receiver, sequence])
        }
    }
    unsafe fn language_result(&self, result: Result<Oop, Oop>) -> Result<Oop, Oop> {
        unsafe {
            match result {
                Ok(v) => Ok(v),
                Err(e) if self.preserve.get() => Err(e),
                Err(e) => {
                    let scope = self.scope(&[e]);
                    let kind = if e == abi::object_reference(self.id.invalid) {
                        b"nativeInvalidRequest".as_slice()
                    } else {
                        b"nativeFailure"
                    };
                    let factory = scope.keep(self.named(b"Error")?);
                    self.send(factory, b"kind:", &[self.names[kind]])
                }
            }
        }
    }
}
unsafe extern "C" fn from_operands(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let values = borrowed_slice(a, n).ok_or_else(invalid)?;
        s.make_block(s.block_vt, values)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn value(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        let method = s.field(r, 0)?;
        s.language_result(s.invoke(method, r, args))
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn capture(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [index] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let i = abi::as_size(*index)
            .and_then(|i| i.checked_add(1))
            .ok_or_else(invalid)?;
        s.field(r, i)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn clone_block(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let fields = s.fields(r)?;
        let scope = s.scope(&fields);
        scope.keep(r);
        let behavior = s.send(s.id.root, b"behaviorOf:", &[r])?;
        s.make_block(behavior, &fields)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn capture_count(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let count = s
            .storage
            .borrow()
            .payload_size(r)
            .ok_or_else(invalid)?
            .checked_div(size_of::<Oop>())
            .and_then(|n| n.checked_sub(1))
            .ok_or_else(invalid)?;
        s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(count).ok_or_else(invalid)?],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn preserve(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [block] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let before = s.preserve.replace(true);
        let result = s.send(*block, b"value", &[]);
        s.preserve.set(before);
        result
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn method_for(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [selector] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let scope = s.scope(&[r, *selector]);
        let behavior = if abi::as_size(s.send(s.id.root, b"isBehavior:", &[r])?) == Some(1) {
            r
        } else {
            s.send(s.id.root, b"behaviorOf:", &[r])?
        };
        scope.keep(behavior);
        let method = scope.keep(s.send(behavior, b"lookup", &[*selector])?);
        if method.is_null() {
            Ok(s.nil)
        } else {
            let captured = scope.keep(s.send(method, b"captured", &[])?);
            resumable::capture_alias(s, method, captured)?;
            Ok(captured)
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn install(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method, selector] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.send(s.gc, b"prepareAllocation", &[])?;
        s.send(r, b"addMethod", &[*selector, *method])?;
        Ok(*method)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![s.ns, s.gc, s.nil, s.receiver, s.block, s.block_vt];
        values.extend_from_slice(&s.scoped.borrow());
        values.extend_from_slice(&s.resume.borrow().roots);
        for value in values {
            s.send(*v, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn references(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [block, v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        if let Some(values) = resumable::references(s, *block)? {
            for value in values {
                s.send(*v, b"visit:", &[value])?;
            }
            return Ok(ptr::null_mut());
        }
        for value in if *block == s.block {
            Vec::new()
        } else {
            s.fields(*block)?
        } {
            s.send(*v, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn retire(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let values = borrowed_slice(a, n).ok_or_else(invalid)?;
        resumable::retire(s, values)?;
        let mut storage = s.storage.borrow_mut();
        for &v in values {
            storage.remove(v);
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
        if s.depth.get() != 0 || s.draining.replace(true) {
            return Ok(ptr::null_mut());
        }
        let result = s
            .send(s.gc, b"prepareAllocation", &[])
            .and_then(|_| resumable::checkpoint(s));
        s.draining.set(false);
        result.map(|_| ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn ready(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        Ok(abi::size(usize::from(s.depth.get() == 0 && !s.draining.get())).unwrap())
    })()
    .unwrap_or_else(abi::fault_reference)
}
const NAMES: &[&[u8]] = &[
    b"Block",
    b"BlockVt",
    b"Error",
    b"Execution",
    b"GC",
    b"Integer",
    b"ObjectModel",
    b"ObjectVt",
    b"Roots",
    b"addMethod",
    b"addMethod:as:",
    b"allocate",
    b"allocate:bytes:",
    b"at:",
    b"at:put:",
    b"behaviorOf:",
    b"bind:to:",
    b"captureCount",
    b"captureWordAt:",
    b"checkpoint",
    b"clone",
    b"delegated",
    b"evaluatePreservingFailures:",
    b"fromMethod:captureOperands:",
    b"fromWord:",
    b"invokeOn:arguments:",
    b"kind:",
    b"methodAt:",
    b"methodFor:",
    b"nativeFailure",
    b"nativeInvalidRequest",
    b"nil",
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
    b"trace:with:",
    b"value",
    b"value:",
    b"value:value:",
    b"value:value:value:",
    b"value:value:value:value:",
    b"value:value:value:value:value:",
    b"value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:",
    b"visit:",
    b"withContext:",
    b"isBehavior:",
    b"lookup",
    b"captured",
    b"fromStep:descriptor:selection:owner:",
    b"resumableEntry",
    b"runBlockStep",
    b"new",
    b"runBlock:",
    b"runBlock:budget:",
    b"continueWithBudget:",
    b"destroy",
    b"executionState",
    b"isRunnable",
    b"liveFrameCount",
    b"oopAt:",
    b"oopAt:put:",
    b"continueAt:",
    b"callSite:",
    b"tailCallSite:",
    b"returnAt:",
    b"Machine",
    b"MachineVt",
    b"identityOf:",
    b"bytes",
    b"sizeWord",
    b"atWord:",
    b"wordOf:",
    b"low32",
    b"high32",
    b"sameNativeMethod:as:",
    b"doesNotUnderstand:",
    b"true",
    b"false",
    b"running",
    b"runnable",
    b"completed",
    b"suspended",
    b"failed",
    b"destroyed",
    b"continue",
    b"call",
    b"tailCall",
    b"return",
    b"suspend",
    b"capture",
    b"restore",
    b"abort",
    b"boundaryCall",
    b"exitBoundary",
    b"oop",
    b"staleFrameView",
    b"invalidFrameSlot",
    b"allocationSizeOf:",
    b"usesDefaultLookup:",
    b"nativeBindingsMatch:",
    b"nativeBindingQuerySemantics:for:",
];
static LITERALS: &[Literal] = &[
    Literal::new(b"Block"),
    Literal::new(b"BlockVt"),
    Literal::new(b"Error"),
    Literal::new(b"Execution"),
    Literal::new(b"GC"),
    Literal::new(b"Integer"),
    Literal::new(b"ObjectModel"),
    Literal::new(b"ObjectVt"),
    Literal::new(b"Roots"),
    Literal::new(b"addMethod"),
    Literal::new(b"addMethod:as:"),
    Literal::new(b"allocate"),
    Literal::new(b"allocate:bytes:"),
    Literal::new(b"at:"),
    Literal::new(b"at:put:"),
    Literal::new(b"behaviorOf:"),
    Literal::new(b"bind:to:"),
    Literal::new(b"captureCount"),
    Literal::new(b"captureWordAt:"),
    Literal::new(b"checkpoint"),
    Literal::new(b"clone"),
    Literal::new(b"delegated"),
    Literal::new(b"evaluatePreservingFailures:"),
    Literal::new(b"fromMethod:captureOperands:"),
    Literal::new(b"fromWord:"),
    Literal::new(b"invokeOn:arguments:"),
    Literal::new(b"kind:"),
    Literal::new(b"methodAt:"),
    Literal::new(b"methodFor:"),
    Literal::new(b"nativeFailure"),
    Literal::new(b"nativeInvalidRequest"),
    Literal::new(b"nil"),
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
    Literal::new(b"trace:with:"),
    Literal::new(b"value"),
    Literal::new(b"value:"),
    Literal::new(b"value:value:"),
    Literal::new(b"value:value:value:"),
    Literal::new(b"value:value:value:value:"),
    Literal::new(b"value:value:value:value:value:"),
    Literal::new(b"value:value:value:value:value:value:"),
    Literal::new(b"value:value:value:value:value:value:value:"),
    Literal::new(b"value:value:value:value:value:value:value:value:"),
    Literal::new(b"visit:"),
    Literal::new(b"withContext:"),
    Literal::new(b"isBehavior:"),
    Literal::new(b"lookup"),
    Literal::new(b"captured"),
    Literal::new(b"fromStep:descriptor:selection:owner:"),
    Literal::new(b"resumableEntry"),
    Literal::new(b"runBlockStep"),
    Literal::new(b"new"),
    Literal::new(b"runBlock:"),
    Literal::new(b"runBlock:budget:"),
    Literal::new(b"continueWithBudget:"),
    Literal::new(b"destroy"),
    Literal::new(b"executionState"),
    Literal::new(b"isRunnable"),
    Literal::new(b"liveFrameCount"),
    Literal::new(b"oopAt:"),
    Literal::new(b"oopAt:put:"),
    Literal::new(b"continueAt:"),
    Literal::new(b"callSite:"),
    Literal::new(b"tailCallSite:"),
    Literal::new(b"returnAt:"),
    Literal::new(b"Machine"),
    Literal::new(b"MachineVt"),
    Literal::new(b"identityOf:"),
    Literal::new(b"bytes"),
    Literal::new(b"sizeWord"),
    Literal::new(b"atWord:"),
    Literal::new(b"wordOf:"),
    Literal::new(b"low32"),
    Literal::new(b"high32"),
    Literal::new(b"sameNativeMethod:as:"),
    Literal::new(b"doesNotUnderstand:"),
    Literal::new(b"true"),
    Literal::new(b"false"),
    Literal::new(b"running"),
    Literal::new(b"runnable"),
    Literal::new(b"completed"),
    Literal::new(b"suspended"),
    Literal::new(b"failed"),
    Literal::new(b"destroyed"),
    Literal::new(b"continue"),
    Literal::new(b"call"),
    Literal::new(b"tailCall"),
    Literal::new(b"return"),
    Literal::new(b"suspend"),
    Literal::new(b"capture"),
    Literal::new(b"restore"),
    Literal::new(b"abort"),
    Literal::new(b"boundaryCall"),
    Literal::new(b"exitBoundary"),
    Literal::new(b"oop"),
    Literal::new(b"staleFrameView"),
    Literal::new(b"invalidFrameSlot"),
    Literal::new(b"allocationSizeOf:"),
    Literal::new(b"usesDefaultLookup:"),
    Literal::new(b"nativeBindingsMatch:"),
    Literal::new(b"nativeBindingQuerySemantics:for:"),
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 23,
        entry: Some(from_operands),
    },
    MethodSpec {
        name: 18,
        entry: Some(capture),
    },
    MethodSpec {
        name: 17,
        entry: Some(capture_count),
    },
    MethodSpec {
        name: 20,
        entry: Some(clone_block),
    },
    MethodSpec {
        name: 22,
        entry: Some(preserve),
    },
    MethodSpec {
        name: 28,
        entry: Some(method_for),
    },
    MethodSpec {
        name: 10,
        entry: Some(install),
    },
    MethodSpec {
        name: 40,
        entry: Some(roots),
    },
    MethodSpec {
        name: 33,
        entry: Some(references),
    },
    MethodSpec {
        name: 38,
        entry: Some(retire),
    },
    MethodSpec {
        name: 19,
        entry: Some(checkpoint),
    },
    MethodSpec {
        name: 39,
        entry: Some(ready),
    },
    MethodSpec {
        name: 43,
        entry: Some(value),
    },
    MethodSpec {
        name: 44,
        entry: Some(value),
    },
    MethodSpec {
        name: 45,
        entry: Some(value),
    },
    MethodSpec {
        name: 46,
        entry: Some(value),
    },
    MethodSpec {
        name: 47,
        entry: Some(value),
    },
    MethodSpec {
        name: 48,
        entry: Some(value),
    },
    MethodSpec {
        name: 49,
        entry: Some(value),
    },
    MethodSpec {
        name: 50,
        entry: Some(value),
    },
    MethodSpec {
        name: 51,
        entry: Some(value),
    },
    MethodSpec {
        name: 57,
        entry: Some(resumable::from_step),
    },
    MethodSpec {
        name: 58,
        entry: Some(resumable::entry),
    },
    MethodSpec {
        name: 59,
        entry: Some(resumable::run_step),
    },
    MethodSpec {
        name: 60,
        entry: Some(resumable::new),
    },
    MethodSpec {
        name: 61,
        entry: Some(resumable::run),
    },
    MethodSpec {
        name: 62,
        entry: Some(resumable::run_bounded),
    },
    MethodSpec {
        name: 63,
        entry: Some(resumable::continue_drive),
    },
    MethodSpec {
        name: 64,
        entry: Some(resumable::destroy),
    },
    MethodSpec {
        name: 65,
        entry: Some(resumable::lifecycle),
    },
    MethodSpec {
        name: 66,
        entry: Some(resumable::is_runnable),
    },
    MethodSpec {
        name: 67,
        entry: Some(resumable::frame_count),
    },
    MethodSpec {
        name: 68,
        entry: Some(resumable::read),
    },
    MethodSpec {
        name: 69,
        entry: Some(resumable::write),
    },
    MethodSpec {
        name: 70,
        entry: Some(resumable::continue_at),
    },
    MethodSpec {
        name: 71,
        entry: Some(resumable::call_site),
    },
    MethodSpec {
        name: 72,
        entry: Some(resumable::tail_call_site),
    },
    MethodSpec {
        name: 73,
        entry: Some(resumable::return_at),
    },
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    let id = unsafe { *p };
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
        let mut tokens = Vec::new();
        let mut keep = |v| -> Result<Oop, Oop> {
            tokens.push(send(roots, b"retain:", &[v])?);
            Ok(v)
        };
        let base = named(b"ObjectVt")?;
        let behavior = keep(send(base, b"delegated", &[])?)?;
        let block_vt = keep(send(base, b"delegated", &[])?)?;
        let receiver = keep(send(behavior, b"allocate", &[abi::size(0).unwrap()])?)?;
        let block = keep(send(block_vt, b"allocate", &[abi::size(0).unwrap()])?)?;
        let s = Box::leak(Box::new(State {
            id,
            ns: *ns,
            gc: named(b"GC")?,
            nil: named(b"nil")?,
            receiver,
            block,
            block_vt,
            names: names.clone(),
            scoped: RefCell::new(Vec::new()),
            storage: RefCell::new(Objects::default()),
            engine: RefCell::new(ExecutionState::new(1)),
            resume: RefCell::new(resumable::Runtime::default()),
            depth: Cell::new(0),
            draining: Cell::new(false),
            preserve: Cell::new(false),
            ready: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for spec in METHODS {
            let name = NAMES[spec.name];
            let target = match name {
                b"fromMethod:captureOperands:" | b"captureWordAt:" | b"captureCount" | b"clone" => {
                    block_vt
                }
                b"methodFor:" | b"addMethod:as:" => base,
                n if n.starts_with(b"value") => block_vt,
                _ if resumable::METHOD_NAMES.contains(&name) => continue,
                _ => behavior,
            };
            let method = send(image, b"methodAt:", &[names[name]])?;
            let method = send(method, b"withContext:", &[*ns])?;
            send(target, b"addMethod", &[names[name], method])?;
            if name.starts_with(b"value") {
                s.resume.borrow_mut().block_values.push(method);
            }
        }
        // The factory has no Block fields; it is a rooted ordinary object. Only actual
        // constructed Blocks are tracked and traced by this representation owner.
        send(roots, b"register:", &[receiver])?;
        send(s.gc, b"registerRetirement:", &[receiver])?;
        send(s.gc, b"trace:with:", &[block_vt, receiver])?;
        for (name, value) in [
            (b"Execution".as_slice(), receiver),
            (b"Block", block),
            (b"BlockVt", block_vt),
        ] {
            send(*ns, b"at:put:", &[names[name], value])?;
        }
        resumable::install(s, image, base)?;
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        s.ready.set(true);
        Ok(receiver)
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
