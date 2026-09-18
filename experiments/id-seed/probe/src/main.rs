//! Independent native consumer and semantic oracle. No linked seed/runtime.
use birnam_seed_abi::*;
use std::cell::Cell;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;
mod integer;

unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
    fn dlerror() -> *const c_char;
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_name(index: u32) -> *const c_char;
}

struct Dylib(*mut c_void);
impl Dylib {
    fn open(path: &str) -> Self {
        assert!(
            Path::new(path).is_absolute(),
            "use an explicit artifact path"
        );
        let path = CString::new(path).unwrap();
        // macOS RTLD_NOW | RTLD_LOCAL. This proof is explicitly macOS-only.
        let handle = unsafe { dlopen(path.as_ptr(), 2 | 4) };
        if handle.is_null() {
            let error = unsafe { CStr::from_ptr(dlerror()) };
            panic!("dlopen: {}", error.to_string_lossy());
        }
        Self(handle)
    }
    fn symbol(&self, name: &CStr) -> *mut c_void {
        let address = unsafe { dlsym(self.0, name.as_ptr()) };
        assert!(!address.is_null(), "missing symbol {name:?}");
        address
    }
}
impl Drop for Dylib {
    fn drop(&mut self) {
        assert_eq!(unsafe { dlclose(self.0) }, 0);
    }
}

// These wrappers are local to the probe: it owns the API and all operand/code
// lifetimes. They are not an additional exported implementation interface.
#[derive(Clone, Copy)]
struct Harness(*mut SeedApi);
impl Harness {
    fn api(&self) -> &SeedApi {
        unsafe { &*self.0 }
    }
    fn send(&self, object: Oop, selector: Oop, args: &[Value]) -> Result<Value, Status> {
        unsafe { call(self.0, object, selector, args) }
    }
    fn intern(&self, name: &[u8]) -> Oop {
        unsafe { intern(self.0, name) }.unwrap()
    }
    fn delegate(&self, parent: Oop) -> Oop {
        let result = self
            .send(parent, self.api().selectors.delegated, &[])
            .unwrap();
        assert_eq!(result.kind, OBJECT);
        result.a
    }
    fn allocate(&self, behavior: Oop, bytes: usize) -> Oop {
        let result = self
            .send(
                behavior,
                self.api().selectors.allocate,
                &[Value::word(bytes)],
            )
            .unwrap();
        assert_eq!(result.kind, OBJECT);
        result.a
    }
    fn install(&self, behavior: Oop, selector: Oop, method: Method) {
        assert_eq!(
            self.send(
                behavior,
                self.api().selectors.add_method,
                &[Value::object(selector), Value::method(method)]
            ),
            Ok(Value::method(method))
        );
    }
    fn behavior(&self, object: Oop) -> Oop {
        unsafe { (self.api().behavior_of)(self.0, object) }
    }
    fn reassociate(&self, object: Oop, behavior: Oop) {
        assert_eq!(
            unsafe { (self.api().reassociate)(self.0, object, behavior) },
            OK
        );
    }
    fn set_parent(&self, behavior: Oop, parent: Oop) {
        self.send(
            behavior,
            self.api().selectors.set_parent,
            &[Value::object(parent)],
        )
        .unwrap();
    }
    fn method(&self, behavior: Oop, selector: Oop) -> Method {
        let value = self
            .send(
                behavior,
                self.api().selectors.lookup,
                &[Value::object(selector)],
            )
            .unwrap();
        unsafe { value.as_method() }.unwrap()
    }
    fn expect_word(&self, object: Oop, selector: Oop, expected: usize) {
        assert_eq!(self.send(object, selector, &[]), Ok(Value::word(expected)));
    }
}

fn case(name: &str, test: impl FnOnce()) {
    println!("BEGIN\t{name}");
    test();
    println!("PASS\t{name}");
}

unsafe extern "C" fn eleven(
    _: *const SeedApi,
    _: *mut c_void,
    _: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if count != 0 {
        return INVALID;
    }
    unsafe { out.write(Value::word(11)) };
    OK
}
unsafe extern "C" fn twenty_two(
    _: *const SeedApi,
    _: *mut c_void,
    _: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if count != 0 {
        return INVALID;
    }
    unsafe { out.write(Value::word(22)) };
    OK
}

