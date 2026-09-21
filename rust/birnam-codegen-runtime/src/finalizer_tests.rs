//! The commit seam is isolated here; installed image tests exercise real mapped
//! and rebuilt artifacts, generation retirement and executable ownership.
use super::*;
use birnam_abi::Finalizer;
use core::cell::{Cell, RefCell};

thread_local! {
    static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static END_FAIL: Cell<bool> = const { Cell::new(false) };
    static FINISH_FAIL: Cell<bool> = const { Cell::new(false) };
}
fn oop(address: usize) -> Oop { ptr::without_provenance_mut(address) }
// This fixture supplies the completed compiler predecessor required at first
// construction. No generated code is executed by this commit-timing test.
unsafe extern "C" fn open(_: *const c_char, _: c_int) -> *mut c_void { oop(1) }
unsafe extern "C" fn resolve(_: *mut c_void, name: *const u8, length: usize) -> *mut c_void {
    assert_eq!(unsafe { core::slice::from_raw_parts(name,length) }, b"birnam_register_resumable_v1");
    finish as *const () as *mut c_void
}
unsafe extern "C" fn compiler() -> *const birnam_abi::runtime_compiler::ApiV1 {
    thread_local! {
        static COMPILER: birnam_abi::runtime_compiler::ApiV1 = birnam_abi::runtime_compiler::ApiV1 {
            version: 1, byte_size: 32, context: oop(1), resolve: Some(resolve),
        };
    }
    COMPILER.with(core::ptr::from_ref)
}
unsafe extern "C" fn symbol(_: *mut c_void, name: *const c_char) -> *mut c_void {
    match unsafe { CStr::from_ptr(name) }.to_bytes() {
        b"birnam_runtime_compiler_provider_v1" => compiler as *const () as *mut c_void,
        b"birnam_runtime_register_method_owner_v1" => register_owner as *const () as *mut c_void,
        b"birnam_runtime_forget_method_owner_v1" => forget_owner as *const () as *mut c_void,
        _ => panic!("unexpected Runtime dependency"),
    }
}
unsafe extern "C" fn register_owner(_: *const c_void, _: Oop) -> i32 { panic!("commit fixture does not publish generated methods"); }
unsafe extern "C" fn forget_owner(_: Oop) { panic!("commit fixture does not release generated methods"); }
unsafe extern "C" fn intern(_: *const c_char) -> Oop { oop(10) }
unsafe extern "C" fn identity(value: Oop) -> Oop { value }
unsafe extern "C" fn method(_: Oop, _: Oop, _: Method) {}
unsafe extern "C" fn allocate(_: Oop, _: usize) -> Oop { panic!("adopted families should not allocate"); }
unsafe extern "C" fn send(_: Oop, _: Oop, _: *const Oop, _: usize) -> Oop { panic!("unexpected send"); }
unsafe extern "C" fn name(_: Oop) -> *const c_char { c"fixture".as_ptr() }
unsafe extern "C" fn count() -> usize { 0 }
unsafe extern "C" fn size(_: Oop) -> usize { 0 }
unsafe extern "C" fn contains(_: *mut c_void, _: *const c_char) -> c_int { 1 }
unsafe extern "C" fn import(_: *mut c_void, _: *const c_char) -> Oop { oop(10) }
unsafe extern "C" fn export(_: *mut c_void, _: *const c_char, value: Oop) -> Oop { value }
unsafe extern "C" fn roots(_: *mut c_void, _: birnam_abi::RootProvider) {}
unsafe extern "C" fn layout(_: *mut c_void, _: Oop, _: *const u8, _: usize, _: u8) {}
unsafe extern "C" fn kind(_: *mut c_void, _: Oop, _: usize) -> u8 { SLOT_RAW }
unsafe extern "C" fn end() -> u32 { EVENTS.with(|v| v.borrow_mut().push("end"));u32::from(END_FAIL.get()) }
unsafe extern "C" fn finish() -> u32 { EVENTS.with(|v| v.borrow_mut().push("finish"));u32::from(FINISH_FAIL.get()) }
unsafe extern "C" fn cancel() -> u32 { EVENTS.with(|v| v.borrow_mut().push("cancel"));0 }
fn registered(context: *mut c_void, object: Oop, callback: Finalizer, expected_context: usize, event: &'static str) {
    assert_eq!(context.addr(), expected_context);
    assert_eq!(callback as usize, portable_mapped_unit_finalize as *const () as usize);
    let payload = unsafe { &*object.cast::<PortableMappedUnitPayload>() };
    assert_eq!((payload.handle, payload.code_generation_identity), (0, 73));
    assert!(PORTABLE_IMAGE_STAGE.try_lock().unwrap().is_none(), "registration precedes publication and holds no stage borrow");
    EVENTS.with(|v| { assert_eq!(*v.borrow(), ["end", "finish"]);v.borrow_mut().push(event); });
}
unsafe extern "C" fn first(context: *mut c_void, object: Oop, callback: Finalizer) { registered(context, object, callback, 11, "first"); }
unsafe extern "C" fn saved(context: *mut c_void, object: Oop, callback: Finalizer) { registered(context, object, callback, 12, "saved"); }
unsafe extern "C" fn forbidden(_: *mut c_void, _: Oop, _: Finalizer) { panic!("mapped commit selected incoming finalizers"); }
fn stage(owner: Oop, prepared: u32) {
    unsafe { owner.cast::<PortableMappedUnitPayload>().write(PortableMappedUnitPayload { handle: 0, code_generation_identity: 73 }) };
    *PORTABLE_IMAGE_STAGE.lock().unwrap() = Some(PortableImageStage {
        carrier_kind: PortableImageCarrierKind::Mapped, owner: owner.addr(), handle: 0,
        generation: 73, authority: [0; 32], bindings: vec![], binding_count: 1,
        program_count: 0, generated_bytes: 0, finish: finish as *const () as usize,
        cancel: cancel as *const () as usize, end_context: end as *const () as usize,
        prepare_binding: 0, binding_entry_address: 0, committed: false,
        prepared_bindings: prepared, installed_bindings: 0,
    });
    EVENTS.with(|v| v.borrow_mut().clear());
}

