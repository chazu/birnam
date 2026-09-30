//! An owner for one private in-memory native unit.

use super::*;
use core::ptr::NonNull;
use std::ffi::CStr;
use std::sync::Mutex;

// libtcc has compiler-global scratch state. No lock survives a native entry.
static COMPILER: Mutex<()> = Mutex::new(());

#[cfg(feature = "resource-metrics")]
mod metrics {
    use std::sync::atomic::{AtomicUsize, Ordering};
    pub(super) static UNITS: AtomicUsize = AtomicUsize::new(0);
    pub(super) static BYTES: AtomicUsize = AtomicUsize::new(0);
    pub(super) fn add(bytes: usize) {
        BYTES
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                old.checked_add(bytes)
            })
            .expect("native resource byte counter overflow");
        UNITS.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn remove(bytes: usize) {
        BYTES
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                old.checked_sub(bytes)
            })
            .expect("native resource byte counter underflow");
        UNITS
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                old.checked_sub(1)
            })
            .expect("native resource owner counter underflow");
    }
    pub(super) fn read() -> (usize, usize) {
        (UNITS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed))
    }
}

pub struct Unit {
    state: NonNull<TccState>,
}

unsafe extern "C" fn discard_diagnostic(_: *mut c_void, _: *const c_char) {}

impl Unit {
    /// Compile trusted generated C to a relocatable ELF object.
    pub fn compile_object(source: &CStr) -> Option<Vec<u8>> {
        let file = ObjectFile::new()?;
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        let state = NonNull::new(unsafe { tcc_new() })?;
        let raw = state.as_ptr();
        let ok = unsafe {
            tcc_set_error_func(raw, core::ptr::null_mut(), Some(discard_diagnostic));
            tcc_set_output_type(raw, 3) >= 0
                && tcc_compile_string(raw, source.as_ptr()) >= 0
                && tcc_output_file(raw, file.name.as_ptr()) >= 0
        };
        unsafe { tcc_delete(raw) };
        ok.then(|| std::fs::read(&file.path).ok()).flatten()
    }
    /// Relocate trusted object bytes without invoking the C compiler front end.
    ///
    /// # Safety
    /// The bytes must be trusted native code for this target. Any retrieved
    /// symbol must be invoked with its declared ABI while this owner is alive.
    pub unsafe fn load_object(bytes: &[u8]) -> Option<Self> {
        let file = ObjectFile::new()?;
        std::fs::write(&file.path, bytes).ok()?;
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        let state = NonNull::new(unsafe { tcc_new() })?;
        let raw = state.as_ptr();
        let ok = unsafe {
            tcc_set_error_func(raw, core::ptr::null_mut(), Some(discard_diagnostic));
            tcc_set_output_type(raw, 1) >= 0
                && tcc_add_file(raw, file.name.as_ptr()) >= 0
                && tcc_relocate(raw) >= 0
        };
        if !ok {
            unsafe { tcc_delete(raw) };
            return None;
        }
        #[cfg(feature = "resource-metrics")]
        metrics::add(unsafe { super::relocated_allocation_bytes(raw) });
        Some(Self { state })
    }

    /// Compile trusted generated C and retain all relocated executable memory.
    ///
    /// # Safety
    /// Every supplied symbol must have the type declared by the generated
    /// source and remain valid until this Unit and all its native calls end.
    /// Executing a returned symbol requires the same proof for its arguments.
    pub unsafe fn compile(source: &CStr, symbols: &[(&CStr, *const c_void)]) -> Option<Self> {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        let state = NonNull::new(unsafe { tcc_new() })?;
        let raw = state.as_ptr();
        // SAFETY: this fresh compiler is exclusively held and all borrowed
        // strings and symbol names outlive these synchronous calls.
        let compiled = unsafe {
            tcc_set_error_func(raw, core::ptr::null_mut(), Some(discard_diagnostic));
            tcc_set_output_type(raw, 1) >= 0
                && symbols.iter().all(|(name, address)| {
                    !address.is_null() && tcc_add_symbol(raw, name.as_ptr(), *address) >= 0
                })
                && tcc_compile_string(raw, source.as_ptr()) >= 0
                && tcc_relocate(raw) >= 0
        };
        if !compiled {
            unsafe { tcc_delete(raw) };
            return None;
        }
        #[cfg(feature = "resource-metrics")]
        metrics::add(unsafe { super::relocated_allocation_bytes(raw) });
        Some(Self { state })
    }

