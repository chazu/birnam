//! Actual native adapters, with a handwritten synchronous Store and real floor.
#[path = "../../birnam-abi/tests/support/namespace.rs"]
mod namespace;

use super::*;
use std::ffi::CString;

unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

struct StoreFixture {
    facts: Vec<[Oop; 3]>,
    output: *const u8,
    before: Vec<u8>,
    written: *const usize,
    written_before: usize,
    callbacks: usize,
    malformed: bool,
}
impl StoreFixture {
    unsafe fn unchanged(&self) {
        assert_eq!(
            unsafe { std::slice::from_raw_parts(self.output, self.before.len()) },
            self.before
        );
        assert_eq!(unsafe { *self.written }, self.written_before);
    }
}
unsafe extern "C" fn store_send(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { &*(API.get().unwrap().0 as *const IdApi) };
    let fixture = unsafe { &mut **receiver.cast::<*mut StoreFixture>() };
    let callback = unsafe { *args.add(count - 1) };
    unsafe { fixture.unchanged() };
    fixture.callbacks += 1;
    for fact in &fixture.facts {
        unsafe {
            (api.send)(
                callback,
                (api.intern)(c"value:value:value:".as_ptr()),
                fact.as_ptr(),
                if fixture.malformed { 2 } else { 3 },
            )
        };
        unsafe { fixture.unchanged() };
    }
    api.nilObj
}
unsafe extern "C" fn vector_size(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    let _api = unsafe { &*(API.get().unwrap().0 as *const IdApi) };
    unsafe { (birnam_abi::integer::make)(*receiver.cast::<usize>() as i64) }
}
unsafe extern "C" fn vector_at(receiver: Oop, args: *const Oop, _: usize) -> Oop {
    let _api = unsafe { &*(API.get().unwrap().0 as *const IdApi) };
    unsafe {
        *receiver
            .cast::<Oop>()
            .add(1 + (birnam_abi::integer::value)(*args) as usize)
    }
}
unsafe fn vector(api: &IdApi, vt: Oop, values: &[Oop]) -> Oop {
    let object = unsafe { (api.alloc)(vt, (1 + values.len()) * size_of::<Oop>()) };
    unsafe {
        *object.cast::<usize>() = values.len();
        ptr::copy_nonoverlapping(values.as_ptr(), object.cast::<Oop>().add(1), values.len())
    };
    object
}

