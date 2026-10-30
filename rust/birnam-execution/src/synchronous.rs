//! Shared Execution owner, synchronous Blocks, and resumable driver connection.
#[path = "oop_native.rs"]
mod native_adapters;
#[path = "oop_reflection.rs"]
mod reflection;
#[path = "native_code.rs"]
mod native_code;
#[path = "oop_numeric.rs"]
mod numeric;
#[path = "resumable_runtime.rs"]
mod resumable;
use crate::{execution::ExecutionState, objects::Objects, word_index::WordIndex};
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{
        ContinuationMethodSpec as MethodSpec, ContinuationModule as Module, ContinuationProtocol,
        Control, Literal, Protocol,
    },
};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
#[derive(Clone, Copy)]
struct CodeShape {
    parameters: usize,
    captures: usize,
}
struct State {
    pub(crate) byte_reader: abi::byte_transfer::Reader,
    id: Protocol,
    control: Control,
    native_value: Oop,
    native_method_value: Oop,
    native_answer: Oop,
    capture_seed: Oop,
    block_value_seed: Oop,
    binding_visitor_vt: Oop,
    ns: Oop,
    gc: Oop,
    nil: Oop,
    receiver: Oop,
    block: Oop,
    block_vt: Oop,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    storage: RefCell<Objects>,
    // Checked compiled-code shapes; native callables need no such description.
    admitted: RefCell<WordIndex<CodeShape>>,
    // Parsed signatures belong only to explicit native entry contexts. The
    // context's Sequence owns its executable, environment and declared Method.
    signature_contexts: RefCell<WordIndex<usize>>,
    native_signatures: RefCell<WordIndex<std::sync::Arc<abi::native_signature::Signature>>>,
    engine: RefCell<ExecutionState>,
    resume: RefCell<resumable::Runtime>,
    native_code: RefCell<native_code::Runtime>,
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
    unsafe fn transfer(&self, operation: &[u8], args: &[Oop]) -> Result<Oop, Oop> {
        let value = unsafe {
            (self.control)(
                self.id.root,
                self.names[operation],
                args.as_ptr(),
                args.len(),
            )
        };
        if abi::is_fault(value) {
            Err(abi::object_reference(value))
        } else {
            Ok(value)
        }
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
        // Execution owns this recorded extent. No callback can retire or
        // mutate its storage while this snapshot is copied.
        let words = unsafe {
            core::slice::from_raw_parts(block.cast::<usize>(), bytes / size_of::<Oop>())
        };
        Ok(words.iter().copied().map(ptr::with_exposed_provenance_mut).collect())
    }
    unsafe fn make_block(&self, behavior: Oop, values: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            if values.is_empty() || values[0].is_null() {
                return Err(invalid());
            }
            if let Some(shape) = self.admitted.borrow().get(&values[0].addr()) {
                if values.len() - 1 != shape.captures || values[1..].iter().any(|v| v.is_null()) {
                    return Err(invalid());
                }
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
                Ok(v) => Ok(if v.is_null() { self.nil } else { v }),
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
unsafe extern "C" fn invalid_request(_: Oop, c: Oop, _: Oop, _: *const Oop, _: usize) -> Oop {
    unsafe {
        match state(c) {
            Ok(s) => s.id.invalid,
            Err(e) => abi::fault_reference(e),
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
        let [k, args @ ..] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[r, *k]);
        if s.storage.borrow().image_shell(r) {
            return Err(invalid());
        }
        let method = q.keep(s.field(r, 0)?);
        if s.admitted.borrow().get(&method.addr()).is_some_and(|shape| shape.parameters != args.len()) {
            return Err(invalid());
        }
        // Block entry preserves the body's native result. Machine call completion
        // owns conversion to language Error data when a language caller awaits it.
        let mut values = vec![method, r, *k];
        values.extend_from_slice(args);
        s.transfer(b"prepareEntry", &values)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn capture(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [index] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Ok(s.nil);
        };
        let Some(i) = reflection::capture_index(s, *index)?.and_then(|i| i.checked_add(1)) else {
            return Ok(s.nil);
        };
        let storage = s.storage.borrow();
        if storage.family_of(r) != Some(s.block_vt) {
            return Ok(s.nil);
        }
        Ok(storage
            .read_payload_word(r, i)
            .map(ptr::with_exposed_provenance_mut)
            .unwrap_or(s.nil))
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
        let pending = s.storage.borrow().image_shell(r);
        let scope = s.scope(&fields);
        scope.keep(r);
        let behavior = s.send(s.id.root, b"behaviorOf:", &[r])?;
        let copy = s.make_block(behavior, &fields)?;
        s.storage.borrow_mut().set_image_shell(copy, pending);
        Ok(copy)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn capture_count(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Ok(s.nil);
        }
        let Some(count) = reflection::inspect_captures(s, r)? else {
            return Ok(s.nil);
        };
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
            // The shared value handler implements this owner's Block representation.
            // Capture its body, as before sharing that handler, only after current lookup.
            let owned_block = r != s.block && s.storage.borrow().family_of(r) == Some(s.block_vt);
            let method = if owned_block
                && s.send(
                    s.id.root,
                    b"sameNativeMethod:as:",
                    &[method, s.block_value_seed],
                )? == abi::size(1).unwrap()
            {
                scope.keep(s.field(r, 0)?)
            } else {
                method
            };
            let captured = scope.keep(s.send(method, b"captured", &[])?);
            Ok(captured)
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Explicit captured-Method invocation accepts the owner's indexed Oop view.
// This adapter is never consulted by an ordinary send to another receiver.
unsafe extern "C" fn invoke_method(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let values = borrowed_slice(a, n).ok_or_else(invalid)?;
        let q = s.scope(values);
        q.keep(r);
        let (k, receiver, arguments) = match values {
            [k, receiver] => (*k, *receiver, None),
            [k, receiver, arguments] => (*k, *receiver, Some(*arguments)),
            _ => return Err(invalid()),
        };
        let mut values = vec![r, receiver, k];
        if let Some(arguments) = arguments {
            let count = abi::as_size(s.send(arguments, b"sizeWord", &[])?).ok_or_else(invalid)?;
            values.try_reserve(count).map_err(|_| invalid())?;
            for i in 0..count {
                values.push(q.keep(s.send(
                    arguments,
                    b"atWord:",
                    &[abi::size(i).ok_or_else(invalid)?],
                )?));
            }
        }
        s.transfer(b"prepareEntry", &values)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn method_identity(_: Oop, _: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    if n == 0 {
        r
    } else {
        abi::fault_reference(invalid())
    }
}
unsafe extern "C" fn install(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method, selector] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let submitted = *method;
        let q = s.scope(&[r, submitted, *selector]);
        let method = if *selector == s.names[b"doesNotUnderstand:".as_slice()] {
            q.keep(s.send(*method, b"forDoesNotUnderstand", &[])?)
        } else {
            *method
        };
        s.send(s.gc, b"prepareAllocation", &[])?;
        s.send(r, b"addMethod", &[*selector, method])?;
        Ok(submitted)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![
            s.ns,
            s.gc,
            s.nil,
            s.receiver,
            s.block,
            s.block_vt,
            s.native_value,
            s.native_method_value,
            s.native_answer,
            s.capture_seed,
            s.block_value_seed,
            s.binding_visitor_vt,
        ];
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
        if let Some(values) = native_code::references(s, *block).or_else(|| resumable::references(s, *block)) {
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
        if values.is_empty() {
            return Ok(ptr::null_mut());
        }
        resumable::retire(s, values)?;
        native_code::retire(s, values);
        let mut storage = s.storage.borrow_mut();
        let mut admitted = s.admitted.borrow_mut();
        let mut signatures = s.native_signatures.borrow_mut();
        let mut contexts = s.signature_contexts.borrow_mut();
        for &v in values {
            storage.remove(v);
            admitted.remove(&v.addr());
            signatures.remove(&v.addr());
            contexts.remove(&v.addr());
        }
        contexts.retain(|_, context| signatures.contains_key(context));
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
    b"prepareSend",
    b"prepareEntry",
    b"answerTo:with:",
    b"drive:budget:",
    b"pause:",
    b"advanceExecution",
    b"receiveCall",
    b"receiveRun",
    b"driveBridge",
    b"faultKind",
    b"driveActive",
    b"continuationBarrier",
    b"suspendAt:continueAt:",
    b"captureAt:continueAt:",
    b"resumeWith:",
    b"resumeWith:budget:",
    b"restoreContinuation:with:",
    b"Continuation",
    b"ContinuationVt",
    b"fault:detail:",
    b"invalidMachineOwner",
    b"machineAlreadyRunning",
    b"invalidStepAction",
    b"staleStepAction",
    b"stepActionWrongMachine",
    b"missingImplementation",
    b"codeGenerationUnavailable",
    b"resumableEntryUnregistered",
    b"resumableEntryRetired",
    b"resumableEntryStaleGeneration",
    b"machineNotSuspended",
    b"continuationConsumed",
    b"continuationWrongMachine",
    b"continuationNotImageable",
    b"boundaryCallSite:",
    b"exitBoundaryAt:",
    b"boundaryDeliveryPending",
    b"acknowledgeBoundaryDelivery",
    b"exitRunnableBoundary:",
    b"controlEligible",
    b"protectedBlock:",
    b"leaveBoundary:",
    b"park",
    b"value:value:value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:",
    b"value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:",
    b"ownerOf:put:",
    b"invalidRequest",
    b"optionalWordResultOf:selector:argument:resultFactory:",
    b"objectResultOf:selector:wordArgument:",
    b"discardResultOf:selector:wordArgument:",
    b"objectResultOf:selector:byteArgument:",
    b"bytesResultOf:selector:resultFactory:",
    b"wordResultOf:selector:",
    b"bytesResultOf:selector:",
    b"block:withNativeSignature:",
    b"nativeSignatureValue",
    b"failNative:detail:",
    b"bits",
    b"fromBits:",
    b"Bytes",
    b"Word",
    b"nativeFailed:detail:",
    b"nativeCleanup",
    b"nativeReceiver",
    b"nativeSelector",
    b"nativeArguments",
    b"nativeSignature",
    b"detail",
    b"size",
    b"fromBytes:",
    b"methodBehavior",
    b"invokeOn:",
    b"codeObject",
    b"reassociateBehaviorOf:with:",
    b"binding:value:weak:",
    b"bindingsDo:",
    b"directMethodFor:in:",
    b"bindingMatchesIn:as:",
    b"sameNativeMethod:as:",
    b"fromMethod:captures:",
    b"setLayoutOf:to:rest:",
    b"slotAt:in:",
    b"slotAt:in:put:",
    b"isSynchronous",
    b"captureAt:",
    b"imageShellFromMethod:codeObject:",
    b"restoreImageCaptures:of:",
    b"fromMethod:codeObject:captures:",
    b"captureCountOf:",
    b"CodeObject",
    b"isError",
    b"withNativeSignature:",
    b"parameterCountOf:",
    b"forDoesNotUnderstand",
    b"currentFrameView",
    b"implementationIdentity",
    b"generationIdentity",
    b"sourceMapIdentity",
    b"programCounter",
    b"slotCount",
    b"frameViewOopAt:",
    b"slotKindAt:",
    b"frameViewClone",
    b"methodIdentity",
    b"sourceMapLocation",
    b"fromUnsignedLow:high:",
    b"FrameView",
    b"withInline:at:owner:",
    b"recoverAt:with:",
    b"recoverAt:withGraph:",
    b"nativeBindingsMatch:",
    b"Float",
    b"bitsOf:",
    b"allOopLayoutOf:words:",
    b"referenceOwnerOf:",
    b"nativeSignatureOf:",
    b"nativeDeferredAnswer",
    b"captureWord:of:",
    b"fromStep:descriptor:selection:owner:callPlans:",
    b"prepareCall:selector:",
    b"callPlan:field:",
    b"optimizeWith:",
    b"numericOperation:left:right:",
    b"methodFromBlock:",
    b"method:withNativeSignature:",
    b"nativeMethodValue",
    b"fromStep:descriptor:selection:owner:callPlans:codeObject:",
    b"methodCodeObject",
    b"methodSynchronous",
    b"canonicalBytesOf:",
    b"fromNativeCodeFactory:codeObject:",
    b"nativeCodeEnvironment",
    b"nativeCodeParameters",
    b"nativeCodeCaptures",
    b"environment",
    b"parameterCount",
    b"nativeCodeSource",
    b"resumableDnuEntry",
    b"methodClone",
    b"bodyOfMethod:",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"objectBehavior",
    b"releaseTokensProtecting",
    b"fromByteValues",
    b"receiveByteValues",
];
static LITERALS: &[Literal] = &abi::id_protocol::literal_table::<{ NAMES.len() }>(NAMES);
static METHODS: &[MethodSpec] = &[
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"bodyOfMethod:"), entry: Some(reflection::method_body), convention: 0 },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"numericOperation:left:right:"),
        entry: Some(numeric::evaluate),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"optimizeWith:"),
        entry: Some(resumable::native_tier::install),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"fromStep:descriptor:selection:owner:callPlans:"),
        entry: Some(resumable::from_step_plans),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"prepareCall:selector:"),
        entry: Some(resumable::call_preparation::prepared),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"callPlan:field:"),
        entry: Some(resumable::call_preparation::field),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"captureWord:of:"),
        entry: Some(reflection::capture_word_of),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"fromMethod:captureOperands:"),
        entry: Some(from_operands),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"captureWordAt:"),
        entry: Some(capture),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"captureCount"),
        entry: Some(capture_count),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"clone"),
        entry: Some(clone_block),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"evaluatePreservingFailures:"),
        entry: Some(preserve),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"methodFor:"),
        entry: Some(method_for),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"addMethod:as:"),
        entry: Some(install),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"rootsDo:"),
        entry: Some(roots),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"referencesOf:do:"),
        entry: Some(references),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"retire:"),
        entry: Some(retire),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"checkpoint"),
        entry: Some(checkpoint),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"retirementReady"),
        entry: Some(ready),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"fromStep:descriptor:selection:owner:"),
        entry: Some(resumable::from_step),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"resumableEntry"),
        entry: Some(resumable::entry),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"runBlockStep"),
        entry: Some(resumable::run_step),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"new"),
        entry: Some(resumable::new),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"runBlock:"),
        entry: Some(resumable::run),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"runBlock:budget:"),
        entry: Some(resumable::run_bounded),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"continueWithBudget:"),
        entry: Some(resumable::continue_drive),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"destroy"),
        entry: Some(resumable::destroy),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"executionState"),
        entry: Some(resumable::lifecycle),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"isRunnable"),
        entry: Some(resumable::is_runnable),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"liveFrameCount"),
        entry: Some(resumable::frame_count),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"oopAt:"),
        entry: Some(resumable::read),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"oopAt:put:"),
        entry: Some(resumable::write),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"continueAt:"),
        entry: Some(resumable::continue_at),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"callSite:"),
        entry: Some(resumable::call_site),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"tailCallSite:"),
        entry: Some(resumable::tail_call_site),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"returnAt:"),
        entry: Some(resumable::return_at),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"advanceExecution"),
        entry: Some(resumable::advance),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"receiveCall"),
        entry: Some(resumable::receive_call),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"receiveRun"),
        entry: Some(resumable::receive_run),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"driveBridge"),
        entry: Some(resumable::drive_bridge),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"suspendAt:continueAt:"),
        entry: Some(resumable::suspend_at),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"captureAt:continueAt:"),
        entry: Some(resumable::capture_at),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"resumeWith:"),
        entry: Some(resumable::resume),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"resumeWith:budget:"),
        entry: Some(resumable::resume_bounded),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"restoreContinuation:with:"),
        entry: Some(resumable::restore),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"boundaryCallSite:"),
        entry: Some(resumable::boundary_call),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"exitBoundaryAt:"),
        entry: Some(resumable::boundary_exit),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"boundaryDeliveryPending"),
        entry: Some(resumable::delivery_pending),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"acknowledgeBoundaryDelivery"),
        entry: Some(resumable::acknowledge),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"exitRunnableBoundary:"),
        entry: Some(resumable::exit_runnable),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"controlEligible"),
        entry: Some(resumable::control::eligible),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"protectedBlock:"),
        entry: Some(resumable::control::protected),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"leaveBoundary:"),
        entry: Some(resumable::control::leave),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"park"),
        entry: Some(resumable::control::park),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:value:"),
        entry: Some(value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"invalidRequest"),
        entry: Some(invalid_request),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"optionalWordResultOf:selector:argument:resultFactory:"),
        entry: Some(native_adapters::optional_word),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"objectResultOf:selector:wordArgument:"),
        entry: Some(native_adapters::object_word),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"discardResultOf:selector:wordArgument:"),
        entry: Some(native_adapters::discard_word),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"objectResultOf:selector:byteArgument:"),
        entry: Some(native_adapters::object_bytes),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"bytesResultOf:selector:resultFactory:"),
        entry: Some(native_adapters::bytes_factory),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"wordResultOf:selector:"),
        entry: Some(native_adapters::word_result),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"bytesResultOf:selector:"),
        entry: Some(native_adapters::bytes_result),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"block:withNativeSignature:"),
        entry: Some(native_adapters::with_signature),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"nativeSignatureValue"),
        entry: Some(native_adapters::value),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"failNative:detail:"),
        entry: Some(native_adapters::fail),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"invokeOn:arguments:"),
        entry: Some(invoke_method),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"invokeOn:"),
        entry: Some(invoke_method),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"codeObject"),
        entry: Some(reflection::code_object),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"captured"),
        entry: Some(reflection::captured),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"binding:value:weak:"),
        entry: Some(reflection::binding),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"directMethodFor:in:"),
        entry: Some(reflection::direct_method),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"bindingMatchesIn:as:"),
        entry: Some(reflection::binding_matches),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"fromMethod:captures:"),
        entry: Some(reflection::from_method),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"isSynchronous"),
        entry: Some(reflection::block_synchronous),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"captureAt:"),
        entry: Some(reflection::capture_at),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"imageShellFromMethod:codeObject:"),
        entry: Some(reflection::shell),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"restoreImageCaptures:of:"),
        entry: Some(reflection::restore_captures),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"fromMethod:codeObject:captures:"),
        entry: Some(reflection::from_code),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"forDoesNotUnderstand"),
        entry: Some(reflection::for_dnu),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"currentFrameView"),
        entry: Some(resumable::frame_view::current),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"implementationIdentity"),
        entry: Some(resumable::frame_view::word::<0>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"generationIdentity"),
        entry: Some(resumable::frame_view::word::<1>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"sourceMapIdentity"),
        entry: Some(resumable::frame_view::word::<2>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"programCounter"),
        entry: Some(resumable::frame_view::word::<3>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"slotCount"),
        entry: Some(resumable::frame_view::word::<4>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"frameViewOopAt:"),
        entry: Some(resumable::frame_view::read::<false>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"slotKindAt:"),
        entry: Some(resumable::frame_view::read::<true>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"frameViewClone"),
        entry: Some(invalid_request),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"withInline:at:owner:"),
        entry: Some(resumable::recovery::with_inline),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"recoverAt:with:"),
        entry: Some(resumable::recovery::recover::<false>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"recoverAt:withGraph:"),
        entry: Some(resumable::recovery::recover::<true>),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"nativeSignatureOf:"),
        entry: Some(native_adapters::native_signature),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"nativeDeferredAnswer"),
        entry: Some(native_adapters::deferred_answer),
        convention: 1,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"methodFromBlock:"),
        entry: Some(resumable::method_from_block),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"method:withNativeSignature:"),
        entry: Some(native_adapters::method_with_signature),
        convention: 0,
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"nativeMethodValue"),
        entry: Some(native_adapters::method_value),
        convention: 1,
    },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"fromStep:descriptor:selection:owner:callPlans:codeObject:"), entry: Some(resumable::from_step_code), convention: 0 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"methodCodeObject"), entry: Some(reflection::method_code), convention: 0 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"methodSynchronous"), entry: Some(reflection::method_synchronous), convention: 0 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"fromNativeCodeFactory:codeObject:"), entry:Some(native_code::from_factory), convention:0 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"nativeCodeEnvironment"), entry:Some(native_code::field::<0>), convention:0 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"nativeCodeParameters"), entry:Some(native_code::field::<1>), convention:0 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"nativeCodeCaptures"), entry:Some(native_code::field::<2>), convention:0 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"nativeCodeSource"), entry:Some(native_code::field::<3>), convention:0 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"resumableDnuEntry"), entry:Some(resumable::dnu_entry), convention:1 },
    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"methodClone"), entry:Some(method_identity), convention:0 },

    MethodSpec { name: birnam_abi::id_protocol::literal_index(NAMES, b"receiveByteValues"), entry: Some(abi::byte_transfer::receive_entry), convention: 0 },
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
        let block_value = keep(send(image, b"methodAt:", &[names[b"value".as_slice()]])?)?;
        let block_value_seed = keep(send(block_value, b"withContext:", &[*ns])?)?;
        let s = Box::leak(Box::new(State {
                byte_reader: abi::byte_transfer::Reader::new(id, image, roots, |n| names[n])?,
            id,
            control,
            native_value: keep(send(
                image,
                b"methodAt:",
                &[names[b"nativeSignatureValue".as_slice()]],
            )?)?,
            native_method_value: keep(send(
                image,
                b"methodAt:",
                &[names[b"nativeMethodValue".as_slice()]],
            )?)?,
            native_answer: keep(send(
                image,
                b"methodAt:",
                &[names[b"nativeDeferredAnswer".as_slice()]],
            )?)?,
            block_value_seed,
            capture_seed: keep(send(
                send(id.root, b"methodBehavior", &[])?,
                b"lookup",
                &[names[b"captured".as_slice()]],
            )?)?,
            binding_visitor_vt: keep(send(base, b"delegated", &[])?)?,
            ns: *ns,
            gc: named(b"GC")?,
            nil: named(b"nil")?,
            receiver,
            block,
            block_vt,
            names: names.clone(),
            scoped: RefCell::new(Vec::new()),
            storage: RefCell::new(Objects::default()),
            admitted: RefCell::new(WordIndex::default()),
            signature_contexts: RefCell::new(WordIndex::default()),
            native_signatures: RefCell::new(WordIndex::default()),
            engine: RefCell::new(ExecutionState::new(1)),
            resume: RefCell::new(resumable::Runtime::default()),
            native_code: RefCell::new(native_code::Runtime::default()),
            depth: Cell::new(0),
            draining: Cell::new(false),
            preserve: Cell::new(false),
            ready: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for spec in &METHODS[..METHODS.len()-1] {
            let name = NAMES[spec.name];
            if matches!(
                name,
                b"nativeSignatureValue"
                    | b"nativeMethodValue"
                    | b"nativeDeferredAnswer"
                    | b"methodCodeObject"
                    | b"methodSynchronous"
                    | b"nativeCodeEnvironment"
                    | b"nativeCodeParameters"
                    | b"nativeCodeCaptures"
                    | b"nativeCodeSource"
                    | b"resumableDnuEntry"
                    | b"methodClone"
                    | b"binding:value:weak:"
            ) {
                continue;
            }
            let target = match name {
                b"fromMethod:captureOperands:"
                | b"fromMethod:captures:"
                | b"captureWordAt:"
                | b"captureWord:of:"
                | b"captureCount"
                | b"clone" => block_vt,
                b"methodFor:" => base,
                b"addMethod:as:" => send(id.root, b"behaviorOf:", &[base])?,
                b"codeObject"
                | b"isSynchronous"
                | b"captureAt:"
                | b"imageShellFromMethod:codeObject:"
                | b"restoreImageCaptures:of:"
                | b"fromMethod:codeObject:captures:" => block_vt,
                b"invokeOn:"
                | b"invokeOn:arguments:"
                | b"captured"
                | b"forDoesNotUnderstand"
                | b"bindingMatchesIn:as:" => send(id.root, b"methodBehavior", &[])?,
                n if n.starts_with(b"value") => block_vt,
                _ if resumable::METHOD_NAMES.contains(&name)
                    || resumable::frame_view::METHODS
                        .iter()
                        .any(|&(_, declaration)| declaration == name) =>
                {
                    continue;
                }
                _ => behavior,
            };
            let method = send(image, b"methodAt:", &[names[name]])?;
            let method = send(method, b"withContext:", &[*ns])?;
            send(target, b"addMethod", &[names[name], method])?;
            if name == b"forDoesNotUnderstand" {
                let clone = send(image, b"methodAt:", &[names[b"methodClone".as_slice()]])?;
                let clone = send(clone, b"withContext:", &[*ns])?;
                send(target, b"addMethod", &[names[b"clone".as_slice()], clone])?;
            }
        }
        let layout = keep(send(
            id.root,
            b"sequence",
            &[abi::size(1).unwrap(), abi::size(1).unwrap()],
        )?)?;
        send(
            named(b"ObjectModel")?,
            b"setLayoutOf:to:rest:",
            &[s.binding_visitor_vt, layout, abi::size(0).unwrap()],
        )?;
        let observer = keep(send(
            image,
            b"methodAt:",
            &[names[b"binding:value:weak:".as_slice()]],
        )?)?;
        let observer = keep(send(observer, b"withContext:", &[*ns])?)?;
        send(
            s.binding_visitor_vt,
            b"addMethod",
            &[names[b"binding:value:weak:".as_slice()], observer],
        )?;
        for (selector, declaration) in [(b"codeObject".as_slice(), b"methodCodeObject".as_slice()), (b"isSynchronous".as_slice(), b"methodSynchronous".as_slice())] {
            let getter = keep(send(image, b"methodAt:", &[names[declaration]])?)?;
            let getter = keep(send(getter, b"withContext:", &[*ns])?)?;
            send(send(id.root, b"methodBehavior", &[])?, b"addMethod", &[names[selector], getter])?;
        }
        send(id.root, b"ownerOf:put:", &[block_vt, block])?;
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
        let native_context = keep(native_code::install(s, image, base)?)?;
        s.resume.borrow_mut().roots.push(native_context);
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
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
