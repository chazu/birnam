//! One unfiltered physical-heap report under normal automatic collection.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{Operand, Outcome, Protocol},
};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Report {
    id: Protocol,
    gc: Oop,
    previous: Cell<usize>,
    count: Cell<usize>,
    completed: Cell<bool>,
    settings_writes: Cell<usize>,
}
unsafe extern "C" fn changed_setting(
    c: *mut c_void,
    _: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    let r = unsafe { &*c.cast::<Report>() };
    r.settings_writes.set(r.settings_writes.get() + 1);
    Outcome::failed(1, 99)
}
unsafe extern "C" fn row(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let r = unsafe { &*c.cast::<Report>() };
    let [value, identity] = unsafe { borrowed_slice(a, n) }.unwrap() else {
        panic!("row shape")
    };
    let identity = identity.as_word().unwrap();
    assert!(
        identity > r.previous.replace(identity),
        "each allocation reported once in identity order"
    );
    assert!(unsafe { present(r.id, object(*value)) });
    r.count.set(r.count.get() + 1);
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn complete(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let r = unsafe { &*c.cast::<Report>() };
    let [gaps] = unsafe { borrowed_slice(a, n) }.unwrap() else {
        panic!("completion shape")
    };
    assert_eq!(gaps.as_word().unwrap() & 1, 1);
    assert!(!r.completed.replace(true));
    unsafe { collect(r.id, r.gc) };
    Outcome::loaded(Operand::word(73))
}
#[test]
#[ignore = "requires production authored Trace"]
fn complete_physical_inventory_survives_automatic_and_completion_collection() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |n: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(n).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        send(id, gc, b"precisePayloads:", &[Operand::word(1)]);
        let r = Box::leak(Box::new(Report {
            id,
            gc,
            previous: Cell::new(0),
            count: Cell::new(0),
            completed: Cell::new(false),
            settings_writes: Cell::new(0),
        }));
        let c = (r as *mut Report).cast();
        let gc_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(gc)],
        ));
        install(id, gc_vt, b"automaticCollection:", changed_setting, c);
        let vt = behavior(id);
        install(id, vt, b"reached:identity:", row, c);
        install(id, vt, b"notReached:identity:", row, c);
        install(id, vt, b"complete:", complete, c);
        let visitor = allocate(id, vt, 0);
        retain(id, roots, visitor);
        collect(id, gc);
        let result = send(
            id,
            named(b"Trace"),
            b"report:acceptingGaps:",
            &[Operand::object(visitor), Operand::word(usize::MAX)],
        );
        assert_eq!(result.as_word(), Some(73));
        assert!(r.completed.get());
        assert!(r.count.get() > 1000);
        eprintln!("unfiltered Trace reported {} allocations", r.count.get());
        assert_eq!(r.settings_writes.get(), 0);
    }
}
