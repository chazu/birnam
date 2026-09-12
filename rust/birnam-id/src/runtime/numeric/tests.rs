use super::*;
use birnam_codegen::numeric_chain::{Operation, Operator, Plan, Value};
use birnam_native_compiler::Unit;
use core::ffi::c_void;
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

unsafe extern "C" {
    fn dlclose(handle: *mut c_void) -> core::ffi::c_int;
}

unsafe extern "C" fn make(value: i64) -> Oop {
    with_active(|r| r.make_integer(value))
}
unsafe extern "C" fn read(value: Oop) -> i64 {
    with_active(|r| r.integer_value(value))
}
unsafe extern "C" fn error(value: Oop) -> core::ffi::c_int {
    with_active(|r| i32::from(r.checked_vtable_of(value) == r.import(b"ErrorVt").unwrap()))
}

pub(in crate::runtime) struct Fixture {
    handle: Option<core::ptr::NonNull<c_void>>,
    directory: PathBuf,
}

impl Fixture {
    pub(in crate::runtime) fn load(r: &Runtime) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "birnam-numeric-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("numeric.dylib");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/fixtures/numeric-semantics.c");
        let status = std::process::Command::new("cc")
            .args(["-dynamiclib", "-O2"])
            .arg(source)
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let handle =
            core::ptr::NonNull::new(unsafe { crate::loader::open(name.as_ptr(), 0x1 | 0x4) })
                .unwrap();
        let fixture = Self {
            handle: Some(handle),
            directory,
        };
        let setup: unsafe extern "C" fn(*const c_void, *const c_void, *const c_void, Oop, Oop) =
            unsafe { core::mem::transmute(fixture.symbol(c"numeric_setup")) };
        unsafe {
            setup(
                make as *const c_void,
                read as *const c_void,
                error as *const c_void,
                r.canonical().nil,
                r.canonical().smallint_vt,
            )
        };
        fixture
    }

    fn symbol(&self, name: &CStr) -> *mut c_void {
        let pointer =
            unsafe { crate::loader::symbol(self.handle.unwrap().as_ptr(), name.as_ptr()) };
        assert!(!pointer.is_null());
        pointer
    }

    pub(in crate::runtime) fn method(&self, code: u32) -> Method {
        let name = CString::new(format!("numeric_{code}")).unwrap();
        unsafe { core::mem::transmute(self.symbol(&name)) }
    }

    fn calls(&self) -> usize {
        let calls: unsafe extern "C" fn() -> usize =
            unsafe { core::mem::transmute(self.symbol(c"numeric_calls")) };
        unsafe { calls() }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe { dlclose(handle.as_ptr()) };
        }
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

thread_local! {
    static GC_OPERAND: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static GC_OBSERVATIONS: Cell<usize> = const { Cell::new(0) };
}

unsafe extern "C" fn mutate_operand_at_collection(mark: crate::abi::MarkFn) {
    with_active(|r| {
        GC_OBSERVATIONS.set(GC_OBSERVATIONS.get() + 1);
        let input = GC_OPERAND.get();
        assert!(unsafe { (*r.state.get()).heap.write_payload_word(input, 0, 1000) });
        r.numeric.borrow_mut().records.clear();
        unsafe { mark(input) };
    });
}

#[test]
fn result_allocation_keeps_the_baseline_gc_boundary_and_precollection_values() {
    for enabled in [false, true] {
        struct Mode(bool);
        impl Drop for Mode {
            fn drop(&mut self) {
                ENABLED.set(self.0);
            }
        }
        let _mode = Mode(ENABLED.replace(enabled));
        let bootstrap = crate::dispatch::build_bootstrap();
        let r = &bootstrap.runtime;
        r.install_execution_objects();
        let fixture = Fixture::load(r);
        let method = fixture.method(1);
        let input = r.make_integer(11);
        let _input = ScopedOopRootGuard::new(r, input);
        let invoke = || {
            r.enter(|| {
                r.try_numeric_call(method, input, &[input])
                    .unwrap_or_else(|| unsafe { method(input, &input, 1) })
            })
        };
        for _ in 0..HOT_CALLS + 1 {
            invoke();
        }
        GC_OPERAND.set(input);
        GC_OBSERVATIONS.set(0);
        r.gc_register_root_provider(mutate_operand_at_collection);
        // Real floor allocation accounting, not a synthetic guard override.
        r.allocate_instance(r.globals().object_vt, crate::gc::AUTO_COLLECTION_THRESHOLD);
        let anchor = [0usize; 32];
        r.gc_set_stack_bottom(anchor.as_ptr().addr() + core::mem::size_of_val(&anchor));
        let result = invoke();
        assert_eq!(
            r.integer_value(result),
            22,
            "operands were read before collecting allocation"
        );
        assert_eq!(r.integer_value(input), 1000);
        assert!(GC_OBSERVATIONS.get() > 0);
        r.gc_set_stack_bottom(0);
        GC_OPERAND.set(core::ptr::null_mut());
    }
}

#[test]
fn native_numeric_declarations_pin_the_actual_image_and_reject_unknown_methods() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    let mut fixture = Fixture::load(r);
    let add = fixture.method(1);
    let summary = NumericSummary::discover(add, r.canonical().smallint_vt).unwrap();
    assert_eq!(summary.operator(r.canonical().smallint_vt), Some(Operator::Add));
    let unknown: Method = unsafe { core::mem::transmute(fixture.symbol(c"numeric_unknown")) };
    assert!(NumericSummary::discover(unknown, r.canonical().smallint_vt).is_none());
    assert!(
        NumericSummary::discover(
            crate::dispatch::vtable_lookup_method,
            r.canonical().smallint_vt
        )
        .is_none()
    );
    let other = crate::dispatch::build_bootstrap();
    assert!(
        NumericSummary::discover(add, other.runtime.canonical().smallint_vt).is_none(),
        "declaration belongs to its initialized floor"
    );
    unsafe { dlclose(fixture.handle.take().unwrap().as_ptr()) };
    let input = r.make_integer(9);
    assert_eq!(
        r.enter(|| r.integer_value(unsafe { add(input, &input, 1) })),
        18
    );
    drop(summary);
}

