//! Allocation policy above Id. Raw storage remains reachable through live messages.
use super::*;
pub(super) fn add(current: usize, bytes: usize) -> usize {
    current.checked_add(bytes).unwrap_or_else(|| {
        eprintln!("birnam invariant failed: GC allocation byte counter overflow");
        std::process::abort()
    })
}
impl Memory {
    pub(super) unsafe fn allocation_charge(&self, bytes: usize) -> Result<usize, Oop> {
        abi::as_size(unsafe {
            self.send(
                self.id.root,
                b"allocationChargeFor:",
                &[abi::size(bytes).ok_or(self.invalid())?],
            )
        }?)
        .filter(|n| *n != 0)
        .ok_or(self.invalid())
    }
    unsafe fn prepare(&self) -> Result<(), Oop> {
        if !matches!(self.phase.get(), Phase::Inactive | Phase::Idle) {
            return Err(self.invalid());
        }
        if self.phase.get() == Phase::Idle
            && self.automatic.get()
            && !self.constructing.get()
            && self.charged.get() >= self.budget.get()
        {
            unsafe {
                native_stack::with_roots(self.invalid(), |roots| collection::run(self, roots))
            }?;
        }
        Ok(())
    }
    unsafe fn construct(&self, operation: impl FnOnce() -> Result<Oop, Oop>) -> Result<Oop, Oop> {
        // Id already keeps the incoming receiver and arguments alive. Only the
        // returned object needs an additional root across the accounting sends.
        unsafe { self.prepare() }?;
        let depth = self.temporary.borrow().len();
        let nested = self.constructing.replace(true);
        let answer = (|| unsafe {
            let value = operation()?;
            if value.is_null() || abi::as_size(value).is_some() {
                return Err(self.invalid());
            }
            self.temporary.borrow_mut().push(value);
            let bytes = abi::as_size(self.send(self.id.root, b"allocationSizeOf:", &[value])?)
                .ok_or(self.invalid())?;
            let charge = self.allocation_charge(bytes)?;
            self.charged.set(add(self.charged.get(), charge));
            Ok(value)
        })();
        self.constructing.set(nested);
        self.temporary.borrow_mut().truncate(depth);
        answer
    }
}
macro_rules! wrappers{($(($entry:ident,$raw:literal)),*$(,)?)=>{$(
    pub(super) unsafe extern "C" fn $entry(_id:Oop,c:Oop,r:Oop,a:*const Oop,n:usize)->Oop{
        (||unsafe{let s=memory(c)?;let a=abi::borrowed_slice(a,n).ok_or(s.invalid())?;
            s.construct(||s.send(r,$raw,a))
        })().unwrap_or_else(|e|e)
    }
)*};}
wrappers![
    (new_name, b"rawNameWithBytes"),
    (sequence, b"rawSequence"),
    (method_context, b"rawWithContext:"),
    (native_method, b"rawNewMethodAt:")
];
pub(super) unsafe extern "C" fn allocate(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        let Some([bytes]) = abi::borrowed_slice(a, n) else {
            return Err(s.invalid());
        };
        abi::as_size(*bytes).ok_or(s.invalid())?;
        s.construct(|| s.send(s.id.root, b"allocate:bytes:", &[r, *bytes]))
    })()
    .unwrap_or_else(|e| e)
}
pub(super) unsafe extern "C" fn delegated(
    _id: Oop,
    c: Oop,
    r: Oop,
    _a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        if n != 0 {
            return Err(s.invalid());
        }
        s.construct(|| s.send(s.id.root, b"newBehaviorWithParent:", &[r]))
    })()
    .unwrap_or_else(|e| e)
}
pub(super) unsafe extern "C" fn intern(_id: Oop, c: Oop, r: Oop, _a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        if n != 0 {
            return Err(s.invalid());
        }
        let current = s.send(r, b"existingSymbol", &[])?;
        if !current.is_null() {
            return Ok(current);
        }
        s.construct(|| s.send(r, b"rawIntern", &[]))
    })()
    .unwrap_or_else(|e| e)
}
pub(super) unsafe extern "C" fn prepare(
    _id: Oop,
    c: Oop,
    _r: Oop,
    _a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        if n != 0 {
            return Err(s.invalid());
        }
        s.prepare()?;
        Ok(core::ptr::null_mut())
    })()
    .unwrap_or_else(|e| e)
}
pub(super) unsafe extern "C" fn activate(
    _id: Oop,
    c: Oop,
    _r: Oop,
    _a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        if n != 0 {
            return Err(s.invalid());
        }
        if s.phase.get() == Phase::Idle {
            return Ok(core::ptr::null_mut());
        }
        if s.phase.get() != Phase::Inactive {
            return Err(s.invalid());
        }
        let object = s.send(s.id.root, b"objectBehavior", &[])?;
        let meta = s.send(s.id.root, b"vtableBehavior", &[])?;
        let name = s.send(s.id.root, b"nameBehavior", &[])?;
        let method = s.send(s.id.root, b"methodBehavior", &[])?;
        let module = s.send(s.id.root, b"moduleBehavior", &[])?;
        for (target, public, implementation) in [
            (meta, b"allocate".as_slice(), b"managedAllocate".as_slice()),
            (meta, b"delegated", b"managedDelegated"),
            (name, b"intern", b"managedIntern"),
            (object, b"nameWithBytes", b"managedName"),
            (object, b"sequence", b"managedSequence"),
            (method, b"withContext:", b"managedContext"),
            (module, b"newMethodAt:", b"managedNativeMethod"),
        ] {
            let implementation = s.send(s.image, b"methodAt:", &[s.names[implementation]])?;
            let implementation = s.send(implementation, b"withContext:", &[s.receiver])?;
            s.send(target, b"addMethod", &[s.names[public], implementation])?;
        }
        // Failed validation leaves complete policy methods installed; inactive
        // construction can repair missing owners before a later activation.
        s.phase.set(Phase::Marking);
        let answer = collection::validate(s);
        *s.walk.borrow_mut() = None;
        s.phase.set(if answer.is_ok() {
            Phase::Idle
        } else {
            Phase::Inactive
        });
        s.charged.set(answer?);
        Ok(core::ptr::null_mut())
    })()
    .unwrap_or_else(|e| e)
}
pub(super) unsafe extern "C" fn budget(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        if s.phase.get() != Phase::Idle {
            return Err(s.invalid());
        }
        let Some([v]) = abi::borrowed_slice(a, n) else {
            return Err(s.invalid());
        };
        s.budget
            .set(abi::as_size(*v).filter(|v| *v != 0).ok_or(s.invalid())?);
        Ok(core::ptr::null_mut())
    })()
    .unwrap_or_else(|e| e)
}
unsafe fn setting(c: Oop, a: *const Oop, n: usize, precise: bool) -> Result<Oop, Oop> {
    let s = unsafe { memory(c) }?;
    let Some([v]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return Err(s.invalid());
    };
    let enabled = abi::as_size(*v).ok_or(s.invalid())? != 0;
    if precise {
        s.precise_payloads.set(enabled);
    } else {
        s.automatic.set(enabled);
    }
    Ok(core::ptr::null_mut())
}
pub(super) unsafe extern "C" fn automatic(
    _id: Oop,
    c: Oop,
    _r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { setting(c, a, n, false) }.unwrap_or_else(|e| e)
}
pub(super) unsafe extern "C" fn precise_set(
    _id: Oop,
    c: Oop,
    _r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { setting(c, a, n, true) }.unwrap_or_else(|e| e)
}
pub(super) unsafe extern "C" fn precise(
    _id: Oop,
    c: Oop,
    _r: Oop,
    _a: *const Oop,
    n: usize,
) -> Oop {
    match unsafe { memory(c) } {
        Ok(s) => {
            if n == 0 {
                abi::size(usize::from(s.precise_payloads.get())).unwrap()
            } else {
                s.invalid()
            }
        }
        Err(e) => e,
    }
}
pub(super) unsafe extern "C" fn charged(
    _id: Oop,
    c: Oop,
    _r: Oop,
    _a: *const Oop,
    n: usize,
) -> Oop {
    match unsafe { memory(c) } {
        Ok(s) => {
            if n == 0 {
                abi::size(s.charged.get()).unwrap_or(s.invalid())
            } else {
                s.invalid()
            }
        }
        Err(e) => e,
    }
}
pub(super) unsafe extern "C" fn external(
    _id: Oop,
    c: Oop,
    _r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        if !matches!(s.phase.get(), Phase::Idle | Phase::Inactive) {
            return Err(s.invalid());
        }
        let Some([bytes]) = abi::borrowed_slice(a, n) else {
            return Err(s.invalid());
        };
        s.charged.set(add(
            s.charged.get(),
            abi::as_size(*bytes).ok_or(s.invalid())?,
        ));
        Ok(core::ptr::null_mut())
    })()
    .unwrap_or_else(|e| e)
}
pub(super) unsafe extern "C" fn collect(
    _id: Oop,
    c: Oop,
    _r: Oop,
    _a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        if n != 0 {
            return Err(s.invalid());
        }
        if matches!(s.phase.get(), Phase::Marking | Phase::Finalizing) {
            return Ok(abi::size(0).unwrap());
        }
        native_stack::with_roots(s.invalid(), |roots| collection::run(s, roots))
    })()
    .unwrap_or_else(|e| e)
}
