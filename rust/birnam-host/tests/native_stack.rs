//! Fresh-process proof of the real host boundary against an installed stack.
use birnam_abi::{gc_settings, native_stack};
use core::cell::{Cell, RefCell};
use core::ffi::{c_int, c_void};
use std::ffi::CString;
use std::path::PathBuf;

struct Probe {
    original_stack: native_stack::ApiV1,
    original_settings: gc_settings::ApiV1,
    factory: native_stack::FactoryV2,
    selected: *const native_stack::ApiV1,
    expected: Cell<usize>,
    events: RefCell<Vec<&'static str>>,
    round: usize,
}
unsafe fn assigned(context: *mut c_void, address: usize, round: usize) {
    let probe = unsafe { &*context.cast::<Probe>() };
    assert_eq!(probe.round, round);
    assert_eq!(address, probe.expected.get());
    assert!(probe.events.borrow().is_empty());
    probe.events.borrow_mut().push("anchor");
    unsafe { (probe.original_stack.set.unwrap())(probe.original_stack.context, address) };
    assert_eq!(unsafe { probe.factory.get.unwrap()(probe.selected) }, address);
}
unsafe extern "C" fn first(context: *mut c_void, address: usize) { unsafe { assigned(context, address, 1) }; }
unsafe extern "C" fn second(context: *mut c_void, address: usize) { unsafe { assigned(context, address, 2) }; }
unsafe extern "C" fn precise(context: *mut c_void, enabled: c_int) {
    let probe = unsafe { &*context.cast::<Probe>() };
    assert_eq!(enabled, 1);
    assert_eq!(*probe.events.borrow(), ["anchor"]);
    assert_eq!(unsafe { probe.factory.get.unwrap()(probe.selected) }, probe.expected.get());
    probe.events.borrow_mut().push("precise");
    unsafe { probe.original_settings.set_precise.unwrap()(probe.original_settings.context, enabled) };
}

#[test]
#[ignore = "requires BIRNAM_NATIVE_STACK_PREFIX and a fresh process with an installed debug stack"]
fn real_host_uses_live_anchor_selection_before_optional_precise_policy() {
    let prefix = PathBuf::from(std::env::var_os("BIRNAM_NATIVE_STACK_PREFIX").unwrap());
    let resources = birnam_host::resolve_from_exe_dir(&prefix.join("bin"), birnam_host::Overrides::default()).unwrap();
    let mut runtime = unsafe { birnam_host::start(resources, birnam_host::StartupPolicy::from_flags(false, false)) }.unwrap();
    let selected = runtime.stack.native_stack().cast_mut();
    let settings = runtime.stack.gc_settings().cast_mut();
    let path = CString::new(prefix.join("lib/libbirnam_native_stack.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let api = runtime.stack.api();
    let image = unsafe { (api.dlopen)(path.as_ptr(), 6) };assert!(!image.is_null());
    let symbol = unsafe { (api.dlsym)(image, c"birnam_native_stack_factory_v2".as_ptr()) };assert!(!symbol.is_null());
    let provide: native_stack::FactoryProviderV2 = unsafe { std::mem::transmute(symbol) };
    let original_stack = unsafe { *selected };
    let original_settings = unsafe { *settings };
    let factory = unsafe { *provide() };
    for (round, callback) in [(1, first as unsafe extern "C" fn(*mut c_void, usize)), (2, second)] {
        let mut probe = Probe { original_stack, original_settings, factory, selected,
            expected: Cell::new(0), events: RefCell::new(Vec::new()), round };
        unsafe {
            (*selected).context = core::ptr::from_mut(&mut probe).cast();(*selected).set = Some(callback);
            (*settings).context = core::ptr::from_mut(&mut probe).cast();(*settings).set_precise = Some(precise);
        }
        for policy in [false, true, false] {
            runtime.policy.precise_gc = policy;
            let mut anchor = 0usize;probe.expected.set(core::ptr::from_mut(&mut anchor).addr());probe.events.borrow_mut().clear();
            unsafe { runtime.prepare_execution(&mut anchor) };
            assert_eq!(*probe.events.borrow(), if policy { vec!["anchor", "precise"] } else { vec!["anchor"] });
        }
        unsafe { selected.write(original_stack);settings.write(original_settings); }
    }
    // Do not retain the last loop-local stack anchor after its scope has ended.
    unsafe { native_stack::set(selected, 0) };
}
