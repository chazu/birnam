//! Independent Runtime witness; the preserved source uses its original Cells.
use super::*;
use crate::dispatch::build_bootstrap;

// Only these adapters differ when running against the preserved source.
fn inject_refusal(runtime: &mut Runtime) {
    runtime.observation.scope.refuse_advancement_for_test();
}
fn unwind_flags(runtime: &Runtime) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.observation.scope.set_permit(true);
        let _permit = Reset(&runtime.observation.scope, Flag::Permit);
        runtime.observation.scope.set_active(true);
        let _active = Reset(&runtime.observation.scope, Flag::Active);
        std::panic::resume_unwind(Box::new(()));
    }));
    assert!(result.is_err());
}

struct Record {
    runtime: *const Runtime,
    events: Vec<(u64, u64)>,
    status: Status,
    replacements: usize,
    mode: String,
}
impl Record {
    fn new(runtime: &Runtime) -> Self {
        Self {
            runtime,
            events: Vec::new(),
            status: OK,
            replacements: 0,
            mode: String::new(),
        }
    }
    fn request(&mut self) -> RequestV1 {
        RequestV1 {
            version: VERSION,
            byte_size: size_of::<RequestV1>(),
            context: (self as *mut Self).cast(),
            visitor: Some(visit),
        }
    }
}
unsafe extern "C" fn visit(context: *mut core::ffi::c_void, view: *const ViewV1) -> Status {
    let record = unsafe { &mut *context.cast::<Record>() };
    let view = unsafe { &*view };
    let runtime = unsafe { &*record.runtime };
    assert_eq!(view.runtime_identity, runtime.observation.identity);
    assert!(view.observation_identity > 0 && view.allocation_count > 0);
    assert_eq!(
        unsafe { runtime.observe_storage(core::ptr::without_provenance(8)) },
        BUSY
    );
    if record.mode == "active-read" {
        runtime.gc_live_count();
        panic!("active read returned");
    }
    if record.mode == "owner-before-active" {
        let address = record.runtime.addr();
        std::thread::spawn(move || unsafe {
            (&*core::ptr::with_exposed_provenance::<Runtime>(address))
                .observe_storage(core::ptr::without_provenance(8))
        })
        .join()
        .unwrap();
        panic!("wrong-thread observation returned");
    }
    record
        .events
        .push((view.runtime_identity, view.observation_identity));
    record.status
}
fn copy_request(request: &RequestV1) -> RequestV1 {
    RequestV1 { version: request.version, byte_size: request.byte_size, context: request.context, visitor: request.visitor }
}
unsafe extern "C" fn replacement(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    let request = unsafe { &*(*args).cast::<RequestV1>() };
    unsafe { (*request.context.cast::<Record>()).replacements += 1 };
    with_active(|runtime| {
        assert_eq!(
            unsafe { runtime.observe_storage(core::ptr::without_provenance(8)) },
            BUSY
        );
        let mut invalid = copy_request(request);
        invalid.version += 1;
        assert_eq!(unsafe { runtime.with_storage(&invalid) }, INVALID);
        let result = unsafe { runtime.with_storage(request) };
        assert_eq!(unsafe { runtime.with_storage(request) }, BUSY);
        result as usize as Oop
    })
}
unsafe extern "C" fn bypass(_: Oop, _: *const Oop, _: usize) -> Oop {
    41usize as Oop
}
unsafe extern "C" fn refusal(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    let request = unsafe { &*(*args).cast::<RequestV1>() };
    with_active(|runtime| {
        assert_eq!(unsafe { runtime.with_storage(request) }, INVALID);
        assert_eq!(
            unsafe { runtime.with_storage(request) },
            BUSY,
            "epoch refusal must consume the permission"
        );
        INVALID as usize as Oop
    })
}
fn check(record: &Record, expected: &[u64], stage: &str) {
    assert_eq!(
        record.events.iter().map(|e| e.1).collect::<Vec<_>>(),
        expected
    );
    assert!(
        record
            .events
            .iter()
            .all(|e| e.0 == unsafe { &*record.runtime }.observation.identity)
    );
    println!("\nobservation {stage}: {expected:?}");
}
#[test]
fn observation_scope_transcript() {
    let mut owners = vec![build_bootstrap(), build_bootstrap()];
    let first = owners.remove(0);
    let runtime = &first.runtime;
    assert!(
        runtime.observation.identity > 0
            && owners[0].runtime.observation.identity > runtime.observation.identity
    );
    println!("\nobservation owner identities: {} {}", runtime.observation.identity, owners[0].runtime.observation.identity);
    let mut record = Record::new(runtime);
    record.mode = std::env::var("BIRNAM_OBSERVATION_CASE").unwrap_or_default();
    let request = record.request();
    if !record.mode.is_empty() {
        unsafe { runtime.observe_storage(&request) };
        panic!("expected scope rejection");
    }
    let mut bad = copy_request(&request);
    bad.version += 1;
    assert_eq!(unsafe { runtime.observe_storage(&bad) }, INVALID);
    bad = copy_request(&request);
    bad.byte_size -= 1;
    assert_eq!(unsafe { runtime.observe_storage(&bad) }, INVALID);
    bad = copy_request(&request);
    bad.visitor = None;
    assert_eq!(unsafe { runtime.observe_storage(&bad) }, INVALID);
    assert_eq!(
        unsafe { runtime.observe_storage(core::ptr::null()) },
        INVALID
    );
    assert_eq!(
        unsafe { runtime.observe_storage(core::ptr::without_provenance(1)) },
        INVALID
    );
    check(&record, &[], "malformed requests do not visit");
    assert_eq!(unsafe { runtime.observe_storage(&request) }, OK);
    check(&record, &[1], "first accepted epoch");
    record.status = -77;
    assert_eq!(unsafe { runtime.observe_storage(&request) }, -77);
    check(&record, &[1, 2], "visitor failure consumes epoch");
    record.status = OK;
    assert_eq!(unsafe { runtime.observe_storage(&request) }, OK);
    check(&record, &[1, 2, 3], "scope resets after visitor failure");
    let selector = runtime.intern(b"withStorageView:").unwrap();
    let result = runtime.enter(|| {
        runtime.send_slice(
            runtime.globals().vtable_vt,
            selector,
            &[(&raw const request).cast_mut().cast()],
        )
    });
    assert_eq!(result.addr(), BUSY as usize);
    check(&record, &[1, 2, 3], "ordinary unpermitted invocation");
    runtime.add_method(runtime.globals().vtable_vt, selector, replacement);
    assert_eq!(unsafe { runtime.observe_storage(&request) }, OK);
    assert_eq!(record.replacements, 1);
    check(
        &record,
        &[1, 2, 3, 4],
        "live replacement preserves one-use permission",
    );
    runtime.add_method(runtime.globals().vtable_vt, selector, bypass);
    assert_eq!(unsafe { runtime.observe_storage(&request) }, 41);
    check(
        &record,
        &[1, 2, 3, 4],
        "replacement can decline default observation",
    );
    runtime.add_method(runtime.globals().vtable_vt, selector, with_storage_view);
    assert_eq!(unsafe { runtime.observe_storage(&request) }, OK);
    check(
        &record,
        &[1, 2, 3, 4, 5],
        "fresh method selection restores default",
    );
    unwind_flags(runtime);
    assert_eq!(unsafe { runtime.observe_storage(&request) }, OK);
    check(
        &record,
        &[1, 2, 3, 4, 5, 6],
        "unwind clears flags without resetting epoch",
    );
    println!("\nobservation moved owners retain distinct runtime identities");
    drop(first);
    let second = &owners[0].runtime;
    let mut other = Record::new(second);
    let request = other.request();
    assert_eq!(unsafe { second.observe_storage(&request) }, OK);
    check(&other, &[1], "independent owner survives destruction");
}
#[test]
fn advancement_refusal_consumes_permission_without_visiting() {
    let mut bootstrap = build_bootstrap();
    let runtime = &mut bootstrap.runtime;
    inject_refusal(runtime);
    let selector = runtime.intern(b"withStorageView:").unwrap();
    runtime.add_method(runtime.globals().vtable_vt, selector, refusal);
    let mut record = Record::new(runtime);
    let request = record.request();
    for _ in 0..2 {
        assert_eq!(unsafe { runtime.observe_storage(&request) }, INVALID);
    }
    check(
        &record,
        &[],
        "epoch refusal consumes permission and never visits",
    );
}
#[test]
fn scope_failures_keep_owner_and_active_diagnostics() {
    use std::os::unix::process::ExitStatusExt;
    for (mode, message) in [
        ("active-read", "Id access during storage observation"),
        (
            "owner-before-active",
            "Rust floor entered from a non-owner thread",
        ),
    ] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime::storage::scope_tests::observation_scope_transcript",
                "--nocapture",
            ])
            .env("BIRNAM_OBSERVATION_CASE", mode)
            .output()
            .unwrap();
        assert_eq!(result.status.signal(), Some(6), "{mode}");
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            format!("birnam-id invariant failed: {message}\n")
        );
    }
}
