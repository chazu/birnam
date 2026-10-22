//! Id's batched binding facts must never execute foreign lookup callbacks.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{cell::Cell, ffi::c_void};
use support::*;
unsafe extern "C" fn answer(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(9))
}

fn batches_recheck_every_row_and_the_current_lookup_between_calls(id: Protocol) {
    unsafe {
        let parent = behavior(id);
        let child = object(send(id, parent, b"delegated", &[]));
        let first = allocate(id, parent, 0);
        let last = allocate(id, child, 0);
        let selector = id.intern_name(b"probe").unwrap();
        let lookup = id.intern_name(b"lookup").unwrap();
        let bind = |receiver, selector| {
            send(id, id.receiver, b"bind:to:", &[
                Operand::object(selector), Operand::object(receiver),
            ]).as_entry().unwrap()
        };
        let matcher = bind(id.receiver, id.intern_name(b"nativeBindingsMatch:").unwrap());
        let original_lookup = bind(id.receiver, lookup);
        let add = bind(id.receiver, id.intern_name(b"addMethod").unwrap());
        let calls = Cell::new(0usize);
        let context = (&calls as *const Cell<usize>).cast_mut().cast();
        install(id, parent, b"probe", answer, context);
        // Mix direct and inherited selection, with a non-null context witness.
        let mut rows = Vec::new();
        for receiver in [first, last].into_iter().cycle().take(32) {
            rows.extend([receiver.addr(), selector.addr(), answer as *const () as usize, context.addr()]);
        }
        let query = |rows: &[usize]| {
            let bytes = core::slice::from_raw_parts(rows.as_ptr().cast::<u8>(), size_of_val(rows));
            let args = [Operand::bytes(bytes)];
            (matcher.method.unwrap())(matcher.context, id.receiver, args.as_ptr(), 1)
                .into_result().unwrap().as_word().unwrap()
        };
        assert_eq!(query(&rows), 1);
        let mut wrong = rows.clone();
        wrong[127] = 0;
        assert_eq!(query(&wrong), 0, "the last row's context must be checked");
        wrong[127] = context.addr();
        wrong[125] = 7;
        assert_eq!(query(&wrong), 0, "the last selector must be a live allocation");
        install(id, child, b"probe", watched, context);
        assert_eq!(query(&rows), 0, "an inherited target may be overridden");
        send(id, child, b"removeMethod:", &[Operand::object(selector)]);
        assert_eq!(query(&rows), 1);
        let set_lookup = |entry: NativeEntry| {
            let args = [Operand::object(lookup), Operand::entry(entry)];
            (add.method.unwrap())(add.context, id.receiver, args.as_ptr(), args.len())
                .into_result().unwrap();
        };
        // Invoke the saved matcher directly so a custom lookup cannot intercept
        // the query itself. Its inspection must decline without calling lookup.
        set_lookup(NativeEntry::new(watched, context));
        assert_eq!(query(&rows), 0);
        assert_eq!(calls.get(), 0);
        set_lookup(original_lookup);
        assert_eq!(query(&rows), 1, "restoration is visible on the next call");
        let meta = behavior(id);
        install(id, meta, b"lookup", watched, context);
        send(id, id.receiver, b"reassociateBehaviorOf:with:", &[
            Operand::object(child), Operand::object(meta),
        ]);
        assert_eq!(query(&rows), 0, "a later row can have custom metabehavior");
        assert_eq!(calls.get(), 0);
    }
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
        batches_recheck_every_row_and_the_current_lookup_between_calls(id);
    }
}
