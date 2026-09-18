//! Upper-layer consumer. Depends on the public carrier ONLY, never Seed state.
use birnam_observation_abi::{ProviderSink, decode};
use birnam_seed_abi::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::c_void;

struct Library {
    seed: *const SeedApi,
    // Private-vtable owner/reuse policy is owned here, not inside the seed.
    private: RefCell<BTreeMap<Oop, Oop>>,
    created: Cell<usize>,
    selector: Oop,
}
unsafe extern "C" fn ensure_private(
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
    let library = unsafe { &*data.cast::<Library>() };
    if api != library.seed {
        return INVALID;
    }
    let current = unsafe { ((*api).behavior_of)(api, receiver) };
    if current == 0 {
        return INVALID;
    }
    let prior = library.private.borrow().get(&receiver).copied();
    if prior == Some(current) {
        unsafe { out.write(Value::object(current)) };
        return OK;
    }
    let result = unsafe { call(api, current, (*api).selectors.delegated, &[]) };
    let private = match result {
        Ok(value) if value.kind == OBJECT => value.a,
        Ok(_) => return INVALID,
        Err(e) => return e,
    };
    let status = unsafe { ((*api).reassociate)(api, receiver, private) };
    if status != OK {
        return status;
    }
    library.private.borrow_mut().insert(receiver, private);
    library.created.set(library.created.get() + 1);
    unsafe { out.write(Value::object(private)) };
    OK
}
unsafe extern "C" fn created_count(api: *const LibraryApi) -> usize {
    unsafe { &*((*api).context.cast::<Library>()) }
        .created
        .get()
}
unsafe extern "C" fn describe(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let library = unsafe { &*data.cast::<Library>() };
    if api != library.seed {
        return INVALID;
    }
    let sink = match unsafe { decode::<ProviderSink>(args, count) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    // Owner/reuse bookkeeping is a weak cache, not a reason to keep every
    // private object alive. Header edges supplied by Id retain active behavior.
    unsafe { (sink.root)(sink.context, library.selector) }
}
unsafe extern "C" fn storage_provider(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let library = unsafe { &*data.cast::<Library>() };
    if api != library.seed || count != 0 {
        return INVALID;
    }
    unsafe {
        out.write(Value::method(Method {
            entry: describe,
            data,
        }))
    };
    OK
}

/// # Safety
/// Seed API and its owner thread/code must remain valid throughout this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_object_model_create_v1(api: *const SeedApi) -> *mut LibraryApi {
    if api.is_null()
        || unsafe { (*api).version } != VERSION
        || unsafe { (*api).size } as usize != size_of::<SeedApi>()
    {
        return core::ptr::null_mut();
    }
    let Ok(selector) = (unsafe { intern(api, b"ensurePrivateVtable") }) else {
        return core::ptr::null_mut();
    };
    let library = Box::new(Library {
        seed: api,
        private: RefCell::new(BTreeMap::new()),
        created: Cell::new(0),
        selector,
    });
    let data = Box::into_raw(library).cast();
    let method = Method {
        entry: ensure_private,
        data,
    };
    let publication = unsafe {
        call(
            api,
            (*api).roots.object,
            (*api).selectors.add_method,
            &[Value::object(selector), Value::method(method)],
        )
    };
    // An overridden installer could publish and then report failure. Without a
    // retirement/rollback protocol we cannot free this data and keep running.
    // This experiment treats failed publication as fatal construction failure.
    assert!(
        publication.is_ok(),
        "experimental method publication failed"
    );
    // Explicitly published service handle, obtained before the observation phase.
    let provider = unsafe { intern(api, b"objectModelStorageProvider") }.unwrap();
    unsafe {
        call(
            api,
            (*api).roots.vtable,
            (*api).selectors.add_method,
            &[
                Value::object(provider),
                Value::method(Method {
                    entry: storage_provider,
                    data,
                }),
            ],
        )
    }
    .unwrap();
    Box::into_raw(Box::new(LibraryApi {
        version: VERSION,
        size: size_of::<LibraryApi>() as u32,
        context: data,
        ensure_private: selector,
        created_count,
    }))
}
/// # Safety
/// Seed must already be destroyed so no dictionary refers to this method/data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_object_model_destroy_v1(api: *mut LibraryApi) {
    if api.is_null() {
        return;
    }
    let api = unsafe { Box::from_raw(api) };
    drop(unsafe { Box::from_raw(api.context.cast::<Library>()) });
}