    /// Live private Units in this compiler instance, including selected owners
    /// retained after cache removal. Bytes include code, data and alignment;
    /// they exclude compiler metadata and unrelated module/compiler instances.
    #[cfg(feature = "resource-metrics")]
    pub fn resource_totals() -> (usize, usize) {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        metrics::read()
    }

    /// All live TinyCC heap requests in this compiler instance, including
    /// compiler metadata and scratch. Raw compiler users must also serialize.
    #[cfg(feature = "allocation-metrics")]
    pub fn compiler_heap_totals() -> super::HeapAllocationTotals {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        super::heap_allocation_totals()
    }

    pub fn relocated_allocation_bytes(&self) -> usize {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        // The C helper reads the live state using the compiler's own layout.
        unsafe { super::relocated_allocation_bytes(self.state.as_ptr()) }
    }

    /// The address is usable only while its Unit remains alive. Returning a
    /// raw address does not grant permission to invoke it or transfer ownership.
    pub fn symbol(&self, name: &CStr) -> Option<NonNull<c_void>> {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        // SAFETY: this owner retains the relocated compiler and its memory.
        NonNull::new(unsafe { tcc_get_symbol(self.state.as_ptr(), name.as_ptr()) })
    }
}

struct ObjectFile {
    directory: std::path::PathBuf,
    path: std::path::PathBuf,
    name: std::ffi::CString,
}
impl ObjectFile {
    fn new() -> Option<Self> {
        use std::os::unix::ffi::OsStrExt;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let directory = std::env::temp_dir().join(format!(
                "birnam-native-object-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&directory) {
                Ok(()) => {
                    let path = directory.join("unit.o");
                    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
                    return Some(Self {
                        directory,
                        path,
                        name,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return None,
            }
        }
        None
    }
}
impl Drop for ObjectFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

impl Drop for Unit {
    fn drop(&mut self) {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        // SAFETY: Unit owns this compiler. Callers must retain that owner for
        // every invocation of an address borrowed from symbol().
        #[cfg(feature = "resource-metrics")]
        metrics::remove(unsafe { super::relocated_allocation_bytes(self.state.as_ptr()) });
        unsafe { tcc_delete(self.state.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    static TEST_COMPILER: Mutex<()> = Mutex::new(());

    unsafe extern "C" fn twice(value: i64) -> i64 {
        value * 2
    }

    #[test]
    fn relocatable_object_roundtrip_preserves_code_and_data_ownership() {
        let _test = TEST_COMPILER.lock().unwrap();
        #[cfg(feature = "resource-metrics")]
        let before = Unit::resource_totals();
        let bytes =
            Unit::compile_object(c"static long counter=40; long next(void) { return ++counter; }")
                .unwrap();
        assert!(bytes.starts_with(b"\x7fELF"));
        let unit = Rc::new(unsafe { Unit::load_object(&bytes) }.unwrap());
        let entry: unsafe extern "C" fn() -> i64 =
            unsafe { core::mem::transmute(unit.symbol(c"next").unwrap().as_ptr()) };
        let selected = unit.clone();
        drop(unit);
        assert_eq!(unsafe { entry() }, 41);
        assert_eq!(unsafe { entry() }, 42);
        assert!(unsafe { Unit::load_object(b"not an object") }.is_none());
        assert!(Unit::compile_object(c"invalid C").is_none());
        drop(selected);
        #[cfg(feature = "resource-metrics")]
        assert_eq!(Unit::resource_totals(), before);
    }

    #[test]
    fn relocated_code_retains_its_owner_across_cache_eviction() {
        let _test = TEST_COMPILER.lock().unwrap();
        let owner = Rc::new(unsafe {
            Unit::compile(
                c"extern long twice(long); long answer(long n) { return twice(n) + 1; }",
                &[(c"twice", twice as *const c_void)],
            )
            .expect("compile and relocate")
        });
        let weak = Rc::downgrade(&owner);
        let entry: unsafe extern "C" fn(i64) -> i64 =
            unsafe { core::mem::transmute(owner.symbol(c"answer").unwrap().as_ptr()) };
        let selected_owner = owner.clone();
        drop(owner);
        assert_eq!(unsafe { entry(20) }, 41);
        assert!(weak.upgrade().is_some());
        drop(selected_owner);
        assert!(weak.upgrade().is_none());
        assert!(unsafe { Unit::compile(c"not valid C", &[]) }.is_none());
    }
    #[test]
    #[cfg(feature = "resource-metrics")]
    fn relocated_allocations_follow_actual_unit_owners_and_include_data() {
        let _test = TEST_COMPILER.lock().unwrap();
        let before = Unit::resource_totals();
        let small = Rc::new(
            unsafe { Unit::compile(c"long answer(long n) { return n + 1; }", &[]) }.unwrap(),
        );
        let size = small.relocated_allocation_bytes();
        assert!(size > 0);
        assert_eq!(Unit::resource_totals(), (before.0 + 1, before.1 + size));
        let entry: unsafe extern "C" fn(i64) -> i64 =
            unsafe { core::mem::transmute(small.symbol(c"answer").unwrap().as_ptr()) };
        let selected = small.clone();
        let weak = Rc::downgrade(&small);
        drop(small);
        assert_eq!(unsafe { entry(40) }, 41);
        assert_eq!(selected.relocated_allocation_bytes(), size);
        assert_eq!(Unit::resource_totals(), (before.0 + 1, before.1 + size));
        let big = unsafe { Unit::compile(c"static volatile unsigned char data[65536]; long answer(long n) { data[n & 65535] = 7; return data[n & 65535]; }", &[]) }.unwrap();
        let big_size = big.relocated_allocation_bytes();
        assert!(big_size > size);
        let big_entry: unsafe extern "C" fn(i64) -> i64 =
            unsafe { core::mem::transmute(big.symbol(c"answer").unwrap().as_ptr()) };
        assert_eq!(unsafe { big_entry(123) }, 7);
        assert_eq!(
            Unit::resource_totals(),
            (before.0 + 2, before.1 + size + big_size)
        );
        assert!(unsafe { Unit::compile(c"invalid C", &[]) }.is_none());
        assert_eq!(
            Unit::resource_totals(),
            (before.0 + 2, before.1 + size + big_size)
        );
        drop(selected);
        assert!(weak.upgrade().is_none());
        assert_eq!(Unit::resource_totals(), (before.0 + 1, before.1 + big_size));
        drop(big);
        assert_eq!(Unit::resource_totals(), before);
    }

    #[test]
    #[cfg(feature = "allocation-metrics")]
    fn compiler_heap_returns_to_baseline_after_failure_and_last_owner_release() {
        let _test = TEST_COMPILER.lock().unwrap();
        let before = Unit::compiler_heap_totals();
        for _ in 0..16 {
            let owner = Rc::new(unsafe {
                Unit::compile(c"static volatile unsigned char values[65536]; long answer(long n) { values[n & 65535] = 7; return values[n & 65535]; }", &[])
            }.unwrap());
            let live = Unit::compiler_heap_totals();
            assert!(live.blocks > before.blocks);
            assert!(live.requested_bytes > before.requested_bytes);
            assert!(unsafe { Unit::compile(c"not valid C", &[]) }.is_none());
            assert_eq!(Unit::compiler_heap_totals(), live);
            assert!(
                unsafe {
                    Unit::compile(
                        c"extern long missing(void); long answer(void) { return missing(); }",
                        &[],
                    )
                }
                .is_none()
            );
            assert_eq!(Unit::compiler_heap_totals(), live);
            let entry: unsafe extern "C" fn(i64) -> i64 =
                unsafe { core::mem::transmute(owner.symbol(c"answer").unwrap().as_ptr()) };
            let selected = owner.clone();
            drop(owner);
            assert_eq!(unsafe { entry(12345) }, 7);
            assert_eq!(Unit::compiler_heap_totals(), live);
            drop(selected);
            assert_eq!(Unit::compiler_heap_totals(), before);
        }
    }
}
