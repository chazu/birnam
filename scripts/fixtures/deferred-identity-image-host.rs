//! Require the actual host's mapped/rebuilt outcome before executing image main.
use birnam_host::{ImageLoadOutcome, Overrides, StartupPolicy};
use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 5, "PREFIX MODE ARTIFACT NATIVE_PROBE");
    let prefix = Path::new(&args[1]);
    let artifact = Path::new(&args[3]);
    let lib_dir = std::env::var_os("BIRNAM_LIB_DIR").map(std::path::PathBuf::from);
    let resources = birnam_host::resolve_from_exe_dir(&prefix.join("bin"), Overrides { lib_dir: lib_dir.as_deref(), ..Overrides::default() }).unwrap();
    let mut runtime = unsafe { birnam_host::start(resources, StartupPolicy::from_flags(false, false)) }.unwrap();
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    let outcome = match args[2].as_str() {
        "mapped" | "rebuilt" => unsafe {
            birnam_host::load_image(&mut runtime, artifact, args[2] == "rebuilt")
        }.unwrap(),
        "baked" => {
            let bytes = birnam_host::read_baked_image(artifact).unwrap().expect("baked image");
            unsafe { birnam_host::load_image_bytes(&mut runtime, &bytes, false) }.unwrap()
        }
        other => panic!("unknown mode {other}"),
    };
    assert_eq!(outcome, if args[2] == "rebuilt" { ImageLoadOutcome::SourceRecovery } else { ImageLoadOutcome::Mapped });
    // Initialize the fixture through the host before its idle storage check.
    use birnam_abi::id_protocol::{self as native, Operand};
    let stack = &runtime.stack;
    let inputs = [Operand::word((stack.api() as *const birnam_abi::IdApi).addr()),
        Operand::word(stack.imports().addr()), Operand::word(stack.root_registry().addr()),
        Operand::word(stack.gc_settings().addr()), Operand::word(stack.finalizers().addr())];
    assert_eq!(unsafe { stack.load_native(args[4].as_bytes(), &inputs) }.status, native::LOADED);
    let library = std::ffi::CString::new(args[4].as_bytes()).unwrap();
    let api = runtime.stack.api();
    let image = unsafe { (api.dlopen)(library.as_ptr(), 2 | 0x10) };
    assert!(!image.is_null());
    let check = unsafe { (api.dlsym)(image, c"deferred_identity_image_idle_v1".as_ptr()) };
    assert!(!check.is_null());
    let check: unsafe extern "C" fn(i32) = unsafe { std::mem::transmute(check) };
    unsafe { check(0) };
    assert!(unsafe { runtime.run_image_main() }.unwrap());
    // The evicted but still rooted main generation keeps observation guarded.
    unsafe { check(1) };
}
