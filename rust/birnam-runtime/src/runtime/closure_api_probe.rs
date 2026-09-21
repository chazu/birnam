//! Explicit test-floor instrumentation during a hot source closure invocation.
use super::*;
use crate::abi::IdApi;
use core::cell::RefCell;
use core::ffi::{c_char, c_int, c_void};
use std::ffi::CStr;

#[derive(Clone, Copy)]
struct Saved {
    api: *mut IdApi,
    send: unsafe extern "C" fn(Oop, Oop, *const Oop, usize) -> Oop,
    intern: unsafe extern "C" fn(*const c_char) -> Oop,
    integer: unsafe extern "C" fn(*mut c_void, i64) -> Oop,
    integer_api: *mut birnam_abi::integer::IntegerApiV1,
    alloc: unsafe extern "C" fn(Oop, usize) -> Oop,
    imports: birnam_abi::namespace_import::ApiV1,
    import_api: *mut birnam_abi::namespace_import::ApiV1,
    vtable: unsafe extern "C" fn(Oop) -> Oop,
    size: unsafe extern "C" fn(Oop) -> usize,
    open: unsafe extern "C" fn(*const c_char, c_int) -> *mut c_void,
    symbol: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
}
thread_local! {
    static SAVED: Cell<Option<Saved>> = const { Cell::new(None) };
    static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
pub(super) unsafe fn attach(pointer: *mut IdApi) {
    let api = unsafe { &*pointer };
    let integer_api = with_active(|r| r.integer_live_pointer());
    let import_api = with_active(|r| r.namespace_import());
    SAVED.set(Some(Saved {
        api: pointer,
        send: api.send,
        intern: api.intern,
        integer: unsafe { (*integer_api).make },
        integer_api,
        alloc: api.alloc,
        imports: unsafe { *import_api },
        import_api,
        vtable: api.vtableOf,
        size: api.objectPayloadSize,
        open: api.dlopen,
        symbol: api.dlsym,
    }));
}
fn event(value: String) {
    EVENTS.with_borrow_mut(|events| events.push(value));
}
unsafe fn name(value: *const c_char) -> String {
    unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned()
}
unsafe extern "C" fn send(receiver: Oop, selector: Oop, args: *const Oop, count: usize) -> Oop {
    let saved = SAVED.get().unwrap();
    let selector_name = unsafe { ((*saved.api).symbolName)(selector) };
    event(format!("send:{}:{count}", unsafe { name(selector_name) }));
    unsafe { (saved.send)(receiver, selector, args, count) }
}
unsafe extern "C" fn intern(value: *const c_char) -> Oop {
    event(format!("intern:{}", unsafe { name(value) }));
    unsafe { (SAVED.get().unwrap().intern)(value) }
}
unsafe extern "C" fn integer(context: *mut c_void, value: i64) -> Oop {
    event(format!("integer:{value}"));
    unsafe { (SAVED.get().unwrap().integer)(context, value) }
}
unsafe extern "C" fn alloc(vtable: Oop, size: usize) -> Oop {
    event(format!("alloc:{size}"));
    unsafe { (SAVED.get().unwrap().alloc)(vtable, size) }
}
unsafe extern "C" fn import(context: *mut c_void, value: *const c_char) -> Oop {
    event(format!("import:{}", unsafe { name(value) }));
    unsafe { (SAVED.get().unwrap().imports.import.unwrap())(context, value) }
}
unsafe extern "C" fn vtable(value: Oop) -> Oop {
    event("vtable".into());
    unsafe { (SAVED.get().unwrap().vtable)(value) }
}
unsafe extern "C" fn size(value: Oop) -> usize {
    event("size".into());
    unsafe { (SAVED.get().unwrap().size)(value) }
}
unsafe extern "C" fn open(path: *const c_char, mode: c_int) -> *mut c_void {
    event(format!("open:{}:{mode}", unsafe { name(path) }));
    unsafe { (SAVED.get().unwrap().open)(path, mode) }
}
unsafe extern "C" fn symbol(handle: *mut c_void, value: *const c_char) -> *mut c_void {
    event(format!("symbol:{}", unsafe { name(value) }));
    unsafe { (SAVED.get().unwrap().symbol)(handle, value) }
}

struct Restore(Saved);
impl Drop for Restore {
    fn drop(&mut self) {
        // The fixture is owner-thread-only and changes the table only around
        // one synchronous invocation. Restore before logging or later sends.
        unsafe {
            let api = &mut *self.0.api;
            api.send = self.0.send;
            api.intern = self.0.intern;
            (*self.0.integer_api).make = self.0.integer;
            api.alloc = self.0.alloc;
            *self.0.import_api = self.0.imports;
            api.vtableOf = self.0.vtable;
            api.objectPayloadSize = self.0.size;
            api.dlopen = self.0.open;
            api.dlsym = self.0.symbol;
        }
    }
}

pub(super) unsafe extern "C" fn check(probe: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 3);
    let (block, input, operation) = unsafe { (*args, *args.add(1), *args.add(2)) };
    with_active(|r| {
        let operation = r.integer_value(operation);
        assert!((0..=7).contains(&operation));
        let selector = r.intern(b"value:value:").unwrap();
        let saved = SAVED.get().unwrap();
        EVENTS.with_borrow_mut(Vec::clear);
        let restore = Restore(saved);
        unsafe {
            let api = &mut *saved.api;
            // Always observe loader callbacks: even a refusing attempt must
            // not add a private symbol lookup to the original invocation.
            api.dlopen = open;
            api.dlsym = symbol;
            match operation {
                0 => (),
                1 => api.send = send,
                2 => api.intern = intern,
                3 => (*saved.integer_api).make = integer,
                4 => api.alloc = alloc,
                5 => (*saved.import_api).import = Some(import),
                6 => api.vtableOf = vtable,
                7 => api.objectPayloadSize = size,
                _ => unreachable!(),
            }
        }
        println!("closure-live-api: override begin");
        let answer = r.send_slice(block, selector, &[input, probe]);
        let _answer = ScopedOopRootGuard::new(r, answer);
        drop(restore);
        println!("closure-live-api: override end");
        let events = EVENTS.take();
        assert!(
            !events
                .iter()
                .any(|e| e.starts_with("open:") || e.starts_with("symbol:"))
        );
        if operation != 0 {
            assert!(!events.is_empty(), "selected API operation must execute");
        }
        assert_eq!(
            r.send_slice(answer, r.intern(b"value").unwrap(), &[]),
            input
        );
        println!("closure-live-api: operation={operation} events={events:?}");
        println!("closure-live-api: restored begin");
        let second = r.send_slice(block, selector, &[input, probe]);
        let _second = ScopedOopRootGuard::new(r, second);
        println!("closure-live-api: restored end");
        assert_ne!(
            answer, second,
            "distinct source constructions retain fresh identity"
        );
        assert_eq!(
            r.send_slice(second, r.intern(b"value").unwrap(), &[]),
            input
        );
        answer
    })
}
