use super::*;
use core::cell::{Cell, RefCell};

struct Floor {
    api: Cell<*mut ApiV1>,
    reenter: Cell<bool>,
    seen: RefCell<Vec<usize>>,
}
impl Floor {
    fn new() -> Self { Self { api: Cell::new(ptr::null_mut()), reenter: Cell::new(false), seen: RefCell::new(Vec::new()) } }
    fn descriptor(&self) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(),
            context: ptr::from_ref(self).cast_mut().cast(), admit: Some(admit) }
    }
}
unsafe extern "C" fn admit(context: *mut c_void) {
    let floor = unsafe { &*context.cast::<Floor>() };
    floor.seen.borrow_mut().push(unsafe { get(floor.api.get()) });
    if floor.reenter.replace(false) {
        unsafe { birnam_abi::native_stack::set(floor.api.get(), 17) };
        assert_eq!(unsafe { get(floor.api.get()) }, 17);
    }
}
#[test]
fn zero_default_and_all_address_bits_round_trip_after_admission() {
    let floor = Floor::new();
    let api = unsafe { create(&floor.descriptor()) };floor.api.set(api);
    assert_eq!(unsafe { get(api) }, 0);assert!(floor.seen.borrow().is_empty());
    let addresses = [0, 1, 3, usize::MAX, usize::MAX - 1, 0];
    let mut previous = 0;
    for address in addresses {
        unsafe { birnam_abi::native_stack::set(api, address) };
        assert_eq!(floor.seen.borrow().last(), Some(&previous));
        assert_eq!(unsafe { get(api) }, address);previous = address;
    }
    unsafe { destroy(api) };assert_eq!(floor.seen.borrow().len(), addresses.len());
}
#[test]
fn copied_floor_and_reentry_complete_before_the_outer_assignment() {
    let a = Floor::new();let b = Floor::new();
    let mut descriptor = a.descriptor();
    let api_a = unsafe { create(&descriptor) };a.api.set(api_a);
    descriptor.context = b.descriptor().context;
    let api_b = unsafe { create(&descriptor) };b.api.set(api_b);
    a.reenter.set(true);
    unsafe { birnam_abi::native_stack::set(api_a, 23) };
    assert_eq!(*a.seen.borrow(), [0, 0]);assert!(b.seen.borrow().is_empty());
    assert_eq!(unsafe { get(api_a) }, 23);assert_eq!(unsafe { get(api_b) }, 0);
    unsafe { destroy(api_a);birnam_abi::native_stack::set(api_b, 91);destroy(api_b); }
    assert_eq!(*b.seen.borrow(), [0]);
}
#[test]
fn live_public_selection_does_not_redirect_canonical_state_or_destruction() {
    let floor = Floor::new();
    let api = unsafe { create(&floor.descriptor()) };floor.api.set(api);
    unsafe extern "C" fn selected(context: *mut c_void, address: usize) {
        assert_eq!(context.addr(), 7);assert_eq!(address, 19);
    }
    unsafe {
        (*api).context = ptr::without_provenance_mut(7);(*api).set = Some(selected);
        birnam_abi::native_stack::set(api, 19);
        assert_eq!(get(api), 0);set(api, usize::MAX);assert_eq!(get(api), usize::MAX);
        (*api).context = ptr::null_mut();(*api).set = None;
        set(api, 0);assert_eq!(get(api), 0);destroy(api);
    }
    assert!(floor.seen.borrow().is_empty());
}
#[test]
fn descriptor_validation_and_teardown_are_inert() {
    let floor = Floor::new();
    unsafe {
        assert!(create(ptr::null()).is_null());
        let mut bad = floor.descriptor();bad.version += 1;assert!(create(&bad).is_null());
        bad = floor.descriptor();bad.byte_size -= 1;assert!(create(&bad).is_null());
        bad = floor.descriptor();bad.admit = None;assert!(create(&bad).is_null());
        let api = create(&floor.descriptor());
        assert!(birnam_abi::native_stack::valid(api));assert!(!birnam_abi::native_stack::valid(ptr::null()));
        let mut bad = *api;bad.version += 1;assert!(!birnam_abi::native_stack::valid(&bad));
        bad = *api;bad.byte_size -= 1;assert!(!birnam_abi::native_stack::valid(&bad));
        bad = *api;bad.set = None;assert!(!birnam_abi::native_stack::valid(&bad));destroy(api);
    }
    assert!(floor.seen.borrow().is_empty());
}

