//! Rust TCC dynamic rung.
//!
//! This crate owns the libtcc state machine and the generated-C callback
//! boundary.  The object protocol is intentionally the same small surface as
//! `birnam-tcc`; all calls into the host IdApi happen through short unsafe
//! sections so no Rust borrow survives a foreign callback.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{
    IdApi, Method, Oop, SLOT_OOP, SLOT_RAW, borrowed_oop_args,
    foreign_address::{self, KIND_BORROWED},
};
use birnam_native_compiler::{
    TccState, tcc_add_file, tcc_add_library, tcc_add_library_path, tcc_add_symbol,
    tcc_compile_string, tcc_delete, tcc_get_symbol, tcc_new, tcc_output_file, tcc_relocate,
    tcc_set_error_func, tcc_set_output_type,
};
#[cfg(feature = "dev-guarded-slots")]
use core::ffi::c_long;
use core::ffi::{c_char, c_int, c_void};
use core::mem::size_of;
use core::ptr::{self, NonNull};
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

#[cfg(feature = "resource-metrics")]
mod resource_metrics;

mod unit_cache;

const RUST_TCC_IDENTITY: &[u8] = b"rust-tcc-n4.2-state\0";
const TCC_NAME: &[u8] = b"Tcc\0";
const TCC_VT_NAME: &[u8] = b"TccVt\0";
const UNIT_VT_NAME: &[u8] = b"TccUnitVt\0";

const TCC_OUTPUT_MEMORY: c_int = 1;
const TCC_OUTPUT_EXE: c_int = 2;
const TCC_OUTPUT_OBJ: c_int = 3;
const TCC_OUTPUT_DLL: c_int = 4;
static LIVE_UNIT_STATES: AtomicUsize = AtomicUsize::new(0);

static VALIDATION_VEC_FACTORY: u8 = 0;

unsafe extern "C" fn validation_foreign() -> i64 {
    0
}

unsafe extern "C" fn validation_strlen(_value: *const c_char) -> i64 {
    0
}

unsafe extern "C" fn validation_pointer(_value: *mut c_void) -> *mut c_void {
    ptr::null_mut()
}

unsafe extern "C" fn validation_small_int(_value: *mut c_void) -> i64 {
    0
}

unsafe extern "C" fn validation_float(_value: *mut c_void) -> f64 {
    0.0
}

unsafe extern "C" fn validation_word(_value: *mut c_void) -> u64 {
    0
}

unsafe extern "C" fn validation_cstr(_value: *mut c_void) -> *const c_char {
    c"".as_ptr()
}

unsafe extern "C" fn validation_free_cstr(_value: *const c_char) {}

unsafe extern "C" fn validation_make_string(_value: *const c_char, _length: i64) -> *mut c_void {
    ptr::null_mut()
}

unsafe extern "C" fn validation_send(
    _receiver: *mut c_void,
    _selector: *mut c_void,
    _args: *mut c_void,
    _count: i64,
) -> *mut c_void {
    ptr::null_mut()
}

unsafe extern "C" fn validation_intern(_name: *const c_char) -> *mut c_void {
    ptr::null_mut()
}

fn add_validation_symbol(state: *mut TccState, name: &[u8], value: *const c_void) -> bool {
    let Ok(name) = CString::new(name) else {
        return false;
    };
    // SAFETY: `name` owns a NUL-terminated symbol name and `value` is the
    // address of a validation callback or static carrier retained for the
    // whole TCC operation.
    unsafe { tcc_add_symbol(state, name.as_ptr(), value) >= 0 }
}

/// Serializes validation compiles. libtcc carries process-global state across
/// `tcc_new`/`tcc_relocate`/`tcc_delete`, so two of them running at once
/// corrupts it and takes the process down with SIGSEGV — cargo runs a crate's
/// tests in parallel threads, which made any suite holding two of these
/// validation tests fail roughly one run in four. The lock lives here rather
/// than in each caller so no test has to remember the constraint; validation
/// compiles are rare enough that the contention is free.
static VALIDATION_LOCK: Mutex<()> = Mutex::new(());

/// Compile and relocate one generated unit for source-validation tests.
///
/// This uses the same embedded libtcc owner as the Rust TCC floor, but is not
/// part of the object protocol and is never called by production Alien code.
#[doc(hidden)]
pub fn compile_source_for_validation(source: &[u8], foreign_name: &[u8]) -> bool {
    // A poisoned lock only means another validation compile panicked; whatever
    // libtcc state it held died with its TCCState, so resuming is safe.
    let _guard = VALIDATION_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let Ok(source) = CString::new(source) else {
        return false;
    };
    let Some(state) = NonNull::new(unsafe { tcc_new() }) else {
        return false;
    };
    let state = state.as_ptr();
    let configured = unsafe {
        tcc_set_error_func(state, ptr::null_mut(), Some(error_silent));
        tcc_set_output_type(state, TCC_OUTPUT_MEMORY) >= 0
            && tcc_compile_string(state, source.as_ptr()) >= 0
    };
    if !configured {
        unsafe { tcc_delete(state) };
        return false;
    }

    let mut symbols: Vec<(&[u8], *const c_void)> = vec![
        (b"birnam_mkSmallInt", validation_pointer as *const c_void),
        (
            b"birnam_smallIntValue",
            validation_small_int as *const c_void,
        ),
        (b"birnam_mkFloat", validation_pointer as *const c_void),
        (b"birnam_floatValue", validation_float as *const c_void),
        (b"birnam_mkWord", validation_pointer as *const c_void),
        (b"birnam_wordValue", validation_word as *const c_void),
        (
            b"birnam_mkForeignAddress",
            validation_pointer as *const c_void,
        ),
        (
            b"birnam_foreignAddressValue",
            validation_word as *const c_void,
        ),
        (
            b"birnam_isForeignAddress",
            validation_small_int as *const c_void,
        ),
        (b"birnam_uintValue", validation_word as *const c_void),
        (b"birnam_cstr", validation_cstr as *const c_void),
        (b"birnam_free_cstr", validation_free_cstr as *const c_void),
        (b"birnam_mkString", validation_make_string as *const c_void),
        (b"birnam_nil", validation_pointer as *const c_void),
        (b"strlen", validation_strlen as *const c_void),
        (b"birnam_send", validation_send as *const c_void),
        (b"birnam_intern", validation_intern as *const c_void),
        #[cfg(feature = "dev-guarded-slots")]
        (
            b"birnam_dev_frame_oop_at_v1",
            validation_pointer as *const c_void,
        ),
        #[cfg(feature = "dev-guarded-slots")]
        (
            b"birnam_dev_frame_oop_at_put_v1",
            validation_pointer as *const c_void,
        ),
        #[cfg(feature = "dev-guarded-slots")]
        (
            b"birnam_dev_frame_slot_get_v1",
            validation_pointer as *const c_void,
        ),
        #[cfg(feature = "dev-guarded-slots")]
        (
            b"birnam_dev_frame_slot_put_v1",
            validation_pointer as *const c_void,
        ),
    ];
    symbols.push((
        b"__vecFactory",
        ptr::addr_of!(VALIDATION_VEC_FACTORY).cast(),
    ));
    symbols.push((foreign_name, validation_foreign as *const c_void));
    let symbols_added = symbols
        .into_iter()
        .all(|(name, value)| add_validation_symbol(state, name, value));
    let relocated = symbols_added
        && unsafe { tcc_relocate(state) >= 0 }
        && !unsafe { tcc_get_symbol(state, c"__shim".as_ptr()) }.is_null();
    unsafe { tcc_delete(state) };
    relocated
}

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    tcc_vt: usize,
    tcc: usize,
    unit_vt: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();

