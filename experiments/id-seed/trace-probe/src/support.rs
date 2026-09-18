use birnam_seed_abi::*;
use std::cell::Cell;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;

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

