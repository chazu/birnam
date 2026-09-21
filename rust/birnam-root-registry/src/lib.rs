//! The existing ordered native root-provider registry, independently owned.
use birnam_abi::root_registry::{ApiV1, CAPACITY, FactoryV1, FloorV1, SnapshotV1, OwnedV1, RootsV1};
use birnam_abi::RootProvider;
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Registry {
    api: ApiV1,
    floor: FloorV1,
    providers: Vec<RootProvider>,
    owned: OwnedV1,
    retained: Vec<birnam_abi::Oop>,
    root_tokens: Vec<usize>,
    next_root: usize,
    owner: std::thread::ThreadId,
}
unsafe extern "C" fn append(api: *mut ApiV1, provider: RootProvider) -> c_int {
    let registry = unsafe { &mut *api.cast::<Registry>() };
    if registry.providers.len() == CAPACITY { return 0; }
    registry.providers.push(provider);
    1
}
unsafe extern "C" fn register(context: *mut c_void, provider: RootProvider) {
    let owned = context.cast::<Registry>();
    let floor = unsafe { (*owned).floor };
    // No registry borrow survives admission, which can reenter registration.
    unsafe { floor.admit.unwrap()(floor.context) };
    if unsafe { append(owned.cast(), provider) } == 0 {
        unsafe { floor.overflow.unwrap()(floor.context) };
    }
}
unsafe extern "C" fn snapshot(api: *const ApiV1) -> SnapshotV1 {
    let providers = unsafe { &(*api.cast::<Registry>()).providers };
    let mut result = SnapshotV1 { count: providers.len(), entries: [None; CAPACITY] };
    for (slot, provider) in result.entries.iter_mut().zip(providers) { *slot = Some(*provider); }
    result
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.admit.is_none() || floor.overflow.is_none() {
        return ptr::null_mut();
    }
    let mut owned = Box::new(Registry {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(), register: Some(register) },
        floor, providers: Vec::new(), retained: Vec::new(), root_tokens: Vec::new(), next_root: 0, owner: std::thread::current().id(),
        owned: OwnedV1 { version: 1, byte_size: size_of::<OwnedV1>(), api: ptr::null_mut(), factory: &FACTORY,
            context: ptr::null_mut(), retain: Some(retain), release: Some(release), roots: Some(roots), set_admission: Some(set_admission) },
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    owned.owned.context = owned.api.context;
    owned.owned.api = ptr::from_mut(&mut owned.api);
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Registry>())) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1, byte_size: size_of::<FactoryV1>(), create: Some(create), destroy: Some(destroy),
    append: Some(append), snapshot: Some(snapshot),
};

unsafe extern "C" fn admit_native(context: *mut c_void) {
    assert_eq!(unsafe { (*context.cast::<Registry>()).owner }, std::thread::current().id(), "root registry used from another thread");
}
unsafe extern "C" fn overflow_native(_: *mut c_void) -> ! {
    eprintln!("gc: too many root providers");
    std::process::abort()
}
unsafe extern "C" fn retain(context: *mut c_void, object: birnam_abi::Oop) -> usize {
    unsafe { admit_mutation(context) };
    if object.is_null() { return 0; }
    let registry = unsafe { &mut *context.cast::<Registry>() };
    registry.next_root = registry.next_root.checked_add(1).expect("root token space exhausted");
    registry.retained.push(object);
    registry.root_tokens.push(registry.next_root);
    registry.next_root
}
unsafe extern "C" fn release(context: *mut c_void, token: usize) -> c_int {
    unsafe { admit_mutation(context) };
    let registry = unsafe { &mut *context.cast::<Registry>() };
    let Some(index) = registry.root_tokens.iter().position(|candidate| *candidate == token) else { return 0; };
    registry.root_tokens.swap_remove(index);
    registry.retained.swap_remove(index);
    1
}
unsafe extern "C" fn roots(context: *mut c_void) -> RootsV1 {
    unsafe { admit_native(context) };
    let registry = unsafe { &*context.cast::<Registry>() };
    RootsV1 { entries: registry.retained.as_ptr(), count: registry.retained.len() }
}
unsafe fn admit_mutation(context: *mut c_void) {
    let floor = unsafe { (*context.cast::<Registry>()).floor };
    // The current working admission policy may reenter. Borrow records afterward.
    unsafe { floor.admit.unwrap()(floor.context) };
}
unsafe extern "C" fn set_admission(context: *mut c_void, floor: *const FloorV1) -> c_int {
    unsafe { admit_native(context) };
    if floor.is_null() { return 0; }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.admit.is_none() || floor.overflow.is_none() { return 0; }
    unsafe { (*context.cast::<Registry>()).floor = floor };
    1
}