#[test]
fn scanner_emits_only_complete_words_in_order_for_every_byte_boundary() {
    unsafe extern "C" fn emit(context: *mut c_void, value: usize) -> i32 {
        unsafe { &mut *context.cast::<Vec<usize>>() }.push(value);
        birnam_abi::storage::OK
    }
    let words = [0, 7, 7, usize::MAX, 1usize << (usize::BITS - 1), 19];
    let mut values = Vec::<usize>::new();
    let sink = birnam_abi::trace_live::SinkV1 { context: (&raw mut values).cast(), emit };
    let base = words.as_ptr().addr(); let bytes = size_of_val(&words); let word = size_of::<usize>();
    for low in 0..=bytes {
        for high in 0..=bytes {
            values.clear();
            unsafe { scan(&sink, base + low, base + high) };
            let expected: Vec<_> = words.iter().enumerate()
                .filter_map(|(index, value)| (index * word >= low && (index + 1) * word <= high).then_some(*value)).collect();
            assert_eq!(values, expected, "byte interval {low}..{high}");
        }
    }
    values.clear(); unsafe { scan(&sink, 0, 0); scan(&sink, usize::MAX - word + 1, usize::MAX) };
    assert!(values.is_empty());
}

#[test]
fn scanner_reads_after_each_callback_and_supports_synchronous_emission_reentry() {
    struct Recording { words: *mut usize, values: RefCell<Vec<usize>>, entered: Cell<bool> }
    unsafe extern "C" fn emit(context: *mut c_void, value: usize) -> i32 {
        let state = unsafe { &*context.cast::<Recording>() };
        state.values.borrow_mut().push(value);
        if !state.entered.replace(true) {
            unsafe { state.words.add(2).write(99) };
            let nested = [41usize, 43];
            let sink = birnam_abi::trace_live::SinkV1 { context, emit };
            unsafe { scan(&sink, nested.as_ptr().addr(), nested.as_ptr().addr() + size_of_val(&nested)) };
        }
        birnam_abi::storage::OK
    }
    let mut words = [0usize, 7, 7, usize::MAX];
    let mut state = Recording { words: words.as_mut_ptr(), values: RefCell::new(Vec::new()), entered: Cell::new(false) };
    let sink = birnam_abi::trace_live::SinkV1 { context: (&raw mut state).cast(), emit };
    unsafe { scan(&sink, words.as_ptr().addr(), words.as_ptr().addr() + size_of_val(&words)) };
    assert_eq!(*state.values.borrow(), [0, 41, 43, 7, 99, usize::MAX]);
}

#[test]
fn scanner_rejects_before_another_candidate_with_the_original_diagnostic() {
    const CHILD: &str = "BIRNAM_STACK_SCAN_REJECT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        unsafe extern "C" fn reject(_: *mut c_void, value: usize) -> i32 {
            use std::io::Write;
            writeln!(std::io::stdout().lock(), "candidate {value}").unwrap();
            -77
        }
        let words = [17usize, 23];
        let sink = birnam_abi::trace_live::SinkV1 { context: ptr::null_mut(), emit: reject };
        unsafe { scan(&sink, words.as_ptr().addr(), words.as_ptr().addr() + size_of_val(&words)) };
        panic!("scanner returned after rejection");
    }
    use std::os::unix::process::ExitStatusExt;
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::scanner_rejects_before_another_candidate_with_the_original_diagnostic", "--nocapture"])
        .env(CHILD, "1").output().unwrap();
    assert_eq!(output.status.signal(), Some(6));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.ends_with("candidate 17\n") && !stdout.contains("candidate 23"));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "birnam-id invariant failed: GC tracer rejected a candidate\n");
}