// This representation belongs to the probe. The seed knows neither its fields
// nor the rule for resolving methods, and it is NOT a default behavior record.
#[repr(C)]
struct ResolverPayload {
    selector: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn alternating_lookup(
    _: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if count != 1 {
        return INVALID;
    }
    let payload = unsafe { &*(receiver as *const ResolverPayload) };
    let selector = unsafe { *args };
    if selector != Value::object(payload.selector) {
        unsafe { out.write(Value::none()) };
        return OK;
    }
    let n = payload.calls.get();
    payload.calls.set(n + 1);
    let entry = if n % 2 == 0 {
        eleven as Entry
    } else {
        twenty_two as Entry
    };
    unsafe { out.write(Value::method(Method::plain(entry))) };
    OK
}

struct Reentrant {
    behavior: Oop,
    selector: Oop,
    entered: Cell<usize>,
    nested: Cell<usize>,
    destroy: SeedDestroy,
}
unsafe extern "C" fn replace_self(
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
    let state = unsafe { &*data.cast::<Reentrant>() };
    state.entered.set(state.entered.get() + 1);
    // An active invocation must prevent destruction, even across foreign reentry.
    if unsafe { (state.destroy)(api.cast_mut()) } != INVALID {
        return INVALID;
    }
    let installed = unsafe {
        call(
            api,
            state.behavior,
            (*api).selectors.add_method,
            &[
                Value::object(state.selector),
                Value::method(Method::plain(twenty_two)),
            ],
        )
    };
    if let Err(e) = installed {
        return e;
    }
    match unsafe { call(api, receiver, state.selector, &[]) } {
        Ok(value) if value == Value::word(22) => state.nested.set(value.a),
        Ok(_) => return INVALID,
        Err(e) => return e,
    }
    unsafe { out.write(Value::word(11)) };
    OK
}

struct Wrapper {
    original: Method,
    calls: Cell<usize>,
}
unsafe extern "C" fn wrapped(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let state = unsafe { &*data.cast::<Wrapper>() };
    state.calls.set(state.calls.get() + 1);
    unsafe { (state.original.entry)(api, state.original.data, receiver, args, count, out) }
}

fn main() {
    let paths: Vec<_> = std::env::args().collect();
    assert_eq!(
        paths.len(),
        4,
        "probe SEED_DYLIB OBJECT_MODEL_DYLIB_OR_DASH INTEGER_DYLIB"
    );
    let seed_dylib = Dylib::open(&paths[1]);
    let create: SeedCreate =
        unsafe { std::mem::transmute(seed_dylib.symbol(c"birnam_seed_create_v1")) };
    let destroy: SeedDestroy =
        unsafe { std::mem::transmute(seed_dylib.symbol(c"birnam_seed_destroy_v1")) };
    let h = Harness(unsafe { create() });
    assert!(!h.0.is_null());
    assert_eq!(h.api().version, VERSION);
    assert_eq!(h.api().size as usize, size_of::<SeedApi>());
    let roots = h.api().roots;
    let s = h.api().selectors;

    case("bootstrap-and-payload", || {
        assert_eq!(h.behavior(roots.vtable), roots.vtable);
        assert_eq!(h.behavior(roots.object), roots.vtable);
        assert_eq!(h.behavior(roots.symbol), roots.vtable);
        assert_eq!(h.behavior(roots.symbol_prototype), roots.symbol);
        assert_eq!(
            h.send(roots.vtable, s.parent, &[]),
            Ok(Value::object(roots.object))
        );
        let family = h.delegate(roots.object);
        let first = h.allocate(family, 2 * size_of::<usize>());
        let second = h.allocate(family, 0);
        assert_ne!(first, second);
        assert_eq!(h.behavior(first), family);
        // Access only our own explicitly requested payload, never a seed header.
        unsafe {
            let words = first as *mut usize;
            assert_eq!(words.read(), 0);
            words.write(0x1234);
            words.add(1).write(second);
            assert_eq!(words.read(), 0x1234);
            assert_eq!(words.add(1).read(), second);
        }
    });
    case("symbol-identity", || {
        for name in [b"".as_slice(), b"lookup", b"\xff\x00\xfe"] {
            assert_eq!(h.intern(name), h.intern(name));
        }
        assert_eq!(h.intern(b"lookup"), s.lookup);
        assert_ne!(h.intern(b"a"), h.intern(b"b"));
    });
    let selector = h.intern(b"answer");
    case("binding-replace-remove-parent", || {
        let parent = h.delegate(roots.object);
        let child = h.delegate(parent);
        let object = h.allocate(child, 0);
        h.install(parent, selector, Method::plain(eleven));
        h.expect_word(object, selector, 11);
        h.install(child, selector, Method::plain(twenty_two));
        h.expect_word(object, selector, 22);
        assert_eq!(
            h.send(child, s.remove_method, &[Value::object(selector)]),
            Ok(Value::method(Method::plain(twenty_two)))
        );
        h.expect_word(object, selector, 11);
        assert_eq!(
            h.send(child, s.remove_method, &[Value::object(selector)]),
            Ok(Value::none())
        );
        h.send(parent, s.remove_method, &[Value::object(selector)])
            .unwrap();
        assert_eq!(h.send(object, selector, &[]), Err(UNBOUND));
        let replacement = h.delegate(roots.object);
        h.install(replacement, selector, Method::plain(twenty_two));
        h.set_parent(child, replacement);
        h.expect_word(object, selector, 22);
    });

    // Integer is an independent sibling: exercise it BEFORE object-model loads.
    let integers = integer::IntegerFixture::load(h, &paths[3]);
    integers.exercise(h, create, destroy, &paths[3]);
    if paths[2] == "-" {
        dump_images();
        case("integer-only-shutdown", || {
            assert_eq!(unsafe { destroy(h.0) }, OK);
            unsafe { integers.destroy() };
        });
        drop(seed_dylib);
        return;
    }

    // Load the object-model layer only AFTER Integer has finished its cases.
    let library_dylib = Dylib::open(&paths[2]);
    let library_create: LibraryCreate =
        unsafe { std::mem::transmute(library_dylib.symbol(c"birnam_object_model_create_v1")) };
    let library_destroy: LibraryDestroy =
        unsafe { std::mem::transmute(library_dylib.symbol(c"birnam_object_model_destroy_v1")) };
    let library = unsafe { library_create(h.0) };
    assert!(!library.is_null());
    assert_eq!(unsafe { (*library).version }, VERSION);
    assert_eq!(unsafe { (*library).size } as usize, size_of::<LibraryApi>());
    let private_selector = unsafe { (*library).ensure_private };
    case("private-behavior-in-upper-dylib", || {
        let shared = h.delegate(roots.object);
        let first = h.allocate(shared, 0);
        let second = h.allocate(shared, 0);
        h.install(shared, selector, Method::plain(eleven));
        let private = h.send(first, private_selector, &[]).unwrap().a;
        assert_ne!(private, shared);
        assert_eq!(h.behavior(first), private);
        assert_eq!(h.behavior(second), shared);
        assert_eq!(h.send(private, s.parent, &[]), Ok(Value::object(shared)));
        h.install(private, selector, Method::plain(twenty_two));
        h.expect_word(first, selector, 22);
        h.expect_word(second, selector, 11);
        assert_eq!(
            h.send(first, private_selector, &[]),
            Ok(Value::object(private))
        );
        assert_eq!(unsafe { ((*library).created_count)(library) }, 1);
        h.reassociate(first, shared);
        h.expect_word(first, selector, 11);
        let next_private = h.send(first, private_selector, &[]).unwrap().a;
        assert_ne!(next_private, private);
        assert_eq!(h.behavior(first), next_private);
        assert_eq!(unsafe { ((*library).created_count)(library) }, 2);
        assert_eq!(
            h.send(first, private_selector, &[Value::none()]),
            Err(INVALID)
        );
        assert_eq!(unsafe { ((*library).created_count)(library) }, 2);
        h.expect_word(first, selector, 11);
        h.expect_word(second, selector, 11);
    });

    integers.exercise_private_behavior(h, private_selector);

    case("custom-parent-lookup-with-effects", || {
        let resolver_behavior = h.delegate(roots.object);
        h.install(
            resolver_behavior,
            s.lookup,
            Method::plain(alternating_lookup),
        );
        let resolver = h.allocate(resolver_behavior, size_of::<ResolverPayload>());
        unsafe {
            (resolver as *mut ResolverPayload).write(ResolverPayload {
                selector,
                calls: Cell::new(0),
            })
        };
        let child = h.delegate(roots.object);
        let object = h.allocate(child, 0);
        h.set_parent(child, resolver);
        assert_eq!(h.send(child, s.parent, &[]), Ok(Value::object(resolver)));
        h.expect_word(object, selector, 11);
        h.expect_word(object, selector, 22);
        h.expect_word(object, selector, 11);
        assert_eq!(
            unsafe { &*(resolver as *const ResolverPayload) }
                .calls
                .get(),
            3
        );
        // The same arbitrary resolver can be attached directly as a behavior.
        h.reassociate(object, resolver);
        h.expect_word(object, selector, 22);
        assert_eq!(
            unsafe { &*(resolver as *const ResolverPayload) }
                .calls
                .get(),
            4
        );
    });

    // State and native code remain alive until the seed has been destroyed.
    let behavior = h.delegate(roots.object);
    let object = h.allocate(behavior, 0);
    let reentrant = Box::new(Reentrant {
        behavior,
        selector,
        entered: Cell::new(0),
        nested: Cell::new(0),
        destroy,
    });
    case("replace-during-reentrant-call", || {
        h.install(
            behavior,
            selector,
            Method {
                entry: replace_self,
                data: (&*reentrant as *const Reentrant).cast_mut().cast(),
            },
        );
        h.expect_word(object, selector, 11);
        h.expect_word(object, selector, 22);
        assert_eq!(reentrant.entered.get(), 1);
        assert_eq!(reentrant.nested.get(), 22);
    });

    let lookup_wrapper = Box::new(Wrapper {
        original: h.method(roots.vtable, s.lookup),
        calls: Cell::new(0),
    });
    case("replace-metacircular-lookup", || {
        h.install(
            roots.vtable,
            s.lookup,
            Method {
                entry: wrapped,
                data: (&*lookup_wrapper as *const Wrapper).cast_mut().cast(),
            },
        );
        let before_root_send = lookup_wrapper.calls.get();
        let _ = h.method(roots.vtable, s.lookup);
        assert_eq!(
            lookup_wrapper.calls.get(),
            before_root_send + 1,
            "a direct send to the recursion root bypassed its replacement"
        );
        h.expect_word(object, selector, 22);
        assert!(
            lookup_wrapper.calls.get() > 0,
            "root bypassed current lookup binding"
        );
        h.install(roots.vtable, s.lookup, lookup_wrapper.original);
        let before = lookup_wrapper.calls.get();
        h.expect_word(object, selector, 22);
        assert_eq!(lookup_wrapper.calls.get(), before);
    });

    let allocation_wrapper = Box::new(Wrapper {
        original: h.method(roots.vtable, s.allocate),
        calls: Cell::new(0),
    });
    case("replace-allocation-method", || {
        let custom_meta = h.delegate(roots.vtable);
        let family = h.delegate(roots.object);
        h.install(
            custom_meta,
            s.allocate,
            Method {
                entry: wrapped,
                data: (&*allocation_wrapper as *const Wrapper).cast_mut().cast(),
            },
        );
        h.reassociate(family, custom_meta);
        let instance = h.allocate(family, 0);
        assert_eq!(h.behavior(instance), family);
        assert_eq!(allocation_wrapper.calls.get(), 1);
    });

    case("rejected-inputs-and-cycle-recovery", || {
        assert_eq!(h.send(1, selector, &[]), Err(INVALID));
        assert_eq!(h.send(object, object, &[]), Err(INVALID));
        assert_eq!(
            h.send(behavior, s.allocate, &[Value::word(usize::MAX)]),
            Err(INVALID)
        );
        assert_eq!(unsafe { (h.api().reassociate)(h.0, object, 1) }, INVALID);
        assert_eq!(h.behavior(object), behavior);
        let mut out = Value::none();
        assert_eq!(
            unsafe { (h.api().send)(h.0, object, selector, std::ptr::null(), 1, &mut out) },
            INVALID
        );
        let cycle = h.delegate(roots.object);
        let cycle_object = h.allocate(cycle, 0);
        h.set_parent(cycle, cycle);
        assert_eq!(h.send(cycle_object, selector, &[]), Err(RECURSION));
        h.set_parent(cycle, roots.object);
        assert_eq!(h.send(cycle_object, selector, &[]), Err(UNBOUND));
        h.expect_word(object, selector, 22);
    });

    case("independent-seed-universes", || {
        let other = Harness(unsafe { create() });
        assert!(!other.0.is_null());
        let other_selector = other.intern(b"answer");
        assert_ne!(other_selector, selector);
        assert_eq!(other.send(object, other_selector, &[]), Err(INVALID));
        assert_eq!(
            h.send(other.api().roots.object, selector, &[]),
            Err(INVALID)
        );
        let family = other.delegate(other.api().roots.object);
        let instance = other.allocate(family, 0);
        other.install(family, other_selector, Method::plain(eleven));
        other.expect_word(instance, other_selector, 11);
        assert_eq!(unsafe { destroy(other.0) }, OK);
        h.expect_word(object, selector, 22);
    });

    // Observe every mapped image while all experimental implementations live.
    dump_images();
    case("ordered-shutdown", || {
        assert_eq!(unsafe { destroy(h.0) }, OK);
        unsafe { library_destroy(library) };
        unsafe { integers.destroy() };
    });
    // All function/data pointers have retired before code is unmapped.
    drop(library_dylib);
    drop(seed_dylib);
}

fn dump_images() {
    for index in 0..unsafe { _dyld_image_count() } {
        let path = unsafe { CStr::from_ptr(_dyld_get_image_name(index)) };
        println!("IMAGE\t{}", path.to_str().unwrap());
    }
}