unsafe extern "C" fn mark_runtime_roots(mark: birnam_abi::MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [runtime.tcc_vt, runtime.tcc, runtime.unit_vt] {
            unsafe { mark(value as Oop) };
        }
    }
}

#[repr(C)]
struct DlInfo {
    filename: *const c_char,
    base: *mut c_void,
    symbol_name: *const c_char,
    symbol_address: *mut c_void,
}

#[cfg_attr(target_os = "linux", link(name = "dl"))]
unsafe extern "C" {
    fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
}

/// Resolve one private symbol from the exact selected floor image (the one
/// that owns `api.dlsym`), keeping that floor `RTLD_LOCAL`. Answers 0 when the
/// image or symbol cannot be found; the result is cached per call site.
fn floor_symbol_address(
    api: &IdApi,
    symbol: &'static CStr,
    address: &'static OnceLock<usize>,
) -> usize {
    *address.get_or_init(|| {
        let mut info = DlInfo {
            filename: ptr::null(),
            base: ptr::null_mut(),
            symbol_name: ptr::null(),
            symbol_address: ptr::null_mut(),
        };
        if unsafe { dladdr(api.dlsym as *const () as *const c_void, &mut info) } == 0
            || info.filename.is_null()
        {
            return 0;
        }
        #[cfg(target_os = "macos")]
        const RTLD_NOW_LOCAL: c_int = 2 | 4;
        #[cfg(not(target_os = "macos"))]
        const RTLD_NOW_LOCAL: c_int = 2;
        let handle = unsafe { (api.dlopen)(info.filename, RTLD_NOW_LOCAL) };
        if handle.is_null() {
            return 0;
        }
        unsafe { (api.dlsym)(handle, symbol.as_ptr()) as usize }
    })
}

fn floor_registration(
    api: &IdApi,
    symbol: &'static CStr,
    address: &'static OnceLock<usize>,
) -> Option<unsafe extern "C" fn(*mut c_void) -> u32> {
    let address = floor_symbol_address(api, symbol, address);
    if address == 0 {
        None
    } else {
        Some(unsafe {
            core::mem::transmute::<usize, unsafe extern "C" fn(*mut c_void) -> u32>(address)
        })
    }
}

fn floor_retirement(
    api: &IdApi,
    symbol: &'static CStr,
    address: &'static OnceLock<usize>,
) -> Option<unsafe extern "C" fn(u64) -> u32> {
    let address = *address.get_or_init(|| {
        let mut info = DlInfo {
            filename: ptr::null(),
            base: ptr::null_mut(),
            symbol_name: ptr::null(),
            symbol_address: ptr::null_mut(),
        };
        if unsafe { dladdr(api.dlsym as *const () as *const c_void, &mut info) } == 0
            || info.filename.is_null()
        {
            return 0;
        }
        #[cfg(target_os = "macos")]
        const RTLD_NOW_LOCAL: c_int = 2 | 4;
        #[cfg(not(target_os = "macos"))]
        const RTLD_NOW_LOCAL: c_int = 2;
        let handle = unsafe { (api.dlopen)(info.filename, RTLD_NOW_LOCAL) };
        if handle.is_null() {
            return 0;
        }
        unsafe { (api.dlsym)(handle, symbol.as_ptr()) as usize }
    });
    if address == 0 {
        None
    } else {
        Some(unsafe { core::mem::transmute::<usize, unsafe extern "C" fn(u64) -> u32>(address) })
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UnitPhase {
    Created = 0,
    Configured = 1,
    Relocated = 2,
    Installed = 3,
    Disposable = 4,
    Released = 5,
}

#[repr(C)]
struct UnitPayload {
    state: *mut TccState,
    phase: UnitPhase,
    code_generation_identity: u64,
}

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust TCC: {message}");
    std::process::abort();
}

unsafe fn api_ref(runtime: &Runtime) -> &'static IdApi {
    // SAFETY: module_init records the process-owned IdApi table and the host
    // retains its floor dylib for the process lifetime.
    unsafe { &*(runtime.api as *const IdApi) }
}

