//! Exercise successful protected observations at the real host's idle boundary.
use birnam_abi::{Oop, storage::*};
use birnam_host::{Overrides, StartupPolicy};
use std::{ffi::{CString, c_void}, path::Path, ptr};

struct Record {
    observe: ObserveV1,
    runtime: u64,
    epoch: u64,
    sample: Oop,
    allocation: u64,
    status: Status,
}
unsafe extern "C" fn visitor(context: *mut c_void, view: *const ViewV1) -> Status {
    let record = unsafe { &mut *context.cast::<Record>() };
    let view = unsafe { &*view };
    assert_eq!((view.version, view.byte_size), (VERSION, size_of::<ViewV1>()));
    assert!(view.runtime_identity != 0 && view.allocation_count > 0);
    if record.runtime == 0 { record.runtime = view.runtime_identity; }
    assert_eq!(view.runtime_identity, record.runtime);
    assert_eq!(view.observation_identity, record.epoch + 1);
    record.epoch = view.observation_identity;
    // No Id operations, dispatch or allocation while the view is borrowed.
    assert_eq!(unsafe { (record.observe)(ptr::null()) }, BUSY);
    if !record.sample.is_null() {
        let allocations = unsafe { std::slice::from_raw_parts(view.allocations, view.allocation_count) };
        let sample = allocations.iter().find(|item| item.object == record.sample).expect("sample remains live");
        assert!(sample.identity != 0);
        if record.allocation == 0 { record.allocation = sample.identity; }
        assert_eq!(sample.identity, record.allocation);
    }
    record.status
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "PREFIX MODE ARTIFACT");
    let prefix = Path::new(&args[1]);
    let artifact = Path::new(&args[3]);
    let resources = birnam_host::resolve_from_exe_dir(&prefix.join("bin"), Overrides::default()).unwrap();
    let mut runtime = unsafe { birnam_host::start(resources, StartupPolicy::from_flags(false, false)) }.unwrap();
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    let api = runtime.stack.api();
    let path = CString::new(prefix.join("lib/libbirnam_id.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let floor = unsafe { (api.dlopen)(path.as_ptr(), 6) };assert!(!floor.is_null());
    let symbol = unsafe { (api.dlsym)(floor, c"birnam_id_observe_storage_v1".as_ptr()) };assert!(!symbol.is_null());
    let observe: ObserveV1 = unsafe { std::mem::transmute(symbol) };
    let mut record = Record { observe, runtime: 0, epoch: 0, sample: ptr::null_mut(), allocation: 0, status: OK };
    let request = RequestV1 { version: VERSION, byte_size: size_of::<RequestV1>(), context: (&raw mut record).cast(), visitor: Some(visitor) };
    assert_eq!(unsafe { observe(&request) }, OK);
    assert_eq!(record.epoch, 1);
    match args[2].as_str() {
        "source" => { unsafe { birnam_host::eval_script(&runtime, artifact) }.unwrap(); }
        "compiled" => { unsafe { birnam_host::activate_artifact(&runtime, artifact, None) }.unwrap(); }
        "mapped" | "rebuilt" | "baked" => {
            if args[2] == "baked" {
                let bytes = birnam_host::read_baked_image(artifact).unwrap().expect("baked image");
                let outcome = unsafe { birnam_host::load_image_bytes(&mut runtime, &bytes, false) }.unwrap();
                assert_eq!(outcome, birnam_host::ImageLoadOutcome::Mapped);
            } else {
                let rebuild = args[2] == "rebuilt";
                let outcome = unsafe { birnam_host::load_image(&mut runtime, artifact, rebuild) }.unwrap();
                assert_eq!(outcome, if rebuild { birnam_host::ImageLoadOutcome::SourceRecovery } else { birnam_host::ImageLoadOutcome::Mapped });
            }
            assert!(unsafe { runtime.run_image_main() }.unwrap());
        }
        other => panic!("unknown mode {other}"),
    }
    let api = runtime.stack.api();
    let globals = unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"Globals".as_ptr()) };
    let at = unsafe { (api.intern)(c"at:".as_ptr()) };
    let sample_key = unsafe { (api.intern)(c"ObservationSample".as_ptr()) };
    let alias_key = unsafe { (api.intern)(c"ObservationAlias".as_ptr()) };
    record.sample = unsafe { (api.send)(globals, at, &sample_key, 1) };
    let alias = unsafe { (api.send)(globals, at, &alias_key, 1) };
    assert!(record.sample != api.nilObj && !record.sample.is_null());
    assert_eq!(record.sample, alias);
    assert_eq!(unsafe { observe(ptr::null()) }, INVALID);
    for status in [OK, -77, OK] {
        record.status = status;
        assert_eq!(unsafe { observe(&request) }, status);
    }
    unsafe { (api.gcCollect)() };
    assert_eq!(unsafe { observe(&request) }, OK);
    assert_eq!(record.epoch, 5);
    println!("observation-host-{}: epochs 1..5, nested BUSY, visitor failure cleanup, aliases and allocation identity after GC", args[2]);
}
