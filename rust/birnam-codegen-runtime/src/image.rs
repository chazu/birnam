//! Image staging owns copied inputs and constructed Methods. Other owners are
//! reached through Id; mapped bodies never fall back to the compiler.
use super::*;
use birnam_codegen::{backend_request::Request, image::Plan};
#[derive(Clone)]
enum Body {
    Object {
        bytes: Oop,
        range: std::ops::Range<usize>,
    },
    Rebuild(Request),
    Ready(Oop),
}
pub(super) struct Stage {
    bodies: Vec<Body>,
    templates: Vec<Oop>,
    prepared: Vec<Option<Oop>>,
    finished: bool,
    source_owner: Option<Oop>,
}
impl Stage {
    pub(super) fn roots(&self) -> Vec<Oop> {
        self.bodies
            .iter()
            .filter_map(|v| match v {
                Body::Ready(m) => Some(*m),
                Body::Object { bytes, .. } => Some(*bytes),
                _ => None,
            })
            .chain(self.templates.iter().copied())
            .chain(self.prepared.iter().flatten().copied())
            .chain(self.source_owner)
            .collect()
    }
}
#[derive(Clone, Copy)]
enum Op {
    Bindings,
    Compile,
    Rebuild,
    Mapped,
    Method,
    Prepare,
    Install,
    Finish,
    Cancel,
    Sources,
    PrepareSources,
    FinishSources,
    Prepared,
    BeginApplication,
}
unsafe fn call(s: &State, q: &Scope<'_>, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
    unsafe { Ok(q.keep(s.send(r, n, a)?)) }
}
unsafe fn blob(s: &State, q: &Scope<'_>, bytes: &[u8]) -> Result<Oop, Oop> {
    unsafe {
        let mut chunks = bytes.chunks(8192);
        let values = |bytes: &[u8]| {
            bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect::<Vec<_>>()
        };
        let first = values(chunks.next().unwrap_or(&[]));
        let mut output = call(s, q, s.named(b"Bytes")?, b"fromByteValues", &first)?;
        for chunk in chunks {
            let next = s.send(output, b"appendByteValues", &values(chunk))?;
            if next != output {
                q.keep(next);
            }
            output = next;
        }
        Ok(output)
    }
}
unsafe fn at(s: &State, q: &Scope<'_>, r: Oop, i: usize) -> Result<Oop, Oop> {
    unsafe { compiled_source::field(s, q, r, i) }
}
unsafe fn number(s: &State, q: &Scope<'_>, i: usize) -> Result<Oop, Oop> {
    unsafe {
        call(
            s,
            q,
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(i).ok_or_else(invalid)?],
        )
    }
}
unsafe fn fail(s: &State, q: &Scope<'_>, kind: &[u8], message: &[u8]) -> Result<Oop, Oop> {
    unsafe {
        let kind = q.keep(s.intern(kind)?);
        let text = q.keep(s.text(message)?);
        s.send(s.named(b"Error")?, b"kind:message:", &[kind, text])
    }
}
unsafe fn generation(s: &State, q: &Scope<'_>, name: &[u8]) -> Result<Oop, Oop> {
    unsafe { call(s, q, s.named(b"Generation")?, name, &[]) }
}
pub(super) unsafe fn emit(s: &State, q: &Scope<'_>, request: &Request) -> Result<Oop, Oop> {
    unsafe {
        let Request::Program {
            program,
            source,
            filename,
            persistent,
            synchronous,
        } = request
        else {
            return Err(invalid());
        };
        let p = blob(s, q, program)?;
        let source = q.keep(s.text(source)?);
        let filename = q.keep(s.text(filename)?);
        let persistent = s.named(if *persistent { b"true" } else { b"false" })?;
        let sync = s.named(if *synchronous { b"true" } else { b"false" })?;
        call(
            s,
            q,
            s.named(b"NativeBackend")?,
            b"emit:source:filename:persistent:synchronous:",
            &[p, source, filename, persistent, sync],
        )
    }
}
// Callbacks only copy references into this owner's active scope. They do not
// allocate managed objects or inspect foreign payloads while listing allocations.
pub(super) unsafe extern "C" fn allocation(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [object, _, _] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.scoped.borrow_mut().push(*object);
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn binding(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [selector, method, _] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.scoped
            .borrow_mut()
            .extend_from_slice(&[*selector, *method]);
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe fn direct_bindings(
    s: &State,
    q: &Scope<'_>,
    behavior: Oop,
) -> Result<Vec<(Oop, Oop)>, Oop> {
    unsafe {
        q.keep(behavior);
        let start = s.scoped.borrow().len();
        s.send(behavior, b"bindingsDo:", &[s.receiver])?;
        let entries = s.scoped.borrow()[start..].to_vec();
        if entries.len() % 2 != 0 {
            return Err(invalid());
        }
        Ok(entries
            .chunks_exact(2)
            .map(|row| (row[0], row[1]))
            .collect())
    }
}
unsafe fn bindings(s: &State, q: &Scope<'_>) -> Result<Oop, Oop> {
    unsafe {
        let start = s.scoped.borrow().len();
        s.send(s.id.root, b"allocationsDo:", &[s.receiver])?;
        let objects = s.scoped.borrow()[start..].to_vec();
        let nil = s.named(b"nil")?;
        let mut rows = Vec::new();
        for object in objects {
            if abi::as_size(s.send(s.id.root, b"isBehavior:", &[object])?) != Some(1) {
                continue;
            }
            let owner = call(s, q, s.id.root, b"ownerOf:", &[object])?;
            let target = if !owner.is_null()
                && owner != nil
                && s.send(s.id.root, b"behaviorOf:", &[owner])? == object
            {
                owner
            } else if !s.send(s.ns, b"nameOf:", &[object])?.is_null() {
                object
            } else {
                continue;
            };
            q.keep(target);
            for (selector, method) in direct_bindings(s, q, object)? {
                let implementation = call(s, q, s.receiver, b"implementationOf:", &[method])?;
                if implementation == nil {
                    continue;
                }
                if s.is_error(implementation)? {
                    return Ok(implementation);
                }
                let sync = call(s, q, method, b"isSynchronous", &[])?;
                rows.push(s.vector(q, &[target, selector, implementation, nil, sync])?);
            }
        }
        s.vector(q, &rows)
    }
}
unsafe fn operation(s: &State, q: &Scope<'_>, op: Op, args: &[Oop]) -> Result<Oop, Oop> {
    unsafe {
        if s.image_busy.replace(true) {
            return Err(invalid());
        }
        struct Busy<'a>(&'a Cell<bool>);
        impl Drop for Busy<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _busy = Busy(&s.image_busy);
        let nil = q.keep(s.named(b"nil")?);
        let yes = q.keep(s.named(b"true")?);
        match op {
            Op::Bindings => bindings(s, q),
            Op::Sources => {
                let owner = s.source_owner(q)?;
                let checked = call(s, q, owner, b"validate", &[])?;
                if checked != owner {
                    return Ok(checked);
                }
                let tag = q.keep(s.intern(b"sourceRegistryV2")?);
                s.vector(q, &[tag, owner])
            }
            Op::PrepareSources => {
                let source = args[0];
                if s.index(call(s, q, source, b"size", &[])?)? != 2 {
                    return Err(invalid());
                }
                if at(s, q, source, 0)? != s.intern(b"sourceRegistryV2")? {
                    return Err(invalid());
                }
                let owner = at(s, q, source, 1)?;
                if owner == nil {
                    return Err(invalid());
                }
                let mut stage = s.image_stage.borrow_mut();
                let stage = stage.as_mut().ok_or_else(invalid)?;
                if stage.finished {
                    return Err(invalid());
                }
                stage.source_owner = Some(owner);
                Ok(yes)
            }
            Op::FinishSources => {
                let owner = {
                    let stage = s.image_stage.borrow();
                    let stage = stage.as_ref().ok_or_else(invalid)?;
                    if !stage.finished {
                        return Err(invalid());
                    }
                    stage.source_owner.ok_or_else(invalid)?
                };
                q.keep(owner);
                if s.source_owner(q)? != owner {
                    return Err(invalid());
                }
                let checked = call(s, q, owner, b"validate", &[])?;
                Ok(if checked == owner { yes } else { checked })
            }
            Op::BeginApplication => {
                if !s.image_stage.borrow().as_ref().is_some_and(|s| s.finished) {
                    return fail(
                        s,
                        q,
                        b"imageApplicationRequired",
                        b"image application requires a prepared Image stage",
                    );
                }
                generation(s, q, b"beginImageApplication")
            }
            Op::Compile | Op::Rebuild | Op::Mapped => {
                if !matches!(op, Op::Compile) && s.image_stage.borrow().is_some() {
                    return Err(invalid());
                }
                let offset = usize::from(matches!(op, Op::Mapped));
                let p = s.bytes(args[offset])?;
                let b = s.bytes(args[offset + 1])?;
                let selection = s.bytes(args[offset + 2])?;
                let Some(plan) = Plan::new(&p, &b, &selection, false) else {
                    return Ok(nil);
                };
                if matches!(op, Op::Compile) {
                    let mut objects = Vec::new();
                    for request in &plan.requests[..2 * plan.programs] {
                        let input = s.scope(&[]);
                        let q = &input;
                        let source = emit(s, q, request)?;
                        if s.is_error(source)? {
                            return Ok(source);
                        }
                        let object = call(s, q, s.named(b"TCC")?, b"compileObject:", &[source])?;
                        if s.is_error(object)? {
                            return Ok(object);
                        }
                        objects.push(s.bytes(object)?);
                    }
                    return blob(s, q, &plan.encode(&objects).ok_or_else(invalid)?);
                }
                let bodies = if matches!(op, Op::Mapped) {
                    let view = call(s, q, args[0], b"bytes", &[])?;
                    let bytes = call(s, q, view, b"clone", &[])?;
                    let length =
                        abi::as_size(s.send(bytes, b"sizeWord", &[])?).ok_or_else(invalid)?;
                    let mut failure = None;
                    let ranges = plan
                        .decode_ranges(length, |range| {
                            let result = (|| -> Result<Vec<u8>, Oop> {
                                range
                                    .map(|i| {
                                        let v = s.send(
                                            bytes,
                                            b"atWord:",
                                            &[abi::size(i).ok_or_else(invalid)?],
                                        )?;
                                        abi::as_size(v)
                                            .and_then(|n| u8::try_from(n).ok())
                                            .ok_or_else(invalid)
                                    })
                                    .collect()
                            })();
                            match result {
                                Ok(v) => Some(v),
                                Err(e) => {
                                    failure = Some(e);
                                    None
                                }
                            }
                        })
                        .ok_or_else(|| failure.unwrap_or_else(invalid))?;
                    ranges
                        .into_iter()
                        .map(|range| Body::Object { bytes, range })
                        .collect()
                } else {
                    plan.requests[..2 * plan.programs]
                        .iter()
                        .cloned()
                        .map(Body::Rebuild)
                        .collect()
                };
                let mut stage = Stage {
                    bodies,
                    templates: Vec::new(),
                    prepared: vec![None; plan.bindings],
                    finished: false,
                    source_owner: None,
                };
                // These are ordinary current Codegen constructors, with no redundant C units.
                for request in &plan.requests[2 * plan.programs..] {
                    let Request::Named { arity, synchronous } = request else {
                        return Err(invalid());
                    };
                    let arity = number(s, q, *arity as usize)?;
                    let sync = s.named(if *synchronous { b"true" } else { b"false" })?;
                    let template = call(
                        s,
                        q,
                        s.receiver,
                        b"methodTemplateForArity:synchronous:",
                        &[arity, sync],
                    )?;
                    if template == nil || s.is_error(template)? {
                        return Ok(template);
                    }
                    stage.templates.push(template);
                }
                *s.image_stage.borrow_mut() = Some(stage);
                Ok(yes)
            }
            Op::Method => {
                let i = s.index(args[0])?;
                let index = i
                    .checked_mul(2)
                    .and_then(|v| v.checked_add(usize::from(args[1] == yes)))
                    .ok_or_else(invalid)?;
                let body = s
                    .image_stage
                    .borrow()
                    .as_ref()
                    .and_then(|v| v.bodies.get(index))
                    .cloned()
                    .ok_or_else(invalid)?;
                let unit = match body {
                    Body::Ready(method) => return Ok(method),
                    Body::Object { bytes, range } => {
                        let unit = {
                            let input = s.scope(&[]);
                            let data = call(
                                s,
                                &input,
                                bytes,
                                b"sliceFromWord:toWord:",
                                &[
                                    abi::size(range.start).ok_or_else(invalid)?,
                                    abi::size(range.end).ok_or_else(invalid)?,
                                ],
                            )?;
                            s.send(s.named(b"TCC")?, b"loadObject:", &[data])?
                        };
                        q.keep(unit)
                    }
                    Body::Rebuild(request) => {
                        let source = emit(s, q, &request)?;
                        if s.is_error(source)? {
                            // Emission refusal answers nil and leaves this body retryable.
                            // Loader and initializer Errors retain their own result convention.
                            return Ok(nil);
                        }
                        call(s, q, s.named(b"TCC")?, b"compile:", &[source])?
                    }
                };
                if s.is_error(unit)? {
                    return Ok(unit);
                }
                let method = call(s, q, unit, b"initialize", &[])?;
                if method == nil || s.is_error(method)? {
                    return Ok(method);
                }
                s.image_stage
                    .borrow_mut()
                    .as_mut()
                    .ok_or_else(invalid)?
                    .bodies[index] = Body::Ready(method);
                Ok(method)
            }
            Op::Prepare => {
                let i = s.index(args[0])?;
                let template = {
                    let stage = s.image_stage.borrow();
                    let stage = stage.as_ref().ok_or_else(invalid)?;
                    if stage.finished {
                        return Err(invalid());
                    }
                    *stage.templates.get(i).ok_or_else(invalid)?
                };
                q.keep(template);
                let method = call(
                    s,
                    q,
                    s.reflect,
                    b"methodForTemplate:implementation:",
                    &[template, args[1]],
                )?;
                if method == nil || s.is_error(method)? {
                    return Ok(method);
                }
                s.image_stage
                    .borrow_mut()
                    .as_mut()
                    .ok_or_else(invalid)?
                    .prepared[i] = Some(method);
                Ok(yes)
            }
            Op::Prepared => {
                let i = s.index(args[0])?;
                s.image_stage
                    .borrow()
                    .as_ref()
                    .and_then(|s| s.prepared.get(i))
                    .copied()
                    .flatten()
                    .ok_or_else(invalid)
            }
            Op::Finish => {
                let mut stage = s.image_stage.borrow_mut();
                let stage = stage.as_mut().ok_or_else(invalid)?;
                if stage.prepared.iter().any(Option::is_none) {
                    return Ok(nil);
                }
                stage.finished = true;
                Ok(yes)
            }
            Op::Install => {
                let i = s.index(args[0])?;
                let method = {
                    let stage = s.image_stage.borrow();
                    let stage = stage.as_ref().filter(|v| v.finished).ok_or_else(invalid)?;
                    stage
                        .prepared
                        .get(i)
                        .copied()
                        .flatten()
                        .ok_or_else(invalid)?
                };
                q.keep(method);
                let behavior = call(s, q, s.id.root, b"ensurePrivateVtableFor:", &[args[1]])?;
                let result = s.send(behavior, b"addMethod:as:", &[method, args[2]])?;
                Ok(if result == method { args[1] } else { result })
            }
            Op::Cancel => {
                generation(s, q, b"finishImageApplication")?;
                s.image_stage.borrow_mut().take();
                Ok(yes)
            }
        }
    }
}
macro_rules! entries{($(($f:ident,$op:ident,$n:expr)),*$(,)?)=>{$(pub(super) unsafe extern "C" fn $f(_:Oop,c:Oop,_:Oop,a:*const Oop,n:usize)->Oop{(||unsafe{let s=state(c)?;let a=borrowed_slice(a,n).filter(|v|v.len()==$n).ok_or_else(invalid)?;let q=s.scope(a);operation(s,&q,Op::$op,a)})().unwrap_or_else(abi::fault_reference)})*};}
entries![
    (bindings_entry, Bindings, 0),
    (compile, Compile, 3),
    (rebuild, Rebuild, 3),
    (mapped, Mapped, 4),
    (method, Method, 2),
    (prepare, Prepare, 2),
    (install, Install, 4),
    (finish, Finish, 0),
    (cancel, Cancel, 0),
    (sources, Sources, 0),
    (prepare_sources, PrepareSources, 1),
    (finish_sources, FinishSources, 0),
    (prepared, Prepared, 1),
    (begin_application, BeginApplication, 0)
];