#[test]
fn guarded_numeric_calls_preserve_allocations_errors_redefinition_and_argument_identity() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.install_execution_objects();
    let fixture = Fixture::load(r);
    let input = r.make_integer(11);
    let _root = ScopedOopRootGuard::new(r, input);
    let call = |method: Method, left, right| {
        r.enter(|| {
            r.try_numeric_call(method, left, &[right])
                .unwrap_or_else(|| unsafe { method(left, &right, 1) })
        })
    };
    for code in 1..=5 {
        let method = fixture.method(code);
        for _ in 0..HOT_CALLS + 1 {
            call(method, input, input);
        }
        let before = fixture.calls();
        let bytes = r.gc_live_bytes();
        let count = r.gc_live_count();
        let expected = Operator::from_declaration(code)
            .unwrap()
            .evaluate(11, 11)
            .unwrap();
        let result = call(method, input, input);
        assert_eq!(r.integer_value(result), expected);
        assert_ne!(result, input, "the result is a fresh identity");
        assert_eq!(
            fixture.calls(),
            before,
            "hot admitted call bypasses native wrapper"
        );
        assert_eq!(r.gc_live_count(), count + 1);
        assert_eq!(
            r.gc_live_bytes() - bytes,
            crate::heap::Heap::allocation_total_size(8).unwrap()
        );
    }
    for (code, a, b) in [
        (1, i64::MAX, 1),
        (2, i64::MIN, 1),
        (3, i64::MIN, -1),
        (4, i64::MIN, -1),
        (4, 1, 0),
        (5, 1, 0),
    ] {
        let before = fixture.calls();
        assert_eq!(
            call(fixture.method(code), r.make_integer(a), r.make_integer(b)),
            r.canonical().nil
        );
        assert_eq!(
            fixture.calls(),
            before + 1,
            "fault enters original native implementation"
        );
    }
    r.export(b"ErrorVt", r.canonical().smallint_vt).unwrap();
    let count = r.gc_live_count();
    assert_eq!(call(fixture.method(1), input, input), input);
    assert_eq!(r.gc_live_count(), count);
    r.export(b"ErrorVt", core::ptr::null_mut()).unwrap();
    assert_eq!(r.integer_value(call(fixture.method(1), input, input)), 22);
    let unknown: Method = unsafe { core::mem::transmute(fixture.symbol(c"numeric_unknown")) };
    let before = fixture.calls();
    assert_eq!(r.integer_value(call(unknown, input, input)), 0);
    assert_eq!(fixture.calls(), before + 1);
}

#[test]
fn compiled_raw_numeric_chains_match_i128_oracle_and_stop_at_first_fault() {
    let mut seed = 0x74_74_1234_5678u64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let extrema = [
        i64::MIN,
        i64::MIN + 1,
        -3037000500,
        -11,
        -1,
        0,
        1,
        3,
        11,
        3037000500,
        i64::MAX - 1,
        i64::MAX,
    ];
    for case in 0..96 {
        let operations: Vec<_> = (0..if case < 5 { 1 } else { 16 })
            .map(|index| {
                let mut value = || {
                    if index > 0 && random() % 2 == 0 {
                        Value::Result((random() % index as u64) as u32)
                    } else {
                        Value::Input((random() % 2) as u32)
                    }
                };
                let left = if case < 5 { Value::Input(0) } else { value() };
                let right = if case < 5 { Value::Input(1) } else { value() };
                Operation {
                    left,
                    right,
                    operator: Operator::from_declaration(if case < 5 {
                        case + 1
                    } else {
                        (random() % 5) as u32 + 1
                    })
                    .unwrap(),
                }
            })
            .collect();
        let plan = Plan::new(2, operations).unwrap();
        let source = CString::new(plan.emit()).unwrap();
        let unit =
            unsafe { Unit::compile(&source, &[]) }.expect("checked integer C compiles with TinyCC");
        let kernel: unsafe extern "C" fn(*const i64, *mut i64) -> usize =
            unsafe { core::mem::transmute(unit.symbol(c"numeric_chain").unwrap().as_ptr()) };
        for trial in 0..512 {
            let inputs = if trial < extrema.len() * extrema.len() {
                [
                    extrema[trial / extrema.len()],
                    extrema[trial % extrema.len()],
                ]
            } else {
                [random() as i64, random() as i64]
            };
            let mut expected = vec![0x123456789i64; plan.operations().len() + 2];
            let mut failure = 0;
            for (index, operation) in plan.operations().iter().enumerate() {
                let value = |v| match v {
                    Value::Input(i) => inputs[i as usize],
                    Value::Result(i) => expected[1 + i as usize],
                };
                if let Some(answer) = operation
                    .operator
                    .evaluate(value(operation.left), value(operation.right))
                {
                    expected[index + 1] = answer;
                } else {
                    failure = index + 1;
                    break;
                }
            }
            let mut actual = vec![0x123456789i64; expected.len()];
            assert_eq!(
                unsafe { kernel(inputs.as_ptr(), actual.as_mut_ptr().add(1)) },
                failure,
                "case {case} inputs {inputs:?}"
            );
            assert_eq!(
                actual, expected,
                "all raw spills and canaries: case {case} inputs {inputs:?}"
            );
        }
    }
}
