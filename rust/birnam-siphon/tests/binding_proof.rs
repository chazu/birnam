//! Id's batched binding facts must never execute foreign lookup callbacks.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{Operand, Outcome},
};
use std::{cell::Cell, ffi::c_void};
use support::*;
unsafe extern "C" fn answer(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(9))
}
unsafe extern "C" fn watched(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe { &*c.cast::<Cell<usize>>() }.set(1);
    Outcome::loaded(Operand::unit())
}
#[test]
#[ignore = "requires current native Id"]
fn facts_are_exact_and_decline_custom_lookup_without_calling_it() {
    unsafe {
        let id = id();
        let b = behavior(id);
        let o = allocate(id, b, 0);
        install(id, b, b"probe", answer, std::ptr::null_mut());
        let sel = id.intern_name(b"probe").unwrap();
        let row = [o.addr(), sel.addr(), answer as usize, 0usize];
        let query = |rows: &[usize]| {
            let bytes = core::slice::from_raw_parts(rows.as_ptr().cast::<u8>(), size_of_val(rows));
            send(
                id,
                id.receiver,
                b"nativeBindingsMatch:",
                &[Operand::bytes(bytes)],
            )
            .as_word()
            .unwrap()
        };
        assert_eq!(query(&row), 1);
        let mut wrong = row;
        wrong[2] = watched as usize;
        assert_eq!(query(&wrong), 0);
        wrong = row;
        wrong[0] = 7;
        assert_eq!(query(&wrong), 0);
        let calls = Cell::new(0usize);
        let meta = behavior(id);
        install(
            id,
            meta,
            b"lookup:",
            watched,
            (&calls as *const Cell<usize>).cast_mut().cast(),
        );
        send(
            id,
            id.receiver,
            b"reassociateBehaviorOf:with:",
            &[Operand::object(b), Operand::object(meta)],
        );
        assert_eq!(query(&row), 0);
        assert_eq!(calls.get(), 0);
        assert!(
            id.send_named(
                id.receiver,
                b"nativeBindingsMatch:",
                &[Operand::bytes(&[0; 1])]
            )
            .is_err()
        );
    }
}