unsafe fn import(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all module names are static NUL-terminated strings.
    unsafe { (api.import)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn arg(args: *const Oop, count: usize, index: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of this
    // method call; borrowed_oop_args validates the null/count exception.
    unsafe { borrowed_oop_args(args, count)?.get(index).copied() }
}

unsafe fn string_arg(api: &IdApi, object: Oop) -> Option<CString> {
    let mut length = 0usize;
    // SAFETY: the Oop is supplied by the host message ABI and the output slot
    // is valid for the duration of this call.
    let bytes = unsafe { (api.stringBytes)(object, &mut length) };
    if length != 0 && bytes.is_null() {
        return None;
    }
    // SAFETY: stringBytes promises a readable byte span for the reported size.
    let slice = unsafe { core::slice::from_raw_parts(bytes, length) };
    CString::new(slice).ok()
}

unsafe fn unit_payload(object: Oop) -> Option<*mut UnitPayload> {
    if object.is_null() {
        return None;
    }
    Some(object.cast::<UnitPayload>())
}

unsafe fn unit_snapshot(object: Oop) -> Option<(*mut TccState, UnitPhase)> {
    let payload = unsafe { unit_payload(object)? };
    // SAFETY: Unit objects are allocated with this exact raw payload layout.
    Some(unsafe { ((*payload).state, (*payload).phase) })
}

unsafe fn release_payload(payload: *mut UnitPayload) {
    unit_cache::forget(payload.cast());
    // SAFETY: the finalizer owns the one-way transition and clears the pointer
    // before calling foreign code, so a repeated finalizer cannot double free.
    let state = unsafe { (*payload).state };
    unsafe {
        (*payload).state = ptr::null_mut();
        (*payload).phase = UnitPhase::Released;
    }
    if !state.is_null() {
        // SAFETY: state came from tcc_new and has not previously been deleted.
        #[cfg(feature = "resource-metrics")]
        unsafe {
            resource_metrics::releasing(state)
        };
        unsafe { tcc_delete(state) };
        LIVE_UNIT_STATES.fetch_sub(1, Ordering::AcqRel);
    }
}

unsafe extern "C" fn error_silent(_opaque: *mut c_void, _message: *const c_char) {}

unsafe fn new_unit(api: &IdApi, runtime: &Runtime) -> Oop {
    // SAFETY: libtcc returns an owned state or null.
    let state = unsafe { tcc_new() };
    if state.is_null() {
        return api.nilObj;
    }
    // SAFETY: state is live and the callback has no borrowed Rust state.
    unsafe {
        tcc_set_error_func(state, ptr::null_mut(), Some(error_silent));
        if tcc_set_output_type(state, TCC_OUTPUT_MEMORY) < 0 {
            tcc_delete(state);
            return api.nilObj;
        }
    }
    // SAFETY: the selected unit vtable is a live Birnam object and the raw
    // payload is initialized before the object can be observed.
    let object = unsafe { (api.alloc)(runtime.unit_vt as Oop, size_of::<UnitPayload>()) };
    if object.is_null() {
        // SAFETY: state is still owned by this constructor.
        unsafe { tcc_delete(state) };
        return api.nilObj;
    }
    let payload = object.cast::<UnitPayload>();
    // SAFETY: alloc returned storage of the requested size and alignment.
    unsafe {
        ptr::write(
            payload,
            UnitPayload {
                state,
                phase: UnitPhase::Created,
                code_generation_identity: 0,
            },
        );
    }
    LIVE_UNIT_STATES.fetch_add(1, Ordering::AcqRel);
    object
}

unsafe extern "C" fn m_live_unit_count(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    if count != 0 {
        return api.nilObj;
    }
    let value = LIVE_UNIT_STATES.load(Ordering::Acquire);
    unsafe { (api.mkSmallInt)(value as i64) }
}

unsafe fn fail_unit(api: &IdApi, object: Oop) -> Oop {
    if let Some(payload) = unsafe { unit_payload(object) } {
        unsafe { release_payload(payload) };
    }
    api.nilObj
}

unsafe fn configured_state(object: Oop) -> Option<*mut TccState> {
    let (state, phase) = unsafe { unit_snapshot(object)? };
    if state.is_null()
        || matches!(
            phase,
            UnitPhase::Relocated
                | UnitPhase::Installed
                | UnitPhase::Disposable
                | UnitPhase::Released
        )
    {
        return None;
    }
    Some(state)
}

unsafe fn relocated_state(object: Oop) -> Option<*mut TccState> {
    let (state, phase) = unsafe { unit_snapshot(object)? };
    if state.is_null() || !matches!(phase, UnitPhase::Relocated | UnitPhase::Installed) {
        return None;
    }
    Some(state)
}

unsafe extern "C" fn m_new(self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let _ = self_object;
    unsafe { new_unit(api, runtime) }
}

unsafe extern "C" fn m_compile(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let Some(source_object) = (unsafe { arg(args, count, 0) }) else {
        return api.nilObj;
    };
    let unit = unsafe { new_unit(api, runtime) };
    if unit == api.nilObj {
        return unit;
    }
    if unsafe { m_compile_into(unit, source_object, api) } == api.nilObj {
        return unsafe { fail_unit(api, unit) };
    }
    // SAFETY: the staged unit owns its state and relocation is the one-way
    // transition exercised by the public method below.
    unsafe { m_relocate(unit, ptr::null(), 0) }
}

unsafe fn m_compile_into(unit: Oop, source_object: Oop, api: &IdApi) -> Oop {
    let Some(state) = (unsafe { configured_state(unit) }) else {
        return api.nilObj;
    };
    let Some(source) = (unsafe { string_arg(api, source_object) }) else {
        return api.nilObj;
    };
    // SAFETY: CString owns the NUL-terminated source for the duration of TCC.
    if unsafe { tcc_compile_string(state, source.as_ptr()) } < 0 {
        return api.nilObj;
    }
    if let Some(payload) = unsafe { unit_payload(unit) } {
        // SAFETY: no foreign call is active while this phase byte is changed.
        unsafe { (*payload).phase = UnitPhase::Configured };
    }
    unit
}

unsafe extern "C" fn m_compile_into_method(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let Some(source_object) = (unsafe { arg(args, count, 0) }) else {
        return api.nilObj;
    };
    unsafe { m_compile_into(self_object, source_object, api) }
}

unsafe fn path_method(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    api: &IdApi,
    operation: unsafe extern "C" fn(*mut TccState, *const c_char) -> c_int,
) -> Oop {
    let Some(argument) = (unsafe { arg(args, count, 0) }) else {
        return api.nilObj;
    };
    let Some(state) = (unsafe { configured_state(self_object) }) else {
        return api.nilObj;
    };
    let Some(path) = (unsafe { string_arg(api, argument) }) else {
        return api.nilObj;
    };
    // SAFETY: path owns the NUL-terminated value and state is in a mutable
    // pre-relocation phase.
    if unsafe { operation(state, path.as_ptr()) } < 0 {
        api.nilObj
    } else {
        if let Some(payload) = unsafe { unit_payload(self_object) } {
            // SAFETY: phase transition happens after the foreign call returns.
            unsafe { (*payload).phase = UnitPhase::Configured };
        }
        self_object
    }
}

unsafe extern "C" fn m_add_library(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    unsafe { path_method(self_object, args, count, api, tcc_add_library) }
}

unsafe extern "C" fn m_add_library_path(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    unsafe { path_method(self_object, args, count, api, tcc_add_library_path) }
}

unsafe extern "C" fn m_add_file(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    unsafe { path_method(self_object, args, count, api, tcc_add_file) }
}

unsafe extern "C" fn m_relocate(self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let Some((state, phase)) = (unsafe { unit_snapshot(self_object) }) else {
        return api.nilObj;
    };
    if state.is_null()
        || matches!(
            phase,
            UnitPhase::Created | UnitPhase::Disposable | UnitPhase::Released
        )
    {
        return api.nilObj;
    }
    if matches!(phase, UnitPhase::Relocated | UnitPhase::Installed) {
        return self_object;
    }
    // SAFETY: this is the one-way libtcc relocation transition.
    #[cfg(feature = "resource-metrics")]
    let relocated = unsafe { resource_metrics::relocate(state) };
    #[cfg(not(feature = "resource-metrics"))]
    let relocated = unsafe { tcc_relocate(state) };
    if relocated < 0 {
        return api.nilObj;
    }
    if let Some(payload) = unsafe { unit_payload(self_object) } {
        // SAFETY: no foreign call is active while the phase byte changes.
        unsafe { (*payload).phase = UnitPhase::Relocated };
    }
    self_object
}

unsafe extern "C" fn m_add_symbol(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let (Some(name_object), Some(address_object)) = (unsafe { arg(args, count, 0) }, unsafe {
        arg(args, count, 1)
    }) else {
        return api.nilObj;
    };
    let Some(state) = (unsafe { configured_state(self_object) }) else {
        return api.nilObj;
    };
    let Some(name) = (unsafe { string_arg(api, name_object) }) else {
        return api.nilObj;
    };
    let Some((address, _kind)) = (unsafe { foreign_address::value(api, address_object) }) else {
        return api.nilObj;
    };
    // SAFETY: name owns the C string and state is pre-relocation.
    if unsafe { tcc_add_symbol(state, name.as_ptr(), address as *const c_void) } < 0 {
        api.nilObj
    } else {
        if let Some(payload) = unsafe { unit_payload(self_object) } {
            // SAFETY: phase transition happens after the foreign call returns.
            unsafe { (*payload).phase = UnitPhase::Configured };
        }
        self_object
    }
}

unsafe extern "C" fn m_install(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let (Some(name_object), Some(selector), Some(target)) = (
        unsafe { arg(args, count, 0) },
        unsafe { arg(args, count, 1) },
        unsafe { arg(args, count, 2) },
    ) else {
        return api.nilObj;
    };
    let Some(state) = (unsafe { relocated_state(self_object) }) else {
        return api.nilObj;
    };
    let Some(name) = (unsafe { string_arg(api, name_object) }) else {
        return api.nilObj;
    };
    // SAFETY: name remains live through symbol lookup.
    let symbol = unsafe { tcc_get_symbol(state, name.as_ptr()) };
    if symbol.is_null() {
        return api.nilObj;
    }
    // SAFETY: the generated function has the frozen Method ABI.
    let method: Method = unsafe { core::mem::transmute(symbol) };
    // SAFETY: these IdApi calls are the host's canonical vtable mutation path;
    // no Rust reference is held across either callback.
    let private_vtable = unsafe { (api.ensurePrivateVtable)(target) };
    unsafe { (api.method)(private_vtable, selector, method) };
    if let Some(payload) = unsafe { unit_payload(self_object) } {
        // SAFETY: the unit remains live and no foreign call is active.
        unsafe { (*payload).phase = UnitPhase::Installed };
    }
    private_vtable
}

unsafe extern "C" fn unit_finalize(object: Oop) {
    if let Some(payload) = unsafe { unit_payload(object) } {
        let generation = unsafe { (*payload).code_generation_identity };
        unsafe { (*payload).code_generation_identity = 0 };
        if generation != 0 {
            static ADDRESS: OnceLock<usize> = OnceLock::new();
            let runtime = RUNTIME
                .get()
                .unwrap_or_else(|| abort("TCC runtime was not initialized"));
            let Some(retire) = floor_retirement(
                unsafe { api_ref(runtime) },
                c"birnam_id_retire_stackless_code_generation_v1",
                &ADDRESS,
            ) else {
                abort("portable TCC generation retirement seam is unavailable");
            };
            if unsafe { retire(generation) } != 1 {
                abort("portable TCC generation remained live at finalization");
            }
        }
        unsafe { release_payload(payload) };
    }
}

unsafe extern "C" fn m_own_code_generation(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let Some(identity) = (unsafe { arg(args, count, 0) }) else {
        return api.nilObj;
    };
    if unsafe { (api.isSmallInt)(identity) } == 0 {
        return api.nilObj;
    }
    let identity = unsafe { (api.smallIntValue)(identity) };
    if identity <= 0 {
        return api.nilObj;
    }
    let Some(payload) = (unsafe { unit_payload(self_object) }) else {
        return api.nilObj;
    };
    let phase = unsafe { (*payload).phase };
    if phase != UnitPhase::Relocated || unsafe { (*payload).code_generation_identity } != 0 {
        return api.nilObj;
    }
    unsafe { (*payload).code_generation_identity = identity as u64 };
    self_object
}

unsafe extern "C" fn m_disposable(self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let Some((state, phase)) = (unsafe { unit_snapshot(self_object) }) else {
        return api.nilObj;
    };
    if state.is_null()
        || matches!(
            phase,
            UnitPhase::Installed | UnitPhase::Disposable | UnitPhase::Released
        )
    {
        return api.nilObj;
    }
    if let Some(payload) = unsafe { unit_payload(self_object) } {
        // SAFETY: the finalizer is registered before this unit is returned to
        // Birnam and it performs the one-way release transition.
        unsafe {
            (*payload).phase = UnitPhase::Disposable;
            (api.setFinalizer)(self_object, unit_finalize);
        }
    }
    self_object
}

unsafe extern "C" fn m_symbol(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let Some(name_object) = (unsafe { arg(args, count, 0) }) else {
        return api.nilObj;
    };
    let Some(state) = (unsafe { relocated_state(self_object) }) else {
        return api.nilObj;
    };
    let Some(name) = (unsafe { string_arg(api, name_object) }) else {
        return api.nilObj;
    };
    // SAFETY: name owns the NUL-terminated lookup key.
    let symbol = unsafe { tcc_get_symbol(state, name.as_ptr()) };
    if symbol.is_null() {
        api.nilObj
    } else {
        unsafe { foreign_address::allocate(api, symbol as usize, KIND_BORROWED) }
            .unwrap_or(api.nilObj)
    }
}

fn output_type(api: &IdApi, object: Oop) -> Option<c_int> {
    // SAFETY: vtableOf and symbolName are read-only IdApi queries.
    if unsafe { (api.vtableOf)(object) } != api.symbol_vt {
        return None;
    }
    let name = unsafe { CStr::from_ptr((api.symbolName)(object)) };
    match name.to_bytes() {
        b"dll" => Some(TCC_OUTPUT_DLL),
        b"obj" => Some(TCC_OUTPUT_OBJ),
        b"exe" => Some(TCC_OUTPUT_EXE),
        _ => None,
    }
}

unsafe extern "C" fn m_compile_output(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let (Some(source_object), Some(path_object), Some(kind)) = (
        unsafe { arg(args, count, 0) },
        unsafe { arg(args, count, 1) },
        unsafe { arg(args, count, 2) },
    ) else {
        return api.nilObj;
    };
    let Some(output) = output_type(api, kind) else {
        return api.nilObj;
    };
    let Some(source) = (unsafe { string_arg(api, source_object) }) else {
        return api.nilObj;
    };
    let Some(path) = (unsafe { string_arg(api, path_object) }) else {
        return api.nilObj;
    };
    // SAFETY: the new state is owned until output succeeds or the error path
    // deletes it immediately.
    let state = unsafe { tcc_new() };
    if state.is_null() {
        return api.nilObj;
    }
    unsafe {
        tcc_set_error_func(state, ptr::null_mut(), Some(error_silent));
        if tcc_set_output_type(state, output) < 0
            || tcc_compile_string(state, source.as_ptr()) < 0
            || tcc_output_file(state, path.as_ptr()) < 0
        {
            tcc_delete(state);
            return api.nilObj;
        }
        tcc_delete(state);
    }
    path_object
}

// -- generated-C runtime symbols -------------------------------------------

#[unsafe(export_name = "birnam_send")]
pub unsafe extern "C" fn rt_send(
    receiver: Oop,
    selector: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).send)(receiver, selector, args, count) }
}

