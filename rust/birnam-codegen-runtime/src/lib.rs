//! Source compiler facade. Parsing, values, installation and code resources are
//! live message consumers; lowering and byte codecs remain private pure helpers.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use birnam_codegen::{OwnedCst, lower_program, native as emission};
use core::{cell::RefCell, ffi::c_void};
mod artifact;
mod candidate;
mod embedding;
mod image;
mod materialization;
mod source_metadata;
mod reflection;
mod source;
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
    running: core::cell::Cell<bool>,
    permanent: Vec<Oop>,
    sources: source::Sources,
    compiled_adapters: RefCell<Vec<Option<Vec<(u32, Oop)>>>>,
    candidates: candidate::Candidates,
    materializations: RefCell<Vec<materialization::Cached>>,
    image_stage: RefCell<Option<image::Stage>>,
    image_busy: core::cell::Cell<bool>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    tokens: RefCell<Vec<usize>>,
}
struct AdapterScope<'a>(&'a State);
impl Drop for AdapterScope<'_> {
    fn drop(&mut self) { self.0.compiled_adapters.borrow_mut().pop(); }
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
                    .expect("release Codegen root");
            }
        }
    }
}
impl State {
    fn adapter_scope(&self, adapters: Option<Vec<(u32, Oop)>>) -> AdapterScope<'_> {
        self.compiled_adapters.borrow_mut().push(adapters);
        AdapterScope(self)
    }
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
    unsafe fn project(
        &self,
        _scope: &Scope<'_>,
        o: Oop,
        source_len: usize,
        depth: usize,
        budget: &mut usize,
    ) -> Result<OwnedCst, Outcome> {
        unsafe {
            if depth > 4096 || *budget == 0 {
                return Err(invalid());
            }
            *budget -= 1;
            let local = Scope {
                s: self,
                tokens: RefCell::new(Vec::new()),
            };
            let scope = &local;
            scope.keep(o)?;
            let rule = scope.keep(object(self.send(o, b"ruleName", &[])?)?)?;
            let rule = self.bytes(rule)?;
            let start = self.index(scope, o, b"start")?;
            let end = self.index(scope, o, b"end")?;
            if start > end || end > source_len {
                return Err(invalid());
            }
            let count = self.index(scope, o, b"childCount")?;
            if count > *budget {
                return Err(invalid());
            }
            let mut children = Vec::new();
            for i in 0..count {
                let child_scope = Scope {
                    s: self,
                    tokens: RefCell::new(Vec::new()),
                };
                let scope = &child_scope;
                let index = scope.keep(object(self.send(
                    self.named(b"Integer")?,
                    b"fromWord:",
                    &[Operand::word(i)],
                )?)?)?;
                let child = scope.keep(object(self.send(
                    o,
                    b"childAt:",
                    &[Operand::object(index)],
                )?)?)?;
                children.push(self.project(scope, child, source_len, depth + 1, budget)?);
            }
            Ok(OwnedCst::new(&rule, start, end, children))
        }
    }
    unsafe fn native_unit(&self, scope: &Scope<'_>, c: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            let source = scope.keep(object(self.send(
                self.named(b"String")?,
                b"fromBytes:",
                &[Operand::bytes(c)],
            )?)?)?;
            let unit = scope.keep(object(self.send(
                self.named(b"TCC")?,
                b"compile:",
                &[Operand::object(source)],
            )?)?)?;
            Ok(unit)
        }
    }
    unsafe fn native_method(
        &self,
        scope: &Scope<'_>,
        c: &[u8],
        implementation: Option<Oop>,
    ) -> Result<Operand, Outcome> {
        unsafe {
            let unit = self.native_unit(scope, c)?;
            if self.is_error(unit)? {
                return Ok(Operand::object(unit));
            }
            if let Some(implementation) = implementation {
                self.send(unit, b"initializeWith:", &[Operand::object(implementation)])
            } else {
                self.send(unit, b"initialize", &[])
            }
        }
    }
}
impl State {
    unsafe fn run_method(&self, scope: &Scope<'_>, method: Oop) -> Result<Operand, Outcome> {
        unsafe {
            let captures =
                scope.keep(object(self.send(self.named(b"Vector")?, b"new", &[])?)?)?;
            let block = scope.keep(object(self.send(
                self.named(b"Block")?,
                b"fromMethod:captures:",
                &[Operand::object(method), Operand::object(captures)],
            )?)?)?;
            let machine =
                scope.keep(object(self.send(self.named(b"Machine")?, b"new", &[])?)?)?;
            self.send(machine, b"runBlock:", &[Operand::object(block)])
        }
    }
}
#[derive(Clone, Copy)]
enum Op {
    Emit,
    Compile,
    Eval,
    Define,
    Remove,
    EnterRunning,
    AuthorizeMutation,
    NameOf,
    Reflection(reflection::Op),
    Source(source::Op),
    Materialize(materialization::Op),
    Image(image::Op),
    Artifact(artifact::Op),
    Candidate(candidate::Op),
    FulfillIntent,
    Receipt(usize),
    ManagedChanged,
    ValidateImage,
}
struct Entry {
    state: *const State,
    op: Op,
}
unsafe extern "C" fn operation(c: *mut c_void, receiver: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let e = &*c.cast::<Entry>();
        let s = &*e.state;
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let count = match e.op {
            Op::Receipt(_) | Op::ValidateImage | Op::EnterRunning | Op::AuthorizeMutation => 0,
            Op::Remove => 2,
            Op::ManagedChanged => 2,
            Op::Define => 3,
            Op::Reflection(op) => op.count(),
            Op::Source(op) => op.count(),
            Op::Materialize(op) => op.count(),
            Op::Image(op) => op.count(),
            Op::Artifact(op) => op.count(),
            Op::Candidate(op) => op.count(),
            _ => 1,
        };
        if a.len() != count {
            return Err(invalid());
        }
        let scope = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        scope.keep(receiver)?;
        if let Op::Receipt(index) = e.op {
            if !s.exact(&scope, receiver, b"DiscardReceiptVt")? { return Err(invalid()); }
            return s.send(s.named(b"ObjectModel")?, b"slotAt:in:", &[Operand::word(index), Operand::object(receiver)]);
        }
        for &v in a {
            scope.keep(object(v)?)?;
        }
        if matches!(e.op, Op::EnterRunning) {
            return candidate::enter_running(s, &scope);
        }
        if matches!(e.op, Op::AuthorizeMutation) {
            return candidate::authorize(s, &scope);
        }
        if matches!(e.op, Op::ManagedChanged) {
            return candidate::method_changed(s, &scope, object(a[0])?, object(a[1])?);
        }
        if matches!(e.op, Op::ValidateImage) {
            return candidate::validate_image(s, &scope);
        }
        if let Op::Image(op) = e.op {
            return image::operation(s, &scope, op, a);
        }
        if matches!(e.op, Op::FulfillIntent) {
            return candidate::fulfill_intent(s, &scope, object(a[0])?);
        }
        if let Op::Candidate(op) = e.op {
            return candidate::operation(s, &scope, op, a);
        }
        if let Op::Artifact(op) = e.op {
            return artifact::operation(s, &scope, op, a);
        }
        if let Op::Materialize(op) = e.op {
            return materialization::operation(s, &scope, op, a);
        }
        if let Op::Source(op) = e.op {
            return source::operation(s, &scope, op, a);
        }
        if let Op::Reflection(op) = e.op {
            return reflection::operation(s, &scope, op, a);
        }
        if matches!(e.op, Op::NameOf) {
            let name = object(a[0])?;
            if !s.exact(&scope, name, b"SymbolVt")? {
                let kind = scope.keep(s.id.intern_name(b"notASymbol")?)?;
                let message = s.text(&scope, b"nameOf: expects a symbol")?;
                return s.send(
                    s.named(b"Error")?,
                    b"kind:message:",
                    &[Operand::object(kind), Operand::object(message)],
                );
            }
            let bytes = s
                .send(s.id.receiver, b"symbolBytesOf:", &[Operand::object(name)])?
                .as_bytes()
                .ok_or_else(invalid)?;
            let bytes = borrowed_bytes(bytes.pointer, bytes.length)
                .ok_or_else(invalid)?
                .to_vec();
            return Ok(Operand::object(s.text(&scope, &bytes)?));
        }
        if matches!(e.op, Op::Define | Op::Remove) && !candidate::ordinary_mutation_allowed(s) {
            return candidate::reject_mutation(s, &scope);
        }
        if matches!(e.op, Op::Remove) {
            let target = object(a[1])?;
            let behavior = if s.send(s.id.receiver, b"isBehavior:", &[a[1]])?.as_word() == Some(1) {
                target
            } else {
                scope.keep(object(s.send(s.id.receiver, b"behaviorOf:", &[a[1]])?)?)?
            };
            let store = scope.keep(s.named(b"Store")?)?;
            let result = s.send(store, b"removeMethod:on:", &[a[0], Operand::object(behavior)])?;
            return Ok(if result.as_object() == Some(store) { a[1] } else { result });
        }
        if matches!(e.op, Op::Define) {
            let selector = object(a[0])?;
            let b = s
                .send(s.id.receiver, b"symbolBytesOf:", &[a[0]])?
                .as_bytes()
                .ok_or_else(invalid)?;
            let name = borrowed_bytes(b.pointer, b.length)
                .ok_or_else(invalid)?
                .to_vec();
            if name.is_empty() {
                return Err(invalid());
            }
            let colons = name.iter().filter(|&&b| b == b':').count();
            let arity = if colons > 0 {
                colons
            } else if name[0].is_ascii_alphabetic() || name[0] == b'_' {
                0
            } else {
                1
            };
            if arity > 16 {
                return Err(invalid());
            }
            let compiled = s.compiled_adapters.borrow().last().cloned().flatten();
            let method = if let Some(adapters) = compiled {
                let Some((_, bytes)) = adapters.iter().find(|(a, _)| *a == arity as u32) else {
                    return s.error(&scope, b"compiled artifact lacks a required method adapter; rebuild it");
                };
                let unit = scope.keep(object(s.send(s.named(b"TCC")?, b"loadObject:", &[Operand::object(*bytes)])?)?)?;
                if s.is_error(unit)? { return Ok(Operand::object(unit)); }
                scope.keep(object(s.send(unit, b"initializeWith:", &[a[2]])?)?)?
            } else {
                let c = emission::compile_named(arity as u32).map_err(|_| invalid())?;
                scope.keep(object(s.native_method(&scope, &c, Some(object(a[2])?))?)?)?
            };
            if s.is_error(method)? {
                return Ok(Operand::object(method));
            }
            let behavior = scope.keep(object(s.send(
                s.id.receiver,
                b"ensurePrivateVtableFor:",
                &[a[1]],
            )?)?)?;
            let store = scope.keep(s.named(b"Store")?)?;
            let result = s.send(
                store,
                b"addMethod:on:with:",
                &[
                    Operand::object(selector),
                    Operand::object(behavior),
                    Operand::object(method),
                ],
            )?;
            return Ok(if result.as_object() == Some(store) {
                a[1]
            } else {
                result
            });
        }
        let source = object(a[0])?;
        let bytes = s.bytes(source)?;
        let cst = scope.keep(object(s.send(s.named(b"Lang")?, b"parse:", &[a[0]])?)?)?;
        if s.is_error(cst)? {
            return Ok(Operand::object(cst));
        }
        let cst = s.project(&scope, cst, bytes.len(), 0, &mut 1_000_000)?;
        let mut program = match lower_program(&cst, &bytes) {
            Ok(p) => p,
            Err(e) => return s.error(&scope, format!("source lowering: {e:?}").as_bytes()),
        };
        if let Some(error) = s.embed_program(&scope, &mut program, b"<compiler>")? {
            return Ok(Operand::object(error));
        }
        let c = match emission::compile_program(program, &bytes) {
            Ok(c) => c,
            Err(e) => return s.error(&scope, format!("native emission: {e:?}").as_bytes()),
        };
        if matches!(e.op, Op::Emit) {
            return s.send(s.named(b"String")?, b"fromBytes:", &[Operand::bytes(&c)]);
        }
        let method = scope.keep(object(s.native_method(&scope, &c, None)?)?)?;
        if s.is_error(method)? {
            return Ok(Operand::object(method));
        }
        if matches!(e.op, Op::Compile) {
            return Ok(Operand::object(method));
        }
        let _source = s.adapter_scope(None);
        s.run_method(&scope, method)
    })())
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        for value in s.candidates.roots() {
            s.id.send_named(v, b"visit:", &[Operand::object(value)])?;
        }
        let image_roots = s
            .image_stage
            .borrow()
            .as_ref()
            .map(|st| st.roots())
            .unwrap_or_default();
        for value in image_roots {
            s.id.send_named(v, b"visit:", &[Operand::object(value)])?;
        }
        let methods: Vec<_> = s
            .materializations
            .borrow()
            .iter()
            .map(|c| c.method)
            .collect();
        for method in methods {
            s.id.send_named(v, b"visit:", &[Operand::object(method)])?;
        }
        let entries = s.sources.entries.borrow().clone();
        for (name, source) in entries {
            s.id.send_named(v, b"visit:", &[Operand::object(name)])?;
            s.id.send_named(v, b"visit:", &[Operand::object(source)])?;
        }
        let adapters = s.compiled_adapters.borrow().clone();
        for unit in adapters.iter().flatten().flatten().map(|(_, unit)| unit) {
            s.id.send_named(v, b"visit:", &[Operand::object(*unit)])?;
        }
        for &o in &s.permanent {
            s.id.send_named(v, b"visit:", &[Operand::object(o)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing =
            id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Codegen")?)])?;
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
        let reflect_vt = pin(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let receiver = pin(object(id.send_named(
            named(b"ObjectModel")?,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?)?)?;
        let reflect = pin(object(id.send_named(
            named(b"ObjectModel")?,
            b"allocate:bytes:",
            &[Operand::object(reflect_vt), Operand::word(0)],
        )?)?)?;
        let receipt_vt = pin(object(id.send_named(named(b"ObjectVt")?, b"delegated", &[])?)?)?;
        let model = named(b"ObjectModel")?;
        id.send_named(model, b"setLayoutOf:to:rest:", &[Operand::object(receipt_vt), Operand::bytes(&[1; 4]), Operand::word(0)])?;
        let receipt = pin(object(id.send_named(model, b"allocate:bytes:", &[Operand::object(receipt_vt), Operand::word(4 * core::mem::size_of::<Oop>())])?)?)?;
        for index in 0..4 {
            id.send_named(model, b"slotAt:in:put:", &[Operand::word(index), Operand::object(receipt), Operand::object(named(b"nil")?)])?;
        }
        let guard_vt = pin(object(id.send_named(named(b"ObjectVt")?, b"delegated", &[])?)?)?;
        let mutation_guard = pin(object(id.send_named(model, b"allocate:bytes:", &[Operand::object(guard_vt), Operand::word(0)])?)?)?;
        let s = Box::into_raw(Box::new(State {
            id,
            ns,
            roots: root,
            mutation_guard,
            running: core::cell::Cell::new(false),
            permanent: vec![ns, root, vt, reflect_vt, receiver, reflect, receipt_vt, receipt, guard_vt, mutation_guard],
            sources: source::Sources::default(),
            compiled_adapters: RefCell::new(Vec::new()),
            candidates: Default::default(),
            materializations: RefCell::new(Vec::new()),
            image_stage: RefCell::new(None),
            image_busy: core::cell::Cell::new(false),
            thread: std::thread::current().id(),
        }));
        for (target, n, op) in [
            (receipt_vt, b"generation".as_slice(), Op::Receipt(0)),
            (receipt_vt, b"logical", Op::Receipt(1)),
            (receipt_vt, b"physical", Op::Receipt(2)),
            (receipt_vt, b"reason", Op::Receipt(3)),
            (guard_vt, b"value", Op::AuthorizeMutation),
            (vt, b"enterRunning", Op::EnterRunning),
            (vt, b"authorizeManagedMethodMutation", Op::AuthorizeMutation),
            (reflect_vt, b"removeMethod:on:", Op::Remove),
            (vt, b"managedMethodChanged:on:", Op::ManagedChanged),
            (vt, b"validateImageGeneration", Op::ValidateImage),
            (vt, b"discardCandidate:", Op::Candidate(candidate::Op::Discard)),
            (vt, b"retainedGenerationCount", Op::Candidate(candidate::Op::RetainedCount)),
            (vt, b"retainedGenerationBytes", Op::Candidate(candidate::Op::RetainedBytes)),
            (vt, b"emit:", Op::Emit),
            (reflect_vt, b"nameOf:", Op::NameOf),
            (
                reflect_vt,
                b"vtableOf:",
                Op::Reflection(reflection::Op::Behavior),
            ),
            (
                reflect_vt,
                b"globalNamed:",
                Op::Reflection(reflection::Op::Global),
            ),
            (
                reflect_vt,
                b"allGlobalNames",
                Op::Reflection(reflection::Op::Names),
            ),
            (
                reflect_vt,
                b"allMethodsOf:",
                Op::Reflection(reflection::Op::Methods),
            ),
            (vt, b"compile:", Op::Compile),
            (vt, b"compile:output:", Op::Artifact(artifact::Op::Compile)),
            (
                vt,
                b"compile:output:from:",
                Op::Artifact(artifact::Op::From),
            ),
            (
                vt,
                b"compile:output:root:",
                Op::Artifact(artifact::Op::Root),
            ),
            (
                vt,
                b"successorGenerationProjection:replacing:with:unit:",
                Op::Candidate(candidate::Op::Successor),
            ),
            (vt, b"load:", Op::Artifact(artifact::Op::Load)),
            (
                vt,
                b"compileCandidate:named:output:",
                Op::Candidate(candidate::Op::Compile),
            ),
            (
                vt,
                b"compileCandidate:named:output:selection:",
                Op::Candidate(candidate::Op::CompileSelected),
            ),
            (
                vt,
                b"inspectCandidate:into:",
                Op::Candidate(candidate::Op::Inspect),
            ),
            (vt, b"mapCandidate:", Op::Candidate(candidate::Op::Map)),
            (vt, b"fulfillEvolutionIntent:", Op::FulfillIntent),
            (vt, b"prepareActivation:plan:", Op::Candidate(candidate::Op::Prepare)),
            (vt, b"discardPrepared:", Op::Candidate(candidate::Op::DiscardPrepared)),
            (vt, b"commitPrepared:", Op::Candidate(candidate::Op::Commit)),
            (vt, b"prepareRollback:plan:", Op::Candidate(candidate::Op::PrepareRollback)),
            (vt, b"commitPreparedRollback:", Op::Candidate(candidate::Op::CommitRollback)),
            (vt, b"adoptInitialGeneration:", Op::Candidate(candidate::Op::AdoptInitial)),
            (vt, b"adoptComposedGeneration:", Op::Candidate(candidate::Op::AdoptComposed)),
            (vt, b"imageRecoveryIdentity", Op::Candidate(candidate::Op::RecoveryIdentity)),
            (vt, b"reconcileCandidate:", Op::Candidate(candidate::Op::Reconcile)),
            (
                vt,
                b"candidateMapped:",
                Op::Candidate(candidate::Op::Mapped),
            ),
            (
                vt,
                b"candidateContentPath:",
                Op::Candidate(candidate::Op::Path),
            ),
            (
                vt,
                b"materializeSynchronous:captures:",
                Op::Materialize(materialization::Op::Synchronous),
            ),
            (
                vt,
                b"materializeResumable:captures:",
                Op::Materialize(materialization::Op::Resumable),
            ),
            (
                vt,
                b"materializationCount",
                Op::Materialize(materialization::Op::Count),
            ),
            (
                vt,
                b"evictMaterializations",
                Op::Materialize(materialization::Op::Evict),
            ),
            (vt, b"eval:", Op::Eval),
            (vt, b"sourceRevisionOf:", Op::Source(source::Op::Revision)),
            (vt, b"moduleMetadataOf:", Op::Source(source::Op::Metadata)),
            (
                vt,
                b"embeddedFilesOf:",
                Op::Source(source::Op::EmbeddedFiles),
            ),
            (
                vt,
                b"recordModuleSource:named:",
                Op::Source(source::Op::Record),
            ),
            (vt, b"moduleSourceFor:", Op::Source(source::Op::Find)),
            (
                vt,
                b"recordedMethodSources",
                Op::Source(source::Op::MethodSources),
            ),
            (
                vt,
                b"initialGenerationProjection",
                Op::Source(source::Op::InitialProjection),
            ),
            (vt, b"sealInitialGeneration", Op::Source(source::Op::Seal)),
            (vt, b"lastError", Op::Source(source::Op::LastError)),
            (vt, b"evalPersisting:", Op::Source(source::Op::Eval)),
            (
                vt,
                b"evalPersisting:from:",
                Op::Source(source::Op::EvalFrom),
            ),
            (vt, b"portableImageBindings", Op::Image(image::Op::Bindings)),
            (vt, b"portableImageSources", Op::Image(image::Op::Sources)),
            (
                vt,
                b"preparePortableImageSources:",
                Op::Image(image::Op::PrepareSources),
            ),
            (
                vt,
                b"swapPortableImageSources",
                Op::Image(image::Op::SwapSources),
            ),
            (
                vt,
                b"compilePortableImageCode:bindings:selection:",
                Op::Image(image::Op::Compile),
            ),
            (
                vt,
                b"beginPortableImageRebuild:bindings:selection:",
                Op::Image(image::Op::Rebuild),
            ),
            (
                vt,
                b"beginPortableImageMapped:programs:bindings:selection:",
                Op::Image(image::Op::Mapped),
            ),
            (
                vt,
                b"portableImageBlockMethod:synchronous:",
                Op::Image(image::Op::Method),
            ),
            (
                vt,
                b"preparePortableImageBinding:implementation:",
                Op::Image(image::Op::Prepare),
            ),
            (
                vt,
                b"installPortableImageBinding:on:as:source:",
                Op::Image(image::Op::Install),
            ),
            (vt, b"finishPortableImageUnit", Op::Image(image::Op::Finish)),
            (vt, b"cancelPortableImageUnit", Op::Image(image::Op::Cancel)),
            (vt, b"recoverPortableImageGeneration:", Op::Image(image::Op::Recover)),
            (vt, b"beginPortableImageRecovery:", Op::Image(image::Op::BeginRecovery)),
            (reflect_vt, b"defineMethod:on:as:", Op::Define),
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
            (b"Codegen".as_slice(), receiver),
            (b"RustCodegen", receiver),
            (b"CodegenVt", vt),
            (b"Reflect", reflect),
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