#[test]
fn mapped_commit_retains_first_finalizers_reads_live_selection_and_preserves_commit_timing() {
    let api = Box::into_raw(Box::new(IdApi {
        dlopen: open, dlsym: symbol, object_vt: oop(10), vtable_vt: oop(10), symbol_vt: oop(10),
        intern, proto: identity, method, alloc: allocate, send,
        nilObj: oop(1), trueObj: oop(2), falseObj: oop(3), symbolName: name, vtableOf: identity,
        gcCollect: count, gcLiveCount: count, objectPayloadSize: size,
    }));
    let other = Box::into_raw(Box::new(unsafe { ptr::read(api) }));
    unsafe { (*other).nilObj = oop(21); (*other).trueObj = oop(22); }
    let presence = Box::leak(Box::new(birnam_abi::namespace_presence::ApiV1 {
        version: 1, byte_size: 32, context: ptr::null_mut(), contains: Some(contains),
    }));
    let imports = Box::leak(Box::new(birnam_abi::namespace_import::ApiV1 {
        version: 1, byte_size: 32, context: ptr::null_mut(), import: Some(import),
    }));
    let exports = Box::leak(Box::new(birnam_abi::namespace_export::ApiV1 {
        version: 1, byte_size: 32, context: ptr::null_mut(), export: Some(export),
    }));
    let roots = Box::leak(Box::new(birnam_abi::root_registry::ApiV1 {
        version: 1, byte_size: 32, context: ptr::null_mut(), register: Some(roots),
    }));
    let layouts = Box::leak(Box::new(birnam_abi::payload_layout::ApiV1 {
        version: 1, byte_size: 40, context: ptr::null_mut(), set: Some(layout), slot_kind: Some(kind),
    }));
    let selected = Box::into_raw(Box::new(birnam_abi::finalizers::ApiV1 {
        version: 1, byte_size: 32, context: oop(11), set: Some(first),
    }));
    let incoming = Box::leak(Box::new(birnam_abi::finalizers::ApiV1 {
        version: 1, byte_size: 32, context: oop(99), set: Some(forbidden),
    }));
    let mount = |table, capability| unsafe {
        module_init(table, ptr::null_mut(), presence, imports, exports, ptr::null(), ptr::null(), roots,
            ptr::null(), layouts, ptr::null(), capability)
    };
    assert!(mount(api, selected).is_null());
    let mut payload = PortableMappedUnitPayload { handle: 0, code_generation_identity: 0 };
    let owner = ptr::from_mut(&mut payload).cast();
    for round in 0..3 {
        if round != 0 {
            unsafe { (*selected).context = oop(12); (*selected).set = Some(saved); }
            assert!(mount(other, incoming).is_null());
        }
        stage(owner, 0);
        assert_eq!(unsafe { rust_codegen_finish_portable_image_unit(oop(10), ptr::null(), 0) }, oop(1));
        EVENTS.with(|v| assert!(v.borrow().is_empty()));
        assert!(!PORTABLE_IMAGE_STAGE.lock().unwrap().as_ref().unwrap().committed);
        for (end_fails, finish_fails, expected) in [(true, false, vec!["end", "cancel"]), (false, true, vec!["end", "finish"])] {
            stage(owner, 1); END_FAIL.set(end_fails); FINISH_FAIL.set(finish_fails);
            assert_eq!(unsafe { rust_codegen_finish_portable_image_unit(oop(10), ptr::null(), 0) }, oop(1));
            EVENTS.with(|v| assert_eq!(*v.borrow(), expected));
            assert_eq!(payload.code_generation_identity, 0);
            assert!(PORTABLE_IMAGE_STAGE.lock().unwrap().is_none());
        }
        END_FAIL.set(false); FINISH_FAIL.set(false); stage(owner, 1);
        assert_eq!(unsafe { rust_codegen_finish_portable_image_unit(oop(10), ptr::null(), 0) }, oop(2));
        EVENTS.with(|v| assert_eq!(*v.borrow(), ["end", "finish", if round == 0 { "first" } else { "saved" }]));
        assert!(PORTABLE_IMAGE_STAGE.lock().unwrap().take().unwrap().committed);
    }
}
