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
    /// Relocate one trusted ELF object directly from memory, without staging
    /// files or invoking the C compiler front end. No input borrow is retained.
    ///
    /// # Safety
    /// The bytes must be trusted native code for this target. Any retrieved
    /// symbol must be invoked with its declared ABI while this owner is alive.
    pub unsafe fn load_object(bytes: &[u8]) -> Option<Self> {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        let state = NonNull::new(unsafe { tcc_new() })?;
        let raw = state.as_ptr();
        let ok = unsafe {
            tcc_set_error_func(raw, core::ptr::null_mut(), Some(discard_diagnostic));
            birnam_tcc_set_memory_output(raw) >= 0
                && tcc_add_object_memory(raw, bytes.as_ptr().cast(), bytes.len()) >= 0
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
            birnam_tcc_set_memory_output(raw) >= 0
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
    fn object_loading_does_not_require_a_temporary_directory() {
        const INPUT: &str = "BIRNAM_TEST_MEMORY_OBJECT";
        if let Some(path) = std::env::var_os(INPUT) {
            let bytes = std::fs::read(path).unwrap();
            let owner = unsafe { Unit::load_object(&bytes) }.expect("load compiled bytes without temporary files");
            let next: unsafe extern "C" fn() -> i64 = unsafe {
                core::mem::transmute(owner.symbol(c"next").unwrap().as_ptr())
            };
            assert_eq!(unsafe { next() }, 41);
            assert_eq!(unsafe { next() }, 42);
            return;
        }
        let _test = TEST_COMPILER.lock().unwrap();
        let bytes = Unit::compile_object(c"static long counter=40; long next(void) { return ++counter; }").unwrap();
        let directory = std::env::temp_dir().join(format!("birnam-memory-object-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("input.o");
        std::fs::write(&path, bytes).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "unit::tests::object_loading_does_not_require_a_temporary_directory", "--nocapture"])
            .env(INPUT, &path)
            .env("TMPDIR", directory.join("does-not-exist"))
            .status().unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
        assert!(result.success(), "object loading attempted temporary-file I/O");
    }

    #[test]
    fn memory_objects_copy_unaligned_inputs_and_keep_instance_data_separate() {
        let _test = TEST_COMPILER.lock().unwrap();
        let bytes = Unit::compile_object(c"static long counter=40; long next(void) { return ++counter; }").unwrap();
        let mut unaligned = vec![0];
        unaligned.extend_from_slice(&bytes);
        let first = unsafe { Unit::load_object(&unaligned[1..]) }.unwrap();
        let second = unsafe { Unit::load_object(&bytes) }.unwrap();
        unaligned.fill(0);
        drop(unaligned);
        drop(bytes);
        let next: unsafe extern "C" fn() -> i64 = unsafe { core::mem::transmute(first.symbol(c"next").unwrap().as_ptr()) };
        let other: unsafe extern "C" fn() -> i64 = unsafe { core::mem::transmute(second.symbol(c"next").unwrap().as_ptr()) };
        assert_eq!(unsafe { next() }, 41);
        assert_eq!(unsafe { other() }, 41);
        drop(first);
        assert_eq!(unsafe { other() }, 42);
    }

    #[test]
    fn file_and_memory_inputs_share_relocation_and_symbol_resolution() {
        let _test = TEST_COMPILER.lock().unwrap();
        let bytes = Unit::compile_object(c"extern unsigned long strlen(const char *); static long counter=40; long next(const char *s) { return ++counter + strlen(s); }").unwrap();
        let file = ObjectFile::new().unwrap();
        std::fs::write(&file.path, &bytes).unwrap();
        struct Raw(NonNull<TccState>);
        impl Drop for Raw {
            fn drop(&mut self) { unsafe { tcc_delete(self.0.as_ptr()) }; }
        }
        let raw = Raw(NonNull::new(unsafe { tcc_new() }).unwrap());
        unsafe {
            tcc_set_error_func(raw.0.as_ptr(), core::ptr::null_mut(), Some(discard_diagnostic));
            assert!(tcc_set_output_type(raw.0.as_ptr(), 1) >= 0);
            assert!(tcc_add_file(raw.0.as_ptr(), file.name.as_ptr()) >= 0);
            assert!(tcc_relocate(raw.0.as_ptr()) >= 0);
        }
        let memory = unsafe { Unit::load_object(&bytes) }.unwrap();
        drop(file);
        drop(bytes);
        let next: unsafe extern "C" fn(*const c_char) -> i64 = unsafe { core::mem::transmute(tcc_get_symbol(raw.0.as_ptr(), c"next".as_ptr())) };
        let other: unsafe extern "C" fn(*const c_char) -> i64 = unsafe { core::mem::transmute(memory.symbol(c"next").unwrap().as_ptr()) };
        assert_eq!(unsafe { next(c"abc".as_ptr()) }, 44);
        assert_eq!(unsafe { other(c"abc".as_ptr()) }, 44);
        assert_eq!(unsafe { next(c"abc".as_ptr()) }, 45);
        assert_eq!(unsafe { other(c"abc".as_ptr()) }, 45);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn resident_symbols_skip_default_runtime_setup_without_caching_addresses() {
        let _test = TEST_COMPILER.lock().unwrap();
        let source = c"extern unsigned long strlen(const char *); long answer(const char *s) { return strlen(s); }";
        let bytes = Unit::compile_object(source).unwrap();
        for unit in [unsafe { Unit::compile(source, &[]) }.unwrap(), unsafe { Unit::load_object(&bytes) }.unwrap()] {
            assert_eq!(unsafe { birnam_tcc_uses_host_runtime(unit.state.as_ptr()) }, 1);
            let answer: unsafe extern "C" fn(*const c_char) -> i64 = unsafe { core::mem::transmute(unit.symbol(c"answer").unwrap().as_ptr()) };
            assert_eq!(unsafe { answer(c"abcdef".as_ptr()) }, 6);
        }
        // Explicitly supplied native symbols remain per Unit, not global.
        unsafe extern "C" fn triple(n: i64) -> i64 { n * 3 }
        let source = c"extern long scale(long); long answer(long n) { return scale(n); }";
        let first = unsafe { Unit::compile(source, &[(c"scale", twice as *const c_void)]) }.unwrap();
        let second = unsafe { Unit::compile(source, &[(c"scale", triple as *const c_void)]) }.unwrap();
        for (unit, expected) in [(&first, 12), (&second, 18)] {
            assert_eq!(unsafe { birnam_tcc_uses_host_runtime(unit.state.as_ptr()) }, 1);
            let answer: unsafe extern "C" fn(i64) -> i64 = unsafe { core::mem::transmute(unit.symbol(c"answer").unwrap().as_ptr()) };
            assert_eq!(unsafe { answer(6) }, expected);
        }
    }

    // A conflicting host export must not displace the runtime archive's own
    // compiler helper. Keep this fixture in the test executable, not production.
    #[cfg(target_os = "macos")]
    #[unsafe(export_name = "__builtin_popcountll")]
    pub extern "C" fn shadow_popcount(_: u64) -> i32 { 97 }

    #[test]
    #[cfg(target_os = "macos")]
    fn unresolved_builtin_helpers_take_the_original_archive_path() {
        let _test = TEST_COMPILER.lock().unwrap();
        #[cfg(feature = "allocation-metrics")]
        let before = Unit::compiler_heap_totals();
        unsafe extern "C" { fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void; }
        // Darwin RTLD_DEFAULT. The deliberately wrong host implementation is
        // available, but ordinary archive-first helper resolution must win.
        let host = unsafe { dlsym((-2isize) as *mut c_void, c"__builtin_popcountll".as_ptr()) };
        assert_eq!(host, shadow_popcount as *mut c_void);
        // TinyCC supplies the correct builtin through libtcc1's builtin.o.
        let source = c"extern int __builtin_popcountll(unsigned long long); long answer(unsigned long long n) { return __builtin_popcountll(n); }";
        let bytes = Unit::compile_object(source).unwrap();
        assert!(bytes.windows(b"___builtin_popcountll".len()).any(|part| part == b"___builtin_popcountll"));
        for unit in [unsafe { Unit::compile(source, &[]) }.unwrap(), unsafe { Unit::load_object(&bytes) }.unwrap()] {
            let answer: unsafe extern "C" fn(i64) -> i64 = unsafe { core::mem::transmute(unit.symbol(c"answer").unwrap().as_ptr()) };
            assert_eq!(unsafe { answer(0x10203040) }, 5);
            assert_eq!(unsafe { answer(-1) }, 64);
            assert_eq!(unsafe { birnam_tcc_uses_host_runtime(unit.state.as_ptr()) }, 0);
        }
        #[cfg(feature = "allocation-metrics")]
        assert_eq!(Unit::compiler_heap_totals(), before);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn explicit_libraries_and_unresolved_imports_keep_their_failures() {
        let _test = TEST_COMPILER.lock().unwrap();
        #[cfg(feature = "allocation-metrics")]
        let before = Unit::compiler_heap_totals();
        let unit = unsafe { Unit::compile(c"#pragma comment(lib, \"m\")\nextern double cos(double); double answer(double n) { return cos(n); }", &[]) }.unwrap();
        assert_eq!(unsafe { birnam_tcc_uses_host_runtime(unit.state.as_ptr()) }, 0);
        let answer: unsafe extern "C" fn(f64) -> f64 = unsafe { core::mem::transmute(unit.symbol(c"answer").unwrap().as_ptr()) };
        assert_eq!(unsafe { answer(0.0) }, 1.0);
        drop(unit);
        // A requested library is an obligation even when no symbol needs it.
        assert!(unsafe { Unit::compile(c"#pragma comment(lib, \"birnam_missing_runtime_library_fixture\")\nlong answer(void) { return 42; }", &[]) }.is_none());
        let source = c"extern long birnam_missing_native_import(void); long answer(void) { return birnam_missing_native_import(); }";
        let bytes = Unit::compile_object(source).unwrap();
        assert!(unsafe { Unit::compile(source, &[]) }.is_none());
        assert!(unsafe { Unit::load_object(&bytes) }.is_none());
        #[cfg(feature = "allocation-metrics")]
        assert_eq!(Unit::compiler_heap_totals(), before);
    }

    #[test]
    fn truncated_and_invalid_object_ranges_release_the_failed_state() {
        let _test = TEST_COMPILER.lock().unwrap();
        let bytes = Unit::compile_object(c"static long counter=40; long next(void) { return ++counter; }").unwrap();
        #[cfg(feature = "allocation-metrics")]
        let before = Unit::compiler_heap_totals();
        for length in 0..bytes.len() {
            assert!(unsafe { Unit::load_object(&bytes[..length]) }.is_none(), "accepted prefix {length}");
            #[cfg(feature = "allocation-metrics")]
            assert_eq!(Unit::compiler_heap_totals(), before, "leaked prefix {length}");
        }
        // ELF64 header and section-table fields in the compiler's own fixture.
        let table = u64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
        for (offset, replacement) in [
            (4, vec![1]),
            (40, u64::MAX.to_le_bytes().to_vec()),
            (62, u16::MAX.to_le_bytes().to_vec()),
            (table + 64, u32::MAX.to_le_bytes().to_vec()),
            (table + 64 + 32, u64::MAX.to_le_bytes().to_vec()),
            (table + 64 + 40, u32::MAX.to_le_bytes().to_vec()),
        ] {
            let mut invalid = bytes.clone();
            invalid[offset..offset + replacement.len()].copy_from_slice(&replacement);
            assert!(unsafe { Unit::load_object(&invalid) }.is_none(), "accepted field {offset}");
            #[cfg(feature = "allocation-metrics")]
            assert_eq!(Unit::compiler_heap_totals(), before, "leaked field {offset}");
        }
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
    #[cfg(all(target_os = "macos", feature = "allocation-metrics"))]
    fn relocated_storage_does_not_pin_malloc_heap_regions() {
        unsafe extern "C" {
            fn birnam_tcc_mapping_protection(address: *const c_void) -> core::ffi::c_int;
        }
        let _test = TEST_COMPILER.lock().unwrap();
        let source = c"volatile unsigned char data[1048576]; long answer(long n) { data[n & 1048575] = 7; return data[n & 1048575]; }";
        let object = Unit::compile_object(source).unwrap();
        let heap_before = Unit::compiler_heap_totals();
        let units_before = Unit::resource_totals();
        // Empty relocations still own a valid mapping and can be released.
        drop(unsafe { Unit::compile(c"", &[]) }.unwrap());
        assert_eq!(Unit::compiler_heap_totals(), heap_before);
        assert_eq!(Unit::resource_totals(), units_before);
        for mapped in [false, true] {
            let owner = Rc::new(
                unsafe {
                    if mapped {
                        Unit::load_object(&object)
                    } else {
                        Unit::compile(source, &[])
                    }
                }
                .unwrap(),
            );
            assert!(owner.relocated_allocation_bytes() >= 1048576);
            let heap = Unit::compiler_heap_totals();
            assert!(
                heap.requested_bytes - heap_before.requested_bytes < 1048576,
                "relocated BSS must not occupy the malloc heap: {heap:?}"
            );
            let entry: unsafe extern "C" fn(i64) -> i64 =
                unsafe { core::mem::transmute(owner.symbol(c"answer").unwrap().as_ptr()) };
            let code = entry as *const c_void;
            let data = owner.symbol(c"data").unwrap().as_ptr();
            // Mach protection bits: read=1, write=2, execute=4.
            assert_eq!(unsafe { birnam_tcc_mapping_protection(code) }, 5);
            assert_eq!(unsafe { birnam_tcc_mapping_protection(data) }, 3);
            let selected = owner.clone();
            drop(owner);
            assert_eq!(unsafe { entry(1048575) }, 7);
            drop(selected);
            // Ask the kernel without dereferencing released code. Counters alone
            // would not detect a leaked VM mapping in the release implementation.
            assert_eq!(unsafe { birnam_tcc_mapping_protection(code) }, 0);
            assert_eq!(unsafe { birnam_tcc_mapping_protection(data) }, 0);
            assert_eq!(Unit::compiler_heap_totals(), heap_before);
            assert_eq!(Unit::resource_totals(), units_before);
        }
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