/// Forward the private registration ABI into the exact selected floor while
/// keeping that floor `RTLD_LOCAL`, like every other generated-C runtime shim.
#[unsafe(export_name = "birnam_register_resumable_v1")]
pub unsafe extern "C" fn rt_register_resumable_v1(envelope: *mut c_void) -> u32 {
    static ADDRESS: OnceLock<usize> = OnceLock::new();
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let Some(register) = floor_registration(
        unsafe { api_ref(runtime) },
        c"birnam_register_resumable_v1",
        &ADDRESS,
    ) else {
        return 1;
    };
    unsafe { register(envelope) }
}

/// Design 74 E1 (development-only): forward the guarded direct slot read to
/// the selected floor. Answers 0 (ineligible) when the floor export is absent,
/// so a candidate unit always still has its ordinary send fallback.
#[cfg(feature = "dev-guarded-slots")]
#[unsafe(export_name = "birnam_dev_frame_oop_at_v1")]
pub unsafe extern "C" fn rt_dev_frame_oop_at_v1(machine: Oop, operand: Oop, out: *mut Oop) -> u32 {
    static ADDRESS: OnceLock<usize> = OnceLock::new();
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let address = floor_symbol_address(
        unsafe { api_ref(runtime) },
        c"birnam_dev_frame_oop_at_v1",
        &ADDRESS,
    );
    if address == 0 {
        return 0;
    }
    let forward = unsafe {
        core::mem::transmute::<usize, unsafe extern "C" fn(Oop, Oop, *mut Oop) -> u32>(address)
    };
    unsafe { forward(machine, operand, out) }
}

