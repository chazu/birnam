//! Source-described native code. Constructors bind actual provider entries;
//! invocation never passes through a forwarding Method in this owner.
use super::*;
use std::rc::Rc;
struct Code {
    factory: Oop,
    source: Oop,
    parameters: usize,
    captures: usize,
}
#[derive(Clone)]
struct Instance {
    method: Oop,
    code: Rc<Code>,
    environment: Option<Oop>,
}
#[derive(Default)]
pub(super) struct Runtime {
    context_vt: Oop,
    instances: HashMap<usize, Instance>,
}
pub(super) unsafe fn install(s: &State, image: Oop, base: Oop) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[image, base]);
        let vt = q.keep(s.send(base, b"delegated", &[])?);
        for (name, declaration) in [
            (
                b"environment".as_slice(),
                b"nativeCodeEnvironment".as_slice(),
            ),
            (b"parameterCount", b"nativeCodeParameters"),
            (b"captureCount", b"nativeCodeCaptures"),
            (b"codeObject", b"nativeCodeSource"),
        ] {
            let m = q.keep(s.send(image, b"methodAt:", &[s.names[declaration]])?);
            let m = q.keep(s.send(m, b"withContext:", &[s.ns])?);
            s.send(vt, b"addMethod", &[s.names[name], m])?;
        }
        s.send(s.gc, b"trace:with:", &[vt, s.receiver])?;
        s.native_code.borrow_mut().context_vt = vt;
        Ok(vt)
    }
}
unsafe fn construct(
    s: &State,
    code: Rc<Code>,
    environment: Option<Oop>,
    prefix: bool,
) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[code.factory, code.source]);
        if let Some(env) = environment {
            q.keep(env);
        }
        let vt = s.native_code.borrow().context_vt;
        let context = q.keep(s.send(vt, b"allocate", &[abi::size(0).unwrap()])?);
        s.storage.borrow_mut().insert(context, vt, 0);
        // Keep data usable if a replaceable constructor reenters or escapes a
        // Method before returning. A failed construction is reclaimed by GC.
        s.native_code.borrow_mut().instances.insert(
            context.addr(),
            Instance {
                method: ptr::null_mut(),
                code: code.clone(),
                environment,
            },
        );
        let mode = abi::size(if prefix {
            2
        } else {
            usize::from(environment.is_some())
        })
        .unwrap();
        let method = q.keep(s.invoke(code.factory, s.receiver, &[context, mode])?);
        if s.send(s.id.root, b"sameNativeMethod:as:", &[method, method])? != abi::size(1).unwrap() {
            return Err(invalid());
        }
        s.native_code
            .borrow_mut()
            .instances
            .get_mut(&context.addr())
            .ok_or_else(invalid)?
            .method = method;
        if environment.is_none() {
            s.admitted.borrow_mut().insert(
                method.addr(),
                CodeShape {
                    parameters: code.parameters,
                    captures: code.captures,
                },
            );
        }
        Ok(method)
    }
}
pub(super) unsafe extern "C" fn from_factory(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [factory, source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let _q = s.scope(&[*factory, *source]);
        let owner = s.named(b"CodeObject")?;
        let parameters = resumable::index(s, s.send(owner, b"parameterCountOf:", &[*source])?)?;
        let captures = resumable::index(s, s.send(owner, b"captureCountOf:", &[*source])?)?;
        construct(
            s,
            Rc::new(Code {
                factory: *factory,
                source: *source,
                parameters,
                captures,
            }),
            None,
            false,
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn instance(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<Instance>, Oop> {
    unsafe {
        if let Some(v) = s
            .native_code
            .borrow()
            .instances
            .values()
            .find(|v| v.method == method && !method.is_null())
            .cloned()
        {
            return Ok(Some(v));
        }
        let entries: Vec<_> = s
            .native_code
            .borrow()
            .instances
            .values()
            .filter(|v| !v.method.is_null())
            .cloned()
            .collect();
        for v in &entries {
            q.keep(v.method);
        }
        for v in entries {
            if s.send(s.id.root, b"sameNativeMethod:as:", &[method, v.method])?
                == abi::size(1).unwrap()
            {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }
}
pub(super) unsafe fn body(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<Oop>, Oop> {
    unsafe { Ok(instance(s, method, q)?.and_then(|v| v.environment)) }
}
pub(super) unsafe fn source(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<Oop>, Oop> {
    unsafe { Ok(instance(s, method, q)?.map(|v| v.code.source)) }
}
pub(super) unsafe fn method_from_block(
    s: &State,
    body: Oop,
    q: &Scope<'_>,
) -> Result<Option<Oop>, Oop> {
    unsafe {
        let method = q.keep(s.field(body, 0)?);
        let Some(v) = instance(s, method, q)? else {
            return Ok(None);
        };
        if v.environment.is_some()
            || v.code.parameters == 0
            || s.storage.borrow().image_shell(body)
            || s.storage.borrow().payload_size(body)
                != Some((v.code.captures + 1) * size_of::<Oop>())
        {
            return Err(invalid());
        }
        for i in 0..v.code.captures {
            if s.field(body, i + 1)?.is_null() {
                return Err(invalid());
            }
        }
        Ok(Some(construct(s, v.code, Some(body), false)?))
    }
}
pub(super) unsafe fn for_dnu(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<Oop>, Oop> {
    unsafe {
        let Some(v) = instance(s, method, q)? else {
            return Ok(None);
        };
        let Some(environment) = v.environment else {
            return Ok(Some(method));
        };
        Ok(Some(construct(s, v.code, Some(environment), true)?))
    }
}
pub(super) unsafe fn with_source(
    s: &State,
    method: Oop,
    source: Oop,
    q: &Scope<'_>,
) -> Result<Option<Oop>, Oop> {
    unsafe {
        let Some(v) = instance(s, method, q)? else {
            return Ok(None);
        };
        if v.environment.is_some() {
            return Err(invalid());
        }
        if v.code.source == source {
            return Ok(Some(v.method));
        }
        let owner = s.named(b"CodeObject")?;
        let original = q.keep(s.send(owner, b"canonicalBytesOf:", &[v.code.source])?);
        let requested = q.keep(s.send(owner, b"canonicalBytesOf:", &[source])?);
        if resumable::bytes(s, original, 64 << 20)? != resumable::bytes(s, requested, 64 << 20)? {
            return Err(invalid());
        }
        Ok(Some(construct(
            s,
            Rc::new(Code {
                factory: v.code.factory,
                source,
                parameters: v.code.parameters,
                captures: v.code.captures,
            }),
            None,
            false,
        )?))
    }
}
pub(super) fn references(s: &State, object: Oop) -> Option<Vec<Oop>> {
    s.native_code
        .borrow()
        .instances
        .get(&object.addr())
        .map(|v| {
            let mut refs = vec![v.code.factory, v.code.source, v.method];
            refs.extend(v.environment);
            refs
        })
}
pub(super) fn retire(s: &State, objects: &[Oop]) {
    for object in objects {
        s.native_code.borrow_mut().instances.remove(&object.addr());
    }
}
pub(super) unsafe extern "C" fn field<const WHICH: u8>(
    _: Oop,
    c: Oop,
    r: Oop,
    _: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let data = s
            .native_code
            .borrow()
            .instances
            .get(&r.addr())
            .cloned()
            .ok_or_else(invalid)?;
        Ok(match WHICH {
            0 => data.environment.ok_or_else(invalid)?,
            1 => abi::size(data.code.parameters).ok_or_else(invalid)?,
            2 => abi::size(data.code.captures).ok_or_else(invalid)?,
            _ => data.code.source,
        })
    })()
    .unwrap_or_else(abi::fault_reference)
}
