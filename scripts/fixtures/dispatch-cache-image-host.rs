//! Require the actual host's mapped/rebuilt outcome before executing image main.
use birnam_host::{ImageLoadOutcome, Overrides, StartupPolicy};
use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "PREFIX MODE ARTIFACT");
    let prefix = Path::new(&args[1]);
    let artifact = Path::new(&args[3]);
    let resources = birnam_host::resolve_from_exe_dir(&prefix.join("bin"), Overrides::default()).unwrap();
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
    assert!(unsafe { runtime.run_image_main() }.unwrap());
}
