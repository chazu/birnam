//! Root registration is ordered and growable through the real message interface.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Enumerator {
    id: Protocol,
    ordinal: usize,
    child: Cell<Oop>,
    visits: *const RefCell<Vec<usize>>,
}
unsafe extern "C" fn enumerate(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let e = &*c.cast::<Enumerator>();
        (&*e.visits).borrow_mut().push(e.ordinal);
        let [visitor] = borrowed_slice(a, n).unwrap() else {
            panic!("visitor")
        };
        e.id.send_named(visitor.as_object().unwrap(), b"visit:", &[Operand::object(e.child.get())]).map_or_else(|e| e, Outcome::loaded)
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production dylibs"]
fn registered_root_enumerators_grow_and_keep_order_and_children_alive() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let parent = named(b"ObjectVt");
        let visits = Box::leak(Box::new(RefCell::new(Vec::new())));
        let mut objects = Vec::new();
        let mut contexts = Vec::new();
        for ordinal in 0..64 {
            let behavior = object(send(id, parent, b"delegated", &[]));
            let b = retain(id, roots, behavior);
            let child = allocate(id, parent, 0);
            let t = retain(id, roots, child);
            let context = Box::leak(Box::new(Enumerator {
                id,
                ordinal,
                child: Cell::new(child),
                visits,
            }));
            install(
                id,
                behavior,
                b"rootsDo:",
                enumerate,
                (context as *mut Enumerator).cast(),
            );
            let enumerator = allocate(id, behavior, 0);
            send(id, roots, b"register:", &[Operand::object(enumerator)]);
            release(id, roots, t);
            release(id, roots, b);
            objects.push(enumerator);
            contexts.push(context);
        }
        // Duplicates remain ordered registrations, as in the existing contract.
        send(id, roots, b"register:", &[Operand::object(objects[3])]);
        visits.borrow_mut().clear();
        collect(id, gc);
        assert_eq!(*visits.borrow(), (0..64).chain([3]).collect::<Vec<_>>());
        for (enumerator, context) in objects.iter().zip(&contexts) {
            assert!(present(id, *enumerator));
            assert!(present(id, context.child.get()));
        }
        // Only enumerators own these children; deleting the edge permits reclamation.
        let nil = named(b"nil");
        let children: Vec<_> = contexts.iter().map(|e| e.child.replace(nil)).collect();
        collect(id, gc);
        for child in children {
            assert!(!present(id, child));
        }
    }
}