unsafe extern "C" fn initialize_native(id: *const birnam_abi::id_protocol::Protocol, args: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Operand, Outcome};
    if id.is_null() { return Outcome::failed(native::INVALID_REQUEST, 0); }
    let Some(args) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let Some(initial): Option<Vec<_>> = args.iter().map(|arg| arg.as_object()).collect() else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let floor = FloorV1 { version: 1, byte_size: size_of::<FloorV1>(), context: ptr::null_mut(), admit: Some(admit_native), overflow: Some(overflow_native) };
    let api = unsafe { create(&floor) };
    let registry = api.cast::<Registry>();
    // Complete the native owner's self-reference before publishing it. No
    // operation needs a later Runtime, collector or prepared callback.
    unsafe { (*registry).floor.context = registry.cast() };
    for object in initial { unsafe { retain(registry.cast(), object) }; }
    Outcome::loaded(Operand::word(unsafe { &raw const (*registry).owned } as usize))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: birnam_abi::id_protocol::Module = birnam_abi::id_protocol::Module {
    version: birnam_abi::id_protocol::VERSION, byte_size: size_of::<birnam_abi::id_protocol::Module>() as u32, initialize: Some(initialize_native),
};
/// # Safety
/// Owner-thread operations require the declared descriptor and code lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_root_registry_factory_v1() -> *const FactoryV1 { &FACTORY }

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::{MarkFn, Oop};
    use core::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Admission {
        calls: Cell<usize>,
        reenter: Cell<bool>,
        api: Cell<*mut ApiV1>,
    }
    unsafe extern "C" fn admit(context: *mut c_void) {
        let state = unsafe { &*context.cast::<Admission>() };
        state.calls.set(state.calls.get() + 1);
        if state.reenter.replace(false) {
            unsafe { birnam_abi::root_registry::register(state.api.get(), second) };
        }
    }
    unsafe extern "C" fn overflow(_: *mut c_void) -> ! {
        eprintln!("gc: too many root providers");
        std::process::exit(71)
    }
    fn floor(state: &Admission) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(),
            context: ptr::from_ref(state).cast_mut().cast(), admit: Some(admit), overflow: Some(overflow) }
    }
    unsafe extern "C" fn first(mark: MarkFn) { unsafe { mark(1usize as Oop) }; }
    unsafe extern "C" fn second(mark: MarkFn) { unsafe { mark(2usize as Oop) }; }
    thread_local! { static MARKED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) }; }
    unsafe extern "C" fn mark(object: Oop) { MARKED.with(|values| values.borrow_mut().push(object.addr())); }
    fn invoke(view: SnapshotV1) -> Vec<usize> {
        MARKED.with(|values| values.borrow_mut().clear());
        assert!(view.entries[view.count..].iter().all(Option::is_none));
        for provider in &view.entries[..view.count] { unsafe { provider.unwrap()(mark) }; }
        MARKED.with(|values| values.borrow().clone())
    }

    #[test]
    fn creation_validates_inertly_and_copies_floor() {
        let state = Admission::default();
        let mut selected = floor(&state);
        unsafe {
            assert!(create(ptr::null()).is_null());
            selected.version = 2; assert!(create(&selected).is_null());
            selected = floor(&state); selected.byte_size -= 1; assert!(create(&selected).is_null());
            selected = floor(&state); selected.admit = None; assert!(create(&selected).is_null());
            selected = floor(&state); selected.overflow = None; assert!(create(&selected).is_null());
            selected = floor(&state);
            let api = create(&selected); assert!(!api.is_null());
            assert_eq!(state.calls.get(), 0);
            selected.context = ptr::null_mut(); selected.admit = None;
            assert!(selected.admit.is_none());
            birnam_abi::root_registry::register(api, first);
            assert_eq!(state.calls.get(), 1);
            assert_eq!(invoke(snapshot(api)), [1]);
            destroy(api);
            assert_eq!(state.calls.get(), 1);
        }
    }

    #[test]
    fn duplicates_consume_all_32_slots_and_failed_append_leaves_order_unchanged() {
        let state = Admission::default();
        unsafe {
            let api = create(&floor(&state));
            for index in 0..CAPACITY { assert_eq!(append(api, if index % 2 == 0 { first } else { second }), 1); }
            assert_eq!(append(api, first), 0);
            assert_eq!(state.calls.get(), 0);
            assert_eq!(invoke(snapshot(api)), (0..32).map(|i| i % 2 + 1).collect::<Vec<_>>());
            destroy(api);
        }
    }

    #[test]
    fn snapshot_is_owned_and_canonical_operations_ignore_mutable_public_selection() {
        let state = Admission::default();
        unsafe {
            let api = create(&floor(&state));
            assert_eq!(append(api, first), 1);
            let before = snapshot(api);
            (*api).context = ptr::null_mut(); (*api).register = None;
            assert_eq!(append(api, second), 1);
            let after = snapshot(api);
            destroy(api);
            assert_eq!(invoke(before), [1]);
            assert_eq!(invoke(after), [1, 2]);
            assert_eq!(state.calls.get(), 0);
        }
    }

    #[test]
    fn admission_can_reenter_and_inner_registration_precedes_outer_append() {
        let state = Admission::default();
        unsafe {
            let api = create(&floor(&state));
            state.api.set(api); state.reenter.set(true);
            birnam_abi::root_registry::register(api, first);
            assert_eq!(state.calls.get(), 2);
            assert_eq!(invoke(snapshot(api)), [2, 1]);
            destroy(api);
        }
    }

    #[test]
    fn public_overflow_occurs_after_admission_and_reentrant_last_append() {
        const CHILD: &str = "BIRNAM_ROOT_REGISTRY_OVERFLOW_CHILD";
        if std::env::var_os(CHILD).is_some() {
            unsafe extern "C" fn checked_overflow(context: *mut c_void) -> ! {
                let state = unsafe { &*context.cast::<Admission>() };
                let view = unsafe { snapshot(state.api.get()) };
                if state.calls.get() != 2 || view.count != CAPACITY || invoke(view).last() != Some(&2) {
                    std::process::exit(72);
                }
                unsafe { overflow(context) }
            }
            let state = Admission::default();
            let mut selected = floor(&state); selected.overflow = Some(checked_overflow);
            unsafe {
                let api = create(&selected); state.api.set(api);
                for _ in 0..CAPACITY - 1 { assert_eq!(append(api, first), 1); }
                state.reenter.set(true);
                birnam_abi::root_registry::register(api, first);
            }
            unreachable!("overflow must not return");
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::public_overflow_occurs_after_admission_and_reentrant_last_append", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert_eq!(output.status.code(), Some(71));
        assert_eq!(String::from_utf8(output.stderr).unwrap(), "gc: too many root providers\n");
    }
}
