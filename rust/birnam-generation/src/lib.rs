//! Generation admission, publication, inverse ledgers and recovery.
//! All compiler and backend dependencies are live Id-message consumers.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
mod backend;
mod candidate;
mod compiler;
mod registry;
mod values;
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|v| !v.is_null()).ok_or_else(invalid)
}
fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    mutation_guard: Oop,
    running: Cell<bool>,
    permanent: Vec<Oop>,
    candidates: candidate::Candidates,
    registry: registry::Registry,
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
                    .expect("release Generation root");
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
    unsafe fn is_error(&self, o: Oop) -> Result<bool, Outcome> {
        unsafe { Ok(self.send(o, b"isError", &[])?.as_object() == Some(self.named(b"true")?)) }
    }
    unsafe fn index(&self, scope: &Scope<'_>, o: Oop, n: &[u8]) -> Result<usize, Outcome> {
        unsafe {
            let v = scope.keep(object(self.send(o, n, &[])?)?)?;
            self.send(self.named(b"Integer")?, b"wordOf:", &[Operand::object(v)])?
                .as_word()
                .filter(|i| *i <= isize::MAX as usize)
                .ok_or_else(invalid)
        }
    }
}
#[derive(Clone, Copy)]
enum Op {
    Candidate(candidate::Op),
    Registry(registry::Op),
    EnterRunning,
    AuthorizeMutation,
    ManagedChanged,
    ValidateImage,
    FulfillIntent,
    Receipt(usize),
    BeginRecovery,
    Recover,
    CancelRecovery,
    MutationAllowed,
}
struct Entry {
    state: *const State,
    op: Op,
}
unsafe extern "C" fn operation(
    c: *mut c_void,
    receiver: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let e = &*c.cast::<Entry>();
        let s = &*e.state;
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let count = match e.op {
            Op::Candidate(op) => op.count(),
            Op::Registry(op) => op.count(),
            Op::ManagedChanged => 2,
            Op::FulfillIntent | Op::BeginRecovery | Op::Recover => 1,
            _ => 0,
        };
        if a.len() != count {
            return Err(invalid());
        }
        let q = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        q.keep(receiver)?;
        for &v in a {
            q.keep(object(v)?)?;
        }
        match e.op {
            Op::Candidate(op) => candidate::operation(s, &q, op, a),
            Op::Registry(op) => registry::operation(s, &q, op, a),
            Op::EnterRunning => candidate::enter_running(s, &q),
            Op::AuthorizeMutation => candidate::authorize(s, &q),
            Op::ManagedChanged => candidate::method_changed(s, &q, object(a[0])?, object(a[1])?),
            Op::ValidateImage => candidate::validate_image(s, &q),
            Op::FulfillIntent => candidate::fulfill_intent(s, &q, object(a[0])?),
            Op::BeginRecovery => candidate::begin_recovery(s, &q, object(a[0])?),
            Op::Recover => candidate::recover_image(s, &q, object(a[0])?),
            Op::CancelRecovery => {
                let yes = s.named(b"true")?;
                candidate::cancel_recovery(s);
                s.registry.stage.take();
                Ok(Operand::object(yes))
            }
            Op::MutationAllowed => Ok(Operand::object(s.named(
                if candidate::ordinary_mutation_allowed(s) {
                    b"true"
                } else {
                    b"false"
                },
            )?)),
            Op::Receipt(index) => {
                if !s.exact(&q, receiver, b"DiscardReceiptVt")? {
                    return Err(invalid());
                }
                s.send(
                    s.named(b"ObjectModel")?,
                    b"slotAt:in:",
                    &[Operand::word(index), Operand::object(receiver)],
                )
            }
        }
    })())
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        for o in s
            .candidates
            .roots()
            .into_iter()
            .chain(s.registry.roots())
            .chain(s.permanent.iter().copied())
        {
            s.send(v, b"visit:", &[Operand::object(o)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing = id.send_named(
            ns,
            b"at:",
            &[Operand::object(id.intern_name(b"Generation")?)],
        )?;
        if existing.kind() == native::OBJECT {
            return Ok(existing);
        }
        let root = named(b"Roots")?;
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
        let receiver = pin(object(id.send_named(
            named(b"ObjectModel")?,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?)?)?;
        let receipt_vt = pin(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let model = named(b"ObjectModel")?;
        id.send_named(
            model,
            b"setLayoutOf:to:rest:",
            &[
                Operand::object(receipt_vt),
                Operand::bytes(&[1; 4]),
                Operand::word(0),
            ],
        )?;
        let receipt = pin(object(id.send_named(
            model,
            b"allocate:bytes:",
            &[
                Operand::object(receipt_vt),
                Operand::word(4 * core::mem::size_of::<Oop>()),
            ],
        )?)?)?;
        for index in 0..4 {
            id.send_named(
                model,
                b"slotAt:in:put:",
                &[
                    Operand::word(index),
                    Operand::object(receipt),
                    Operand::object(named(b"nil")?),
                ],
            )?;
        }
        let guard_vt = pin(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let mutation_guard = pin(object(id.send_named(
            model,
            b"allocate:bytes:",
            &[Operand::object(guard_vt), Operand::word(0)],
        )?)?)?;
        let s = Box::into_raw(Box::new(State {
            id,
            ns,
            roots: root,
            mutation_guard,
            running: core::cell::Cell::new(false),
            permanent: vec![
                ns,
                root,
                vt,
                receiver,
                receipt_vt,
                receipt,
                guard_vt,
                mutation_guard,
            ],
            candidates: Default::default(),
            registry: Default::default(),
            thread: std::thread::current().id(),
        }));
        for (target, n, op) in [
            (receipt_vt, b"generation".as_slice(), Op::Receipt(0)),
            (receipt_vt, b"logical".as_slice(), Op::Receipt(1)),
            (receipt_vt, b"physical".as_slice(), Op::Receipt(2)),
            (receipt_vt, b"reason".as_slice(), Op::Receipt(3)),
            (guard_vt, b"value".as_slice(), Op::AuthorizeMutation),
            (vt, b"enterRunning".as_slice(), Op::EnterRunning),
            (
                vt,
                b"authorizeManagedMethodMutation".as_slice(),
                Op::AuthorizeMutation,
            ),
            (
                vt,
                b"managedMethodChanged:on:".as_slice(),
                Op::ManagedChanged,
            ),
            (vt, b"validateImageGeneration".as_slice(), Op::ValidateImage),
            (
                vt,
                b"discardCandidate:".as_slice(),
                Op::Candidate(candidate::Op::Discard),
            ),
            (
                vt,
                b"retainedGenerationCount".as_slice(),
                Op::Candidate(candidate::Op::RetainedCount),
            ),
            (
                vt,
                b"retainedGenerationBytes".as_slice(),
                Op::Candidate(candidate::Op::RetainedBytes),
            ),
            (
                vt,
                b"mapCandidate:".as_slice(),
                Op::Candidate(candidate::Op::Map),
            ),
            (vt, b"fulfillEvolutionIntent:".as_slice(), Op::FulfillIntent),
            (
                vt,
                b"prepareActivation:plan:".as_slice(),
                Op::Candidate(candidate::Op::Prepare),
            ),
            (
                vt,
                b"discardPrepared:".as_slice(),
                Op::Candidate(candidate::Op::DiscardPrepared),
            ),
            (
                vt,
                b"commitPrepared:".as_slice(),
                Op::Candidate(candidate::Op::Commit),
            ),
            (
                vt,
                b"prepareRollback:plan:".as_slice(),
                Op::Candidate(candidate::Op::PrepareRollback),
            ),
            (
                vt,
                b"commitPreparedRollback:".as_slice(),
                Op::Candidate(candidate::Op::CommitRollback),
            ),
            (
                vt,
                b"adoptInitialGeneration:".as_slice(),
                Op::Candidate(candidate::Op::AdoptInitial),
            ),
            (
                vt,
                b"adoptComposedGeneration:".as_slice(),
                Op::Candidate(candidate::Op::AdoptComposed),
            ),
            (
                vt,
                b"imageRecoveryIdentity".as_slice(),
                Op::Candidate(candidate::Op::RecoveryIdentity),
            ),
            (
                vt,
                b"reconcileCandidate:".as_slice(),
                Op::Candidate(candidate::Op::Reconcile),
            ),
            (
                vt,
                b"sourceRegistry".as_slice(),
                Op::Registry(registry::Op::Snapshot),
            ),
            (
                vt,
                b"recordCompiledSource:named:".as_slice(),
                Op::Registry(registry::Op::Record),
            ),
            (
                vt,
                b"moduleSourceFor:".as_slice(),
                Op::Registry(registry::Op::Find),
            ),
            (
                vt,
                b"sealInitialGeneration".as_slice(),
                Op::Registry(registry::Op::Seal),
            ),
            (
                vt,
                b"prepareSourceRegistry:".as_slice(),
                Op::Registry(registry::Op::Prepare),
            ),
            (
                vt,
                b"swapSourceRegistry".as_slice(),
                Op::Registry(registry::Op::Swap),
            ),
            (vt, b"beginImageRecovery:".as_slice(), Op::BeginRecovery),
            (vt, b"recoverImageGeneration:".as_slice(), Op::Recover),
            (vt, b"cancelImageRecovery".as_slice(), Op::CancelRecovery),
            (
                vt,
                b"ordinaryMutationAllowed".as_slice(),
                Op::MutationAllowed,
            ),
            (
                vt,
                b"successorGenerationProjection:replacing:with:unit:".as_slice(),
                Op::Candidate(candidate::Op::Successor),
            ),
            (
                vt,
                b"compileCandidate:named:output:".as_slice(),
                Op::Candidate(candidate::Op::Compile),
            ),
            (
                vt,
                b"compileCandidate:named:output:selection:".as_slice(),
                Op::Candidate(candidate::Op::CompileSelected),
            ),
            (
                vt,
                b"inspectCandidate:into:".as_slice(),
                Op::Candidate(candidate::Op::Inspect),
            ),
            (
                vt,
                b"candidateMapped:".as_slice(),
                Op::Candidate(candidate::Op::Mapped),
            ),
            (
                vt,
                b"candidateContentPath:".as_slice(),
                Op::Candidate(candidate::Op::Path),
            ),
            (
                vt,
                b"sourceRegistrySealed".as_slice(),
                Op::Registry(registry::Op::Sealed),
            ),
        ] {
            let c = Box::into_raw(Box::new(Entry { state: s, op }));
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(n)?),
                    Operand::entry(NativeEntry::new(operation, c.cast())),
                ],
            )?;
        }
        id.send_named(
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"rootsDo:")?),
                Operand::entry(NativeEntry::new(roots, s.cast())),
            ],
        )?;
        id.send_named(root, b"register:", &[Operand::object(receiver)])?;
        for (n, v) in [
            (b"Generation".as_slice(), receiver),
            (b"GenerationVt", vt),
            (b"DiscardReceipt", receipt),
            (b"DiscardReceiptVt", receipt_vt),
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