/// Design 74 E1 (development-only): write form of `birnam_dev_frame_oop_at_v1`.
#[cfg(feature = "dev-guarded-slots")]
#[unsafe(export_name = "birnam_dev_frame_oop_at_put_v1")]
pub unsafe extern "C" fn rt_dev_frame_oop_at_put_v1(
    machine: Oop,
    operand: Oop,
    value: Oop,
    out: *mut Oop,
) -> u32 {
    static ADDRESS: OnceLock<usize> = OnceLock::new();
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let address = floor_symbol_address(
        unsafe { api_ref(runtime) },
        c"birnam_dev_frame_oop_at_put_v1",
        &ADDRESS,
    );
    if address == 0 {
        return 0;
    }
    let forward = unsafe {
        core::mem::transmute::<usize, unsafe extern "C" fn(Oop, Oop, Oop, *mut Oop) -> u32>(address)
    };
    unsafe { forward(machine, operand, value, out) }
}

/// Design 74D E4 (development-only): unboxed-index slot read; the floor
/// performs the fallback send itself, so this always answers an Oop.
#[cfg(feature = "dev-guarded-slots")]
#[unsafe(export_name = "birnam_dev_frame_slot_get_v1")]
pub unsafe extern "C" fn rt_dev_frame_slot_get_v1(machine: Oop, index: c_long) -> Oop {
    static ADDRESS: OnceLock<usize> = OnceLock::new();
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let address = floor_symbol_address(
        unsafe { api_ref(runtime) },
        c"birnam_dev_frame_slot_get_v1",
        &ADDRESS,
    );
    if address == 0 {
        abort("dev slot access floor symbol unavailable");
    }
    let forward =
        unsafe { core::mem::transmute::<usize, unsafe extern "C" fn(Oop, c_long) -> Oop>(address) };
    unsafe { forward(machine, index) }
}

