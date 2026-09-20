use super::*;
use core::cell::{Cell, RefCell};

#[derive(Default)]
struct Recording { values: RefCell<Vec<usize>>, scope: Cell<*mut c_void> }
unsafe extern "C" fn emit(context: *mut c_void, value: usize) -> i32 {
    let recording = unsafe { &*context.cast::<Recording>() };
    recording.values.borrow_mut().push(value);
    if value == 1 { unsafe { mark(recording.scope.get(), 2usize as Oop) }; }
    OK
}
fn sink(recording: &Recording) -> SinkV1 {
    SinkV1 { context: ptr::from_ref(recording).cast_mut().cast(), emit }
}
#[test]
fn copied_sink_immediate_duplicates_nulls_reentry_and_later_scope() {
    let first = Recording::default(); let second = Recording::default();
    let mut selected = sink(&first);
    unsafe {
        let scope = create(); first.scope.set(scope); second.scope.set(scope);
        begin(scope, &selected); selected.context = ptr::null_mut();
        mark(scope, ptr::null_mut()); mark(scope, 1usize as Oop); mark(scope, 1usize as Oop); mark(scope, usize::MAX as Oop);
        assert_eq!(*first.values.borrow(), [1, 2, 1, 2, usize::MAX]);
        end(scope); begin(scope, &sink(&second)); mark(scope, 1usize as Oop); end(scope);
        assert_eq!(*second.values.borrow(), [1, 2]);
        assert!(selected.context.is_null()); destroy(scope);
    }
}
#[test]
fn independent_owners_can_have_simultaneous_scopes_on_one_thread() {
    let first = Recording::default(); let second = Recording::default();
    unsafe {
        let a = create(); let b = create();
        first.scope.set(a); second.scope.set(b);
        begin(a, &sink(&first)); begin(b, &sink(&second));
        mark(a, 1usize as Oop); mark(b, 7usize as Oop); end(a);
        mark(b, 8usize as Oop); end(b); destroy(a); destroy(b);
    }
    assert_eq!(*first.values.borrow(), [1, 2]);
    assert_eq!(*second.values.borrow(), [7, 8]);
}
#[test]
fn failures_preserve_exact_abort_diagnostics() {
    const CHILD: &str = "BIRNAM_NATIVE_MARK_CHILD";
    if let Ok(mode) = std::env::var(CHILD) {
        unsafe extern "C" fn reject(_: *mut c_void, _: usize) -> i32 { -77 }
        let recording = Recording::default();
        unsafe {
            let scope = create(); let mut selected = sink(&recording);
            if mode == "reject" { selected.emit = reject; }
            begin(scope, &selected);
            match mode.as_str() {
                "nested" => { begin(scope, ptr::null()); }
                "escaped-null" => { end(scope); mark(scope, ptr::null_mut()); }
                "escaped-value" => { end(scope); mark(scope, 1usize as Oop); }
                "reject" => { mark(scope, 3usize as Oop); }
                _ => unreachable!(),
            }
        }
        panic!("expected abort");
    }
    use std::os::unix::process::ExitStatusExt;
    for (mode, diagnostic) in [
        ("nested", "GC mark callback reentered"),
        ("escaped-null", "GC mark callback escaped its invocation"),
        ("escaped-value", "GC mark callback escaped its invocation"),
        ("reject", "GC tracer rejected a candidate"),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::failures_preserve_exact_abort_diagnostics", "--nocapture"])
            .env(CHILD, mode).output().unwrap();
        assert_eq!(output.status.signal(), Some(6), "{mode}");
        assert_eq!(String::from_utf8(output.stderr).unwrap(), format!("birnam-id invariant failed: {diagnostic}\n"));
    }
}