#[test]
#[ignore = "requires a fresh-process installed-floor fixture"]
fn exported_output_contract() {
    unsafe {
        let path = CString::new(std::env::var("BIRNAM_RELATIONAL_FLOOR").unwrap()).unwrap();
        let floor = dlopen(path.as_ptr(), 6);
        assert!(!floor.is_null());
        namespace::prepare(floor, std::path::Path::new(path.to_str().unwrap()));
        let address = dlsym(floor, c"id_init".as_ptr());
        assert!(!address.is_null());
        let init: unsafe extern "C" fn() -> *mut IdApi = std::mem::transmute(address);
        let api = &*init();
        // This fixture tests synchronous pointer use; GC lifetime has separate suites.
        (api.gcSetAuto)(0);
        // Mount the same explicit numeric pair as the production host. Handles
        // remain pinned for all method, context and codec addresses below.
        let lib_dir = std::path::Path::new(path.to_str().unwrap()).parent().unwrap();
        let load = |name: &str| {
            let path = CString::new(lib_dir.join(name).as_os_str().as_encoded_bytes()).unwrap();
            let handle = (api.dlopen)(path.as_ptr(), 6);
            assert!(!handle.is_null());
            handle
        };
        macro_rules! entry {
            ($handle:expr, $name:expr, $ty:ty) => {{
                let address = (api.dlsym)($handle, $name.as_ptr());
                assert!(!address.is_null());
                std::mem::transmute::<*mut c_void, $ty>(address)
            }};
        }
        let presence = entry!(floor, c"birnam_namespace_presence_provider_v1", birnam_abi::namespace_presence::ProviderV1)();
        let imports = entry!(floor, c"birnam_namespace_import_provider_v1", birnam_abi::namespace_import::ProviderV1)();
        let api_pointer = api as *const _ as *mut IdApi;
        let float_image = load("libbirnam_float.dylib");
        let floats = entry!(float_image, c"birnam_float_prepare_v3", birnam_abi::float::FloatPrepareV3)(api_pointer, presence, imports);
        assert_eq!(birnam_abi::float::attach(floats), 1);
        assert_eq!(entry!(floor, c"birnam_float_dependency_v1", birnam_abi::float::FloatDependencyV1)(floats), 1);
        let integer_image = load("libbirnam_integer.dylib");
        assert_eq!(entry!(integer_image, c"birnam_float_dependency_v1", birnam_abi::float::FloatDependencyV1)(floats), 1);
        entry!(integer_image, c"module_init", birnam_abi::ModuleInitFn)(api_pointer, ptr::null_mut(), presence, imports);
        let integers = entry!(integer_image, c"birnam_integer_provider_v1", unsafe extern "C" fn() -> *mut birnam_abi::integer::IntegerApiV1)();
        assert_eq!(birnam_abi::integer::attach(integers), 1);
        assert_eq!(entry!(floor, c"birnam_integer_dependency_v1", birnam_abi::integer::IntegerDependencyV1)(integers), 1);
        assert_eq!(entry!(float_image, c"birnam_integer_dependency_v1", birnam_abi::integer::IntegerDependencyV1)(integers), 1);
        entry!(float_image, c"module_init", birnam_abi::ModuleInitFn)(api_pointer, ptr::null_mut(), presence, imports);
        module_init(api as *const _ as *mut _, ptr::null_mut(), presence, imports);
        let integer = |n| (birnam_abi::integer::make)(n);
        let null = ptr::null_mut();
        let attribute = integer(7);
        let name = (api.intern)(c"fixture-query".as_ptr());
        let pattern = ProjectPattern {
            entity: null,
            attribute,
            value: null,
        };
        let absent = ProjectPattern {
            attribute: integer(999),
            ..pattern
        };
        let var = |n| RuleTermNode {
            object: null,
            variable: n,
            kind: 1,
            reserved: [0; 3],
        };
        let terms = [
            var(0),
            RuleTermNode {
                object: attribute,
                variable: 0,
                kind: 0,
                reserved: [0; 3],
            },
            var(2),
        ];
        let params = [2];
        let body = RuleClauseNode {
            kind: 0,
            reserved: [0; 3],
            name: null,
            terms: terms.as_ptr(),
            term_count: 3,
            reserved_tail: 0,
        };
        let definition = RuleDefinitionNode {
            name,
            params: params.as_ptr(),
            param_count: 1,
            reserved_params: 0,
            body: &body,
            body_count: 1,
            reserved_body: 0,
        };
        let tree_body = RuleBodyNode {
            kind: 0,
            reserved: [0; 3],
            name: null,
            terms: terms.as_ptr(),
            term_count: 3,
            reserved_terms: 0,
            children: ptr::null(),
            child_count: 0,
            reserved_children: 0,
        };
        let tree = RuleTreeDefinitionNode {
            name,
            params: params.as_ptr(),
            param_count: 1,
            reserved_params: 0,
            body: &tree_body,
            body_count: 1,
            reserved_body: 0,
        };
        let vector_vt = (api.proto)(api.object_vt);
        (api.setLayout)(vector_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.method)(vector_vt, (api.intern)(c"size".as_ptr()), vector_size);
        (api.method)(vector_vt, (api.intern)(c"at:".as_ptr()), vector_at);
        let logic = birnam_abi::namespace_import::import(imports, c"LogicVar".as_ptr());
        let named = (api.intern)(c"named:".as_ptr());
        let x = (api.send)(logic, named, [(api.intern)(c"x".as_ptr())].as_ptr(), 1);
        let y = (api.send)(logic, named, [(api.intern)(c"y".as_ptr())].as_ptr(), 1);
        let store_body = vector(
            api,
            vector_vt,
            &[vector(api, vector_vt, &[x, attribute, y])],
        );
        let pair = vector(api, vector_vt, &[vector(api, vector_vt, &[y]), store_body]);
        let rule_fact = [pair, (api.intern)(c"__rule-name__".as_ptr()), name];
        let store_vt = (api.proto)(api.object_vt);
        (api.setLayout)(store_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        for selector in [
            c"liveTriplesDo:",
            c"liveTriplesAsOf:do:",
            c"liveTriplesSince:do:",
            c"liveTriplesBetween:and:do:",
        ] {
            (api.method)(store_vt, (api.intern)(selector.as_ptr()), store_send);
        }
        let store = (api.alloc)(store_vt, size_of::<usize>());
        for family in 0..10 {
            for case in 0..12 {
                // Room for output and an overlapping RuleTermNode input. All records
                // are valid but carry sentinels independently of the production writer.
                let mut storage = [0xa5a5a5a5a5a5a5a5u64; 15];
                let output = storage.as_mut_ptr().cast::<ClassifiedValue>();
                let mut written = 91usize;
                let written_ptr = if case == 6 {
                    ptr::addr_of_mut!((*output).byte_len)
                } else {
                    &mut written
                };
                let query_term = var(2);
                let query = if case == 5 {
                    let p = output.cast::<RuleTermNode>();
                    ptr::write(p, var(2));
                    p
                } else {
                    &query_term as *const _
                };
                // case 3 has no matches; case 4 supplies malformed callback arity.
                let values = match case {
                    7 => [
                        (birnam_abi::string::make)(b"x".as_ptr(), 1),
                        (birnam_abi::string::make)(b"a\0b".as_ptr(), 3),
                    ],
                    8 => [(birnam_abi::float::make)(1.25), (birnam_abi::float::make)(2.5)],
                    9 => [
                        (api.intern)(c"first".as_ptr()),
                        (api.intern)(c"second".as_ptr()),
                    ],
                    _ => [integer(1), integer(2)],
                };
                let mut fixture = StoreFixture {
                    facts: if case == 3 {
                        vec![]
                    } else {
                        vec![
                            [integer(1), attribute, values[0]],
                            [integer(1), attribute, values[1]],
                            rule_fact,
                        ]
                    },
                    output: output.cast(),
                    before: std::slice::from_raw_parts(output.cast(), size_of_val(&storage))
                        .to_vec(),
                    written: written_ptr,
                    written_before: *written_ptr,
                    callbacks: 0,
                    malformed: case == 4 || case >= 10,
                };
                ptr::write(store.cast::<*mut StoreFixture>(), &mut fixture);
                let capacity = if case == 1 || case == 4 {
                    1
                } else if case == 2 || case == 3 {
                    0
                } else {
                    3
                };
                let destination = if case == 2 || case == 3 || case == 11 {
                    null.cast()
                } else {
                    output
                };
                let count_destination = if case == 10 {
                    ptr::null_mut()
                } else {
                    written_ptr
                };
                let before_live = (api.gcLiveCount)();
                let (status, allocations) = crate::evaluator::allocations(|| match family {
                    0 => birnam_relational_rust_project_live(
                        store,
                        2,
                        null,
                        attribute,
                        null,
                        destination,
                        capacity,
                        count_destination,
                    ),
                    1 => birnam_relational_rust_project_window_live(
                        store,
                        3,
                        0,
                        10,
                        2,
                        pattern,
                        destination,
                        capacity,
                        count_destination,
                    ),
                    2 => birnam_relational_rust_project_join_live(
                        store,
                        2,
                        pattern,
                        pattern,
                        destination,
                        capacity,
                        count_destination,
                    ),
                    3 => birnam_relational_rust_project_predicate_live(
                        store,
                        2,
                        pattern,
                        2,
                        5,
                        integer(-999),
                        destination,
                        capacity,
                        count_destination,
                    ),
                    4 => birnam_relational_rust_project_negated_live(
                        store,
                        2,
                        pattern,
                        absent,
                        destination,
                        capacity,
                        count_destination,
                    ),
                    5 => birnam_relational_rust_project_disjunction_live(
                        store,
                        2,
                        pattern,
                        pattern,
                        destination,
                        capacity,
                        count_destination,
                    ),
                    6 => birnam_relational_rust_project_recursive_live(
                        store,
                        2,
                        pattern,
                        integer(1),
                        destination,
                        capacity,
                        count_destination,
                    ),
                    7 => birnam_relational_rust_project_rule_live(
                        store,
                        &definition,
                        1,
                        name,
                        query,
                        1,
                        2,
                        destination,
                        capacity,
                        count_destination,
                    ),
                    8 => birnam_relational_rust_project_rule_tree_live(
                        store,
                        &tree,
                        1,
                        name,
                        query,
                        1,
                        2,
                        destination,
                        capacity,
                        count_destination,
                    ),
                    9 => birnam_relational_rust_project_rule_store_live(
                        store,
                        name,
                        query,
                        1,
                        2,
                        destination,
                        capacity,
                        count_destination,
                    ),
                    _ => unreachable!(),
                });
                let live_delta = (api.gcLiveCount)() - before_live;
                println!("native-output-allocation family={family} case={case} rust={allocations} floor={live_delta}");
                assert_eq!(
                    fixture.callbacks,
                    usize::from(case < 10),
                    "family={family} case={case}"
                );
                let expected = match case {
                    1 | 2 => -5,
                    4 | 10 | 11 => -3,
                    _ => 0,
                };
                assert_eq!(status, expected, "family={family} case={case}");
                if status != 0 {
                    fixture.unchanged();
                    continue;
                }
                assert_eq!(*written_ptr, if case == 3 { 0 } else { 2 });
                if case == 3 {
                    assert_eq!(storage, [0xa5a5a5a5a5a5a5a5; 15]);
                    continue;
                }
                for slot in 0..2 {
                    let value = &*output.add(slot);
                    assert_eq!(
                        value.tag,
                        match case {
                            7 => 3,
                            8 => 2,
                            9 => 0,
                            _ => 1,
                        }
                    );
                    assert_eq!(
                        value.integer,
                        if (7..=9).contains(&case) {
                            0
                        } else {
                            slot as i64 + 1
                        }
                    );
                    assert_eq!(value.reserved, [0; 7]);
                    assert_eq!(
                        value.float.to_bits(),
                        if case == 8 {
                            [1.25f64, 2.5][slot].to_bits()
                        } else {
                            0
                        }
                    );
                    assert_eq!(
                        value.identity,
                        if case == 9 { values[slot] as u64 } else { 0 }
                    );
                    assert_eq!(
                        value.byte_len,
                        if case == 7 {
                            [1, 3][slot]
                        } else if case == 6 && slot == 0 {
                            2
                        } else {
                            0
                        }
                    );
                }
                assert_eq!(&storage[10..], &[0xa5a5a5a5a5a5a5a5; 5]);
            }
        }
    }
}