/// Design 74D E4 (development-only): write form of `birnam_dev_frame_slot_get_v1`.
#[cfg(feature = "dev-guarded-slots")]
#[unsafe(export_name = "birnam_dev_frame_slot_put_v1")]
pub unsafe extern "C" fn rt_dev_frame_slot_put_v1(machine: Oop, index: c_long, value: Oop) -> Oop {
    static ADDRESS: OnceLock<usize> = OnceLock::new();
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let address = floor_symbol_address(
        unsafe { api_ref(runtime) },
        c"birnam_dev_frame_slot_put_v1",
        &ADDRESS,
    );
    if address == 0 {
        abort("dev slot access floor symbol unavailable");
    }
    let forward = unsafe {
        core::mem::transmute::<usize, unsafe extern "C" fn(Oop, c_long, Oop) -> Oop>(address)
    };
    unsafe { forward(machine, index, value) }
}

/// Append-only registration-v2 forwarding for portable Block capture layout.
#[unsafe(export_name = "birnam_register_resumable_v2")]
pub unsafe extern "C" fn rt_register_resumable_v2(envelope: *mut c_void) -> u32 {
    static ADDRESS: OnceLock<usize> = OnceLock::new();
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let Some(register) = floor_registration(
        unsafe { api_ref(runtime) },
        c"birnam_register_resumable_v2",
        &ADDRESS,
    ) else {
        return 1;
    };
    unsafe { register(envelope) }
}

#[unsafe(export_name = "birnam_intern")]
pub unsafe extern "C" fn rt_intern(name: *const c_char) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).intern)(name) }
}

#[unsafe(export_name = "birnam_mkSmallInt")]
pub unsafe extern "C" fn rt_mk_small_int(value: i64) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).mkSmallInt)(value) }
}

#[unsafe(export_name = "birnam_smallIntValue")]
pub unsafe extern "C" fn rt_small_int_value(object: Oop) -> i64 {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).smallIntValue)(object) }
}

#[unsafe(export_name = "birnam_mkFloat")]
pub unsafe extern "C" fn rt_mk_float(value: f64) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).mkFloat)(value) }
}

#[unsafe(export_name = "birnam_floatValue")]
pub unsafe extern "C" fn rt_float_value(object: Oop) -> f64 {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).floatValue)(object) }
}

#[unsafe(export_name = "birnam_mkWord")]
pub unsafe extern "C" fn rt_mk_word(value: u64) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).mkWord)(value) }
}

#[unsafe(export_name = "birnam_wordValue")]
pub unsafe extern "C" fn rt_word_value(object: Oop) -> u64 {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).wordValue)(object) }
}

#[unsafe(export_name = "birnam_mkForeignAddress")]
pub unsafe extern "C" fn rt_mk_foreign_address(value: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    unsafe { foreign_address::allocate(api, value, KIND_BORROWED) }.unwrap_or(api.nilObj)
}

#[unsafe(export_name = "birnam_foreignAddressValue")]
pub unsafe extern "C" fn rt_foreign_address_value(object: Oop) -> usize {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { foreign_address::value(api_ref(runtime), object) }
        .map(|(address, _kind)| address)
        .unwrap_or(0)
}

#[unsafe(export_name = "birnam_isForeignAddress")]
pub unsafe extern "C" fn rt_is_foreign_address(object: Oop) -> i64 {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    i64::from(unsafe { foreign_address::value(api_ref(runtime), object) }.is_some())
}

#[unsafe(export_name = "birnam_uintValue")]
pub unsafe extern "C" fn rt_uint_value(object: Oop) -> u64 {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    if unsafe { (api.isWord)(object) } != 0 {
        unsafe { (api.wordValue)(object) }
    } else {
        unsafe { (api.smallIntValue)(object) as u64 }
    }
}

#[unsafe(export_name = "birnam_mkString")]
pub unsafe extern "C" fn rt_mk_string(bytes: *const u8, length: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    if length != 0 && bytes.is_null() {
        return api.nilObj;
    }
    unsafe { (api.mkString)(bytes, length) }
}

#[unsafe(export_name = "birnam_nil")]
pub unsafe extern "C" fn rt_nil() -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { api_ref(runtime) }.nilObj
}

