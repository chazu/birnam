//! Integer representation and default behavior, entirely above the seed.
//! See Design 81G for the native mount and message contracts.
use birnam_observation_abi::{
    PROVIDER, ProviderSink, RETIREMENT, RetirementPlan, RetirementRequest, decode,
};
use birnam_seed_abi::*;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ffi::c_void;

// Module-specific status, not an addition to the seed ABI's status constants.
const OVERFLOW: Status = 0x100;

struct IntegerModule {
    seed: *const SeedApi,
    factory: Oop,
    behavior: Oop,
    from_word: Oop,
    // Provenance only: values live in payloads, never in this set. Behavior can
    // change without changing representation. Conversely, attaching Integer
    // behavior to some other payload does not make that payload safe to read.
    instances: RefCell<BTreeSet<Oop>>,
}
impl IntegerModule {
    fn read(&self, object: Oop) -> Result<i64, Status> {
        if !self.instances.borrow().contains(&object) {
            return Err(INVALID);
        }
        // Only our constructor adds identities, after requesting an i64 payload.
        // Collection removes provenance before storage becomes reusable.
        Ok(unsafe { (object as *const i64).read() })
    }
}

unsafe fn module<'a>(api: *const SeedApi, data: *mut c_void) -> Result<&'a IntegerModule, Status> {
    let module = unsafe { &*data.cast::<IntegerModule>() };
    if module.seed != api {
        return Err(INVALID);
    }
    Ok(module)
}

unsafe fn return_value(out: *mut Value, result: Result<Value, Status>) -> Status {
    match result {
        Ok(value) => {
            unsafe { out.write(value) };
            OK
        }
        Err(status) => status,
    }
}

unsafe extern "C" fn behavior(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let module = match unsafe { module(api, data) } {
        Ok(m) => m,
        Err(e) => return e,
    };
    if count != 0 || receiver != module.factory {
        return INVALID;
    }
    unsafe { return_value(out, Ok(Value::object(module.behavior))) }
}

unsafe extern "C" fn from_word(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let module = match unsafe { module(api, data) } {
        Ok(m) => m,
        Err(e) => return e,
    };
    if count != 1 || receiver != module.factory {
        return INVALID;
    }
    let word = unsafe { *args };
    if word.kind != WORD || word.reserved != 0 || word.b != 0 {
        return INVALID;
    }
    // No provenance borrow spans replaceable allocation, which may reenter.
    let allocated = unsafe {
        call(
            api,
            module.behavior,
            (*api).selectors.allocate,
            &[Value::word(size_of::<i64>())],
        )
    };
    let object = match allocated {
        Ok(value) if value.kind == OBJECT && value.reserved == 0 && value.b == 0 => value.a,
        Ok(_) => return INVALID,
        Err(e) => return e,
    };
    if object == 0
        || !object.is_multiple_of(align_of::<i64>())
        || unsafe { ((*api).behavior_of)(api, object) } != module.behavior
        || module.instances.borrow().contains(&object)
    {
        return INVALID;
    }
    // The allocator protocol promises a fresh, writable allocation of the
    // requested extent/alignment. An arbitrary lying allocator is not checked
    // by this trusted native boundary; no seed-private extent table is read.
    unsafe { (object as *mut i64).write(word.a as i64) };
    module.instances.borrow_mut().insert(object);
    unsafe { return_value(out, Ok(Value::object(object))) }
}

unsafe extern "C" fn as_word(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if count != 0 {
        return INVALID;
    }
    let module = match unsafe { module(api, data) } {
        Ok(m) => m,
        Err(e) => return e,
    };
    unsafe {
        return_value(
            out,
            module
                .read(receiver)
                .map(|value| Value::word(value as usize)),
        )
    }
}

unsafe fn binary(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
    operation: fn(i64, i64) -> Option<i64>,
) -> Status {
    if count != 1 {
        return INVALID;
    }
    let module = match unsafe { module(api, data) } {
        Ok(m) => m,
        Err(e) => return e,
    };
    let operand = unsafe { *args };
    if operand.kind != OBJECT || operand.reserved != 0 || operand.b != 0 {
        return INVALID;
    }
    let lhs = match module.read(receiver) {
        Ok(value) => value,
        Err(e) => return e,
    };
    let rhs = match module.read(operand.a) {
        Ok(value) => value,
        Err(e) => return e,
    };
    let Some(value) = operation(lhs, rhs) else {
        return OVERFLOW;
    };
    // Result construction is itself a send. Replacing the factory method or its
    // allocation behavior remains observable; no cached native constructor.
    unsafe {
        return_value(
            out,
            call(
                api,
                module.factory,
                module.from_word,
                &[Value::word(value as usize)],
            ),
        )
    }
}

unsafe extern "C" fn add(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    unsafe { binary(api, data, receiver, args, count, out, i64::checked_add) }
}
unsafe extern "C" fn subtract(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    unsafe { binary(api, data, receiver, args, count, out, i64::checked_sub) }
}
unsafe extern "C" fn multiply(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    unsafe { binary(api, data, receiver, args, count, out, i64::checked_mul) }
}

