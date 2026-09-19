#[path = "namespace.rs"]
mod namespace;
// Test mechanics only. Expected callback counts are supplied by each adapter.
use birnam_abi::IdApi;
use std::ffi::{CStr, CString, c_char, c_int, c_void};

unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

pub fn check(mut lookup: impl FnMut(&IdApi, &'static CStr) -> Option<usize>, cached_symbol: bool) {
    unsafe {
        let open = |variable: &str, flags| {
            let path = CString::new(std::env::var(variable).expect(variable)).unwrap();
            let handle = dlopen(path.as_ptr(), flags);
            assert!(!handle.is_null(), "{variable}");
            handle
        };
        let base = open("BIRNAM_LOOKUP_BASE", 6);
        namespace::prepare(base, std::path::Path::new(&std::env::var("BIRNAM_LOOKUP_BASE").unwrap()));
        let init = dlsym(base, c"id_init".as_ptr());
        assert!(!init.is_null());
        let init: unsafe extern "C" fn() -> *mut IdApi = std::mem::transmute(init);
        let api = init();
        assert!(!api.is_null());
        // A same-named global competitor must not supply the selected symbol.
        let competitor = open("BIRNAM_LOOKUP_COMPETITOR", 10);
        let selected = open("BIRNAM_LOOKUP_SELECTED", 6);
        let make = dlsym(selected, c"fixture_api".as_ptr());
        let count = dlsym(selected, c"fixture_count".as_ptr());
        assert!(!make.is_null() && !count.is_null());
        let make: unsafe extern "C" fn(*const IdApi, c_int) -> *const IdApi = std::mem::transmute(make);
        let count: unsafe extern "C" fn(u32) -> u32 = std::mem::transmute(count);
        let mode: c_int = std::env::var("BIRNAM_LOOKUP_MODE").unwrap().parse().unwrap();
        let copied = make(api, mode);
        let name = if mode == 1 { c"fixture_missing" } else { c"fixture_target" };
        for _ in 0..3 {
            let answer = lookup(&*copied, name);
            if mode == 0 {
                let call: unsafe extern "C" fn(usize) -> u32 = std::mem::transmute(answer.unwrap());
                assert_eq!(call(0), 7);
            } else {
                assert_eq!(answer, None);
            }
        }
        assert_eq!(count(0), 1, "one acquisition including cached failure");
        assert_eq!(count(1), if mode == 2 { 0 } else if cached_symbol { 1 } else { 3 });
        // Discard the harness's ownership; only the adapter's acquisition may
        // keep the selected image mapped. No pointer into it is used afterwards.
        assert_eq!(dlclose(selected), 0);
        let unloaded = std::path::PathBuf::from(std::env::var("BIRNAM_LOOKUP_UNLOADED").unwrap());
        assert_eq!(unloaded.exists(), mode == 2, "adapter retention, including missing-symbol lookup");
        assert_eq!(dlclose(competitor), 0);
        // The real floor supplies process-owned API functions and stays mapped.
    }
}