#[unsafe(export_name = "birnam_true")]
pub unsafe extern "C" fn rt_true() -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { api_ref(runtime) }.trueObj
}

#[unsafe(export_name = "birnam_false")]
pub unsafe extern "C" fn rt_false() -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { api_ref(runtime) }.falseObj
}

#[unsafe(export_name = "birnam_objectVt")]
pub unsafe extern "C" fn rt_object_vt() -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { api_ref(runtime) }.object_vt
}

#[unsafe(export_name = "birnam_blockVt")]
pub unsafe extern "C" fn rt_block_vt() -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let block = unsafe { import(api, b"BlockVt\0") };
    if block.is_null() {
        api.object_vt
    } else {
        block
    }
}

#[unsafe(export_name = "birnam_proto")]
pub unsafe extern "C" fn rt_proto(parent: Oop) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).proto)(parent) }
}

#[unsafe(export_name = "birnam_ensurePrivateVtable")]
pub unsafe extern "C" fn rt_ensure_private_vtable(object: Oop) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).ensurePrivateVtable)(object) }
}

#[unsafe(export_name = "birnam_method")]
pub unsafe extern "C" fn rt_method(vtable: Oop, selector: Oop, function: *mut c_void) {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    // SAFETY: generated code supplies the frozen Method ABI.
    let method: Method = unsafe { core::mem::transmute(function) };
    unsafe { (api_ref(runtime).method)(vtable, selector, method) };
}

#[unsafe(export_name = "birnam_alloc")]
pub unsafe extern "C" fn rt_alloc(vtable: Oop, bytes: i64) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    if bytes < 0 {
        return unsafe { api_ref(runtime) }.nilObj;
    }
    unsafe { (api_ref(runtime).alloc)(vtable, bytes as usize) }
}

#[unsafe(export_name = "birnam_cstr")]
pub unsafe extern "C" fn rt_cstr(object: Oop) -> *mut c_char {
    unsafe extern "C" {
        fn malloc(size: usize) -> *mut c_void;
    }
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let mut length = 0usize;
    let bytes = unsafe { (api.stringBytes)(object, &mut length) };
    if length != 0 && bytes.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: stringBytes provides the reported readable span.
    let slice = unsafe { core::slice::from_raw_parts(bytes, length) };
    let output = unsafe { malloc(length + 1) }.cast::<u8>();
    if output.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: malloc returned at least length + 1 bytes.
    unsafe {
        ptr::copy_nonoverlapping(slice.as_ptr(), output, length);
        *output.add(length) = 0;
    }
    output.cast::<c_char>()
}

#[unsafe(export_name = "birnam_free_cstr")]
pub unsafe extern "C" fn rt_free_cstr(pointer: *mut c_char) {
    unsafe extern "C" {
        fn free(pointer: *mut c_void);
    }
    if !pointer.is_null() {
        // SAFETY: pointer came from rt_cstr's malloc.
        unsafe { free(pointer.cast()) };
    }
}

#[unsafe(export_name = "birnam_layoutAllOops")]
pub unsafe extern "C" fn rt_layout_all_oops(vtable: Oop) {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    unsafe { (api_ref(runtime).setLayout)(vtable, ptr::null(), 0, SLOT_OOP) };
}

struct RuntimeSymbol {
    name: &'static [u8],
    address: *const c_void,
}

// Raw function addresses are process-lifetime constants; the table is only
// consumed by tcc_add_symbol before a unit is relocated.
unsafe impl Sync for RuntimeSymbol {}

static RUNTIME_SYMBOLS: &[RuntimeSymbol] = &[
    #[cfg(feature = "dev-guarded-slots")]
    RuntimeSymbol {
        name: b"birnam_dev_frame_oop_at_v1\0",
        address: rt_dev_frame_oop_at_v1 as *const c_void,
    },
    #[cfg(feature = "dev-guarded-slots")]
    RuntimeSymbol {
        name: b"birnam_dev_frame_oop_at_put_v1\0",
        address: rt_dev_frame_oop_at_put_v1 as *const c_void,
    },
    #[cfg(feature = "dev-guarded-slots")]
    RuntimeSymbol {
        name: b"birnam_dev_frame_slot_get_v1\0",
        address: rt_dev_frame_slot_get_v1 as *const c_void,
    },
    #[cfg(feature = "dev-guarded-slots")]
    RuntimeSymbol {
        name: b"birnam_dev_frame_slot_put_v1\0",
        address: rt_dev_frame_slot_put_v1 as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_send\0",
        address: rt_send as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_intern\0",
        address: rt_intern as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_mkSmallInt\0",
        address: rt_mk_small_int as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_smallIntValue\0",
        address: rt_small_int_value as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_mkFloat\0",
        address: rt_mk_float as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_floatValue\0",
        address: rt_float_value as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_mkWord\0",
        address: rt_mk_word as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_wordValue\0",
        address: rt_word_value as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_mkForeignAddress\0",
        address: rt_mk_foreign_address as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_foreignAddressValue\0",
        address: rt_foreign_address_value as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_isForeignAddress\0",
        address: rt_is_foreign_address as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_uintValue\0",
        address: rt_uint_value as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_mkString\0",
        address: rt_mk_string as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_nil\0",
        address: rt_nil as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_true\0",
        address: rt_true as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_false\0",
        address: rt_false as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_objectVt\0",
        address: rt_object_vt as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_blockVt\0",
        address: rt_block_vt as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_proto\0",
        address: rt_proto as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_ensurePrivateVtable\0",
        address: rt_ensure_private_vtable as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_method\0",
        address: rt_method as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_alloc\0",
        address: rt_alloc as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_cstr\0",
        address: rt_cstr as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_free_cstr\0",
        address: rt_free_cstr as *const c_void,
    },
    RuntimeSymbol {
        name: b"birnam_layoutAllOops\0",
        address: rt_layout_all_oops as *const c_void,
    },
];