unsafe extern "C" fn describe(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let module = match unsafe { module(api, data) } {
        Ok(m) => m,
        Err(e) => return e,
    };
    let sink = match unsafe { decode::<ProviderSink>(args, count) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    for object in [module.factory, module.behavior, module.from_word] {
        let status = unsafe { (sink.root)(sink.context, object) };
        if status != OK {
            return status;
        }
    }
    // Provenance is not a strong root set. Each initialized Integer has raw
    // bytes and zero payload reference slots, regardless of current behavior.
    let instances: Vec<_> = module.instances.borrow().iter().copied().collect();
    for object in instances {
        let status =
            unsafe { (sink.payload)(sink.context, object, size_of::<i64>(), core::ptr::null(), 0) };
        if status != OK {
            return status;
        }
    }
    OK
}
unsafe extern "C" fn storage_provider(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let module = match unsafe { module(api, data) } {
        Ok(m) => m,
        Err(e) => return e,
    };
    if count != 0 || receiver != module.factory {
        return INVALID;
    }
    unsafe {
        return_value(
            out,
            Ok(Value::method(Method {
                entry: describe,
                data,
            })),
        )
    }
}

struct PreparedRetirement {
    owner: *const IntegerModule,
    replacement: BTreeSet<Oop>,
}
unsafe extern "C" fn commit_retirement(data: *mut c_void) {
    let plan = unsafe { Box::from_raw(data.cast::<PreparedRetirement>()) };
    *unsafe { &*plan.owner }.instances.borrow_mut() = plan.replacement;
}
unsafe extern "C" fn abort_retirement(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<PreparedRetirement>()) });
}
unsafe extern "C" fn prepare_retirement(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let owner = unsafe { &*data.cast::<IntegerModule>() };
    if owner.seed != api {
        return INVALID;
    }
    let request = match unsafe { decode::<RetirementRequest>(args, count) } {
        Ok(r) => r,
        Err(e) => return e,
    };
    if request.plan.is_null() || (request.count != 0 && request.objects.is_null()) {
        return INVALID;
    }
    let victims: std::collections::BTreeSet<_> = (0..request.count)
        .map(|i| unsafe { *request.objects.add(i) })
        .collect();
    let mut replacement = owner.instances.borrow().clone();
    replacement.retain(|object| !victims.contains(object));
    let plan = Box::new(PreparedRetirement { owner, replacement });
    unsafe {
        request.plan.write(RetirementPlan {
            data: Box::into_raw(plan).cast(),
            commit: commit_retirement,
            abort: abort_retirement,
        })
    };
    OK
}
unsafe extern "C" fn retirement_provider(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let owner = unsafe { &*data.cast::<IntegerModule>() };
    if owner.seed != api || count != 0 {
        return INVALID;
    }
    unsafe {
        out.write(Value::method(Method {
            entry: prepare_retirement,
            data,
        }))
    };
    OK
}

/// Mount ABI: create(seed, factory_out) -> opaque module lifetime handle.
/// All value operations, including construction, are ordinary messages.
/// # Safety
/// API must be live on its owner thread; factory_out must be writable. Keep
/// returned state/code alive until AFTER seed destruction. Overridden allocation
/// must honor its fresh-storage contract. Failed method publication aborts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_integer_create_v1(
    api: *const SeedApi,
    factory_out: *mut Oop,
) -> *mut c_void {
    if api.is_null() || factory_out.is_null() {
        return core::ptr::null_mut();
    }
    unsafe { factory_out.write(0) };
    if unsafe { (*api).version } != VERSION
        || unsafe { (*api).size } as usize != size_of::<SeedApi>()
        || size_of::<usize>() != size_of::<i64>()
    {
        return core::ptr::null_mut();
    }
    let setup = || -> Result<_, Status> {
        let delegate =
            || unsafe { call(api, (*api).roots.object, (*api).selectors.delegated, &[]) };
        let factory_behavior = delegate()?;
        let integer_behavior = delegate()?;
        if factory_behavior.kind != OBJECT || integer_behavior.kind != OBJECT {
            return Err(INVALID);
        }
        let factory = unsafe {
            call(
                api,
                factory_behavior.a,
                (*api).selectors.allocate,
                &[Value::word(0)],
            )
        }?;
        if factory.kind != OBJECT {
            return Err(INVALID);
        }
        let selectors: Result<Vec<_>, _> = [
            b"integerBehavior".as_slice(),
            b"fromWord:",
            b"asWord",
            b"+",
            b"-",
            b"*",
            PROVIDER,
            RETIREMENT,
        ]
        .into_iter()
        .map(|name| unsafe { intern(api, name) })
        .collect();
        Ok((
            factory_behavior.a,
            integer_behavior.a,
            factory.a,
            selectors?,
        ))
    };
    let Ok((factory_behavior, integer_behavior, factory, selectors)) = setup() else {
        return core::ptr::null_mut();
    };
    let state = Box::into_raw(Box::new(IntegerModule {
        seed: api,
        factory,
        behavior: integer_behavior,
        from_word: selectors[1],
        instances: RefCell::new(BTreeSet::new()),
    }));
    for (target, selector, entry) in [
        (factory_behavior, selectors[0], behavior as Entry),
        (factory_behavior, selectors[1], from_word as Entry),
        (integer_behavior, selectors[2], as_word as Entry),
        (integer_behavior, selectors[3], add as Entry),
        (integer_behavior, selectors[4], subtract as Entry),
        (integer_behavior, selectors[5], multiply as Entry),
        (factory_behavior, selectors[6], storage_provider as Entry),
        (factory_behavior, selectors[7], retirement_provider as Entry),
    ] {
        let published = unsafe {
            call(
                api,
                target,
                (*api).selectors.add_method,
                &[
                    Value::object(selector),
                    Value::method(Method {
                        entry,
                        data: state.cast(),
                    }),
                ],
            )
        };
        // A malleable installer may publish before failing. Without retirement
        // there is no safe rollback which frees state but keeps the seed alive.
        assert!(published.is_ok(), "experimental Integer publication failed");
    }
    unsafe { factory_out.write(factory) };
    state.cast()
}

/// # Safety
/// Handle must come from create, be destroyed once, and its seed must already be
/// destroyed. Its methods retain pointers to this state until seed destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_integer_destroy_v1(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<IntegerModule>()) });
    }
}
