//! A normal image boot must not invoke source compilation or evaluation.
use birnam_abi::Oop;
use birnam_host::{ImageLoadOutcome, Overrides, StartupPolicy};
use std::path::Path;

unsafe extern "C" fn forbidden(_receiver: Oop, _args: *const Oop, _count: usize) -> Oop {
    eprintln!("image startup invoked source compilation/evaluation");
    std::process::exit(90);
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "PREFIX IMAGE MODE");
    let prefix = Path::new(&args[1]);
    let resources =
        birnam_host::resolve_from_exe_dir(&prefix.join("bin"), Overrides::default()).unwrap();
    let mut runtime =
        unsafe { birnam_host::start(resources, StartupPolicy::from_flags(false, false)) }.unwrap();
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    if args[3] == "mapped" || args[3] == "baked" {
        let api = runtime.stack.api();
        for name in [c"Codegen", c"RustCodegen"] {
            let codegen = unsafe {
                birnam_abi::namespace_import::import(runtime.stack.imports(), name.as_ptr())
            };
            assert!(!codegen.is_null() && codegen != api.nilObj);
            let vt = unsafe { (api.vtableOf)(codegen) };
            for selector in [
                c"rebuildImageCode",
                c"compileImageCode:",
                c"compileImageCode:manifest:",
                c"eval:",
                c"evalPersisting:",
                c"eval:from:",
                c"evalPersisting:from:",
                c"compile:",
                c"compile:output:",
                c"compile:output:from:",
                c"compile:output:root:",
                c"compilePortableImageCode:bindings:selection:",
                c"beginPortableImageRebuild:bindings:selection:",
            ] {
                unsafe { (api.method)(vt, (api.intern)(selector.as_ptr()), forbidden) };
            }
        }
    }
    let loaded = if args[3] == "baked" {
        let bytes = birnam_host::read_baked_image(Path::new(&args[2]))
            .unwrap()
            .unwrap();
        unsafe { birnam_host::load_image_bytes(&mut runtime, &bytes, false) }
    } else {
        unsafe { birnam_host::load_image(&mut runtime, Path::new(&args[2]), args[3] == "rebuild") }
    };
    if args[3] == "rejected" {
        assert!(matches!(loaded, Err(birnam_host::ImageError::LoadRejected)));
        return;
    }
    let outcome = loaded.unwrap();
    assert_eq!(
        outcome,
        if args[3] == "rebuild" || args[3] == "missing" {
            ImageLoadOutcome::SourceRecovery
        } else {
            ImageLoadOutcome::Mapped
        }
    );
    assert!(unsafe { runtime.run_image_main() }.unwrap());
}