unsafe extern "C" fn m_bind_runtime(self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    let Some(state) = (unsafe { configured_state(self_object) }) else {
        return api.nilObj;
    };
    for symbol in RUNTIME_SYMBOLS {
        // SAFETY: names and addresses are process-lifetime constants.
        if unsafe { tcc_add_symbol(state, symbol.name.as_ptr().cast(), symbol.address) } < 0 {
            return api.nilObj;
        }
    }
    if let Some(payload) = unsafe { unit_payload(self_object) } {
        // SAFETY: phase transition happens after all foreign calls return.
        unsafe { (*payload).phase = UnitPhase::Configured };
    }
    self_object
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    if unsafe { foreign_address::ensure_family(api) }.is_none() {
        abort("ForeignAddress exports were incomplete");
    }
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { (api.isExported)(TCC_NAME.as_ptr().cast()) != 0 };
        let (tcc_vt, unit_vt, tcc) = if reloading {
            (
                unsafe { import(api, TCC_VT_NAME) },
                unsafe { import(api, UNIT_VT_NAME) },
                unsafe { import(api, TCC_NAME) },
            )
        } else {
            let tcc_vt = unsafe { (api.proto)(api.object_vt) };
            let unit_vt = unsafe { (api.proto)(api.object_vt) };
            let tcc = unsafe { (api.alloc)(tcc_vt, 0) };
            (tcc_vt, unit_vt, tcc)
        };
        if [tcc_vt, unit_vt, tcc].iter().any(|value| value.is_null()) {
            abort("TCC exports were incomplete");
        }
        Runtime {
            api: api_pointer as usize,
            tcc_vt: tcc_vt as usize,
            unit_vt: unit_vt as usize,
            tcc: tcc as usize,
        }
    });
    let api = unsafe { api_ref(&runtime) };
    ROOT_REGISTERED.get_or_init(|| unsafe {
        (api.gcRegisterRootProvider)(mark_runtime_roots);
    });
    let unit_vt = runtime.unit_vt as Oop;
    // The payload remains raw; the trace hook follows only this Unit's native
    // cache, without making that cache a process-wide GC root.
    unsafe {
        (api.setLayout)(unit_vt, ptr::null(), 0, SLOT_RAW);
        (api.setTraceHook)(unit_vt, unit_cache::trace);
    }
    let tcc_vt = runtime.tcc_vt as Oop;
    let tcc_methods: &[(&[u8], Method)] = &[
        (b"compile:\0", m_compile),
        (b"compile:output:as:\0", m_compile_output),
        (b"new\0", m_new),
        (b"liveUnitCount\0", m_live_unit_count),
        #[cfg(feature = "resource-metrics")]
        (b"vtableCacheUnitCount\0", unit_cache::unit_count),
        #[cfg(feature = "resource-metrics")]
        (b"vtableCacheEntryCount\0", unit_cache::entry_count),
        #[cfg(feature = "resource-metrics")]
        (b"checkUnitCacheContract\0", unit_cache::check_contract),
        #[cfg(feature = "resource-metrics")]
        (
            b"relocatedAllocationBytes\0",
            resource_metrics::allocation_bytes,
        ),
        #[cfg(feature = "allocation-metrics")]
        (
            b"heapAllocationBytes\0",
            resource_metrics::heap_allocation_bytes,
        ),
        #[cfg(feature = "allocation-metrics")]
        (
            b"heapAllocationCount\0",
            resource_metrics::heap_allocation_count,
        ),
    ];
    let unit_methods: &[(&[u8], Method)] = &[
        (b"compile:\0", m_compile_into_method),
        (b"addLibrary:\0", m_add_library),
        (b"addLibraryPath:\0", m_add_library_path),
        (b"addFile:\0", m_add_file),
        (b"bindRuntime\0", m_bind_runtime),
        (b"bindVtableCache\0", unit_cache::bind),
        (b"addSymbol:value:\0", m_add_symbol),
        (b"relocate\0", m_relocate),
        (b"ownCodeGeneration:\0", m_own_code_generation),
        (b"disposable\0", m_disposable),
        (b"install:as:on:\0", m_install),
        (b"symbol:\0", m_symbol),
    ];
    for (name, method) in tcc_methods {
        // SAFETY: names are static NUL-terminated selectors and method pointers
        // obey the frozen IdApi Method ABI.
        let selector = unsafe { (api.intern)(name.as_ptr().cast()) };
        unsafe { (api.method)(tcc_vt, selector, *method) };
    }
    for (name, method) in unit_methods {
        // SAFETY: names are static NUL-terminated selectors and method pointers
        // obey the frozen IdApi Method ABI.
        let selector = unsafe { (api.intern)(name.as_ptr().cast()) };
        unsafe { (api.method)(unit_vt, selector, *method) };
    }
    if unsafe { (api.isExported)(TCC_NAME.as_ptr().cast()) } == 0 {
        unsafe {
            let _ = (api.exportObj)(TCC_VT_NAME.as_ptr().cast(), tcc_vt);
            let _ = (api.exportObj)(UNIT_VT_NAME.as_ptr().cast(), unit_vt);
            let _ = (api.exportObj)(TCC_NAME.as_ptr().cast(), runtime.tcc as Oop);
        }
    }
}

/// Identifies the N4.2 state-machine implementation.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_tcc_rust_identity() -> *const c_char {
    RUST_TCC_IDENTITY.as_ptr().cast()
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the TCC rung against the process-owned IdApi.
///
/// # Safety
///
/// `api` must be the live host IdApi table or null for the documented fatal
/// boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    unsafe { bring_up(api) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_TCC_IDENTITY, birnam_tcc_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn state_identity_is_terminated() {
        // SAFETY: the export points to immutable crate storage for process life.
        let identity = unsafe { CStr::from_ptr(birnam_tcc_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_TCC_IDENTITY);
    }
}
