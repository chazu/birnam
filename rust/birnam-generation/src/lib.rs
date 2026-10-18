//! Generation admission, publication, inverse ledgers and image application.
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
    ValidateImage,
    PrepareImageSnapshot,
    BeginImageApplication,
    FinishImageApplication,
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
            Op::ValidateImage => candidate::validate_image(s, &q),
            Op::PrepareImageSnapshot => candidate::prepare_image_snapshot(s, &q),
            Op::BeginImageApplication => candidate::begin_image_application(s, &q),
            Op::FinishImageApplication => candidate::finish_image_application(s, &q),

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
        let s = Box::into_raw(Box::new(State {
            id,
            ns,
            roots: root,
            permanent: vec![ns, root, vt, receiver],
            candidates: Default::default(),
            registry: Default::default(),
            thread: std::thread::current().id(),
        }));
        for (target, n, op) in [
            (vt, b"watchCandidateRetirement:".as_slice(), Op::Candidate(candidate::Op::Watch)),
            (vt, b"removeCandidateFiles:".as_slice(), Op::Candidate(candidate::Op::RemoveFiles)),
            (vt, b"validateImageGeneration".as_slice(), Op::ValidateImage),
            (vt, b"prepareImageSnapshot".as_slice(), Op::PrepareImageSnapshot),
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
            (vt, b"releaseCertification".as_slice(), Op::Candidate(candidate::Op::ReleaseCertification)),
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
                b"prepareSourceRegistry:".as_slice(),
                Op::Registry(registry::Op::Prepare),
            ),
            (
                vt,
                b"installSourceRegistry".as_slice(),
                Op::Registry(registry::Op::Install),
            ),
            (vt, b"beginImageApplication".as_slice(), Op::BeginImageApplication),
            (vt, b"finishImageApplication".as_slice(), Op::FinishImageApplication),
            (vt, b"compilationForSource:", Op::Registry(registry::Op::FindCompilation)),
            (vt, b"replaceCompiledSource:named:", Op::Registry(registry::Op::Replace)),
            (
                vt,
                b"activeCandidateFor:".as_slice(),
                Op::Candidate(candidate::Op::Active),
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
