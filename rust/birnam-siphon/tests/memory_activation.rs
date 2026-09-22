mod support;
use birnam_abi::{
    Oop,
    id_protocol::{Operand, Outcome},
};
use core::{ffi::c_void, ptr};
use support::*;
unsafe extern "C" fn no_references(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::unit())
}
#[test]
#[ignore = "scripts/native-message-test.sh builds the production modules"]
fn activation_requires_complete_representation_owners_without_replacing_the_heap() {
    unsafe {
        let id = id();
        let roots = load(id, "root_registry", &[]);
        let nodes = behavior(id);
        let before = allocate(id, nodes, 8);
        let identity = send(id, id.receiver, b"identityOf:", &[Operand::object(before)]).as_word();
        let token = retain(id, roots, before);
        let gc = load(id, "collect", &[Operand::object(roots)]);
        assert!(id.send_named(gc, b"activate", &[]).is_err());
        assert!(id.send_named(gc, b"collectPrecisely", &[]).is_err());
        assert!(present(id, before));
        let owner_behavior = behavior(id);
        install(
            id,
            owner_behavior,
            b"referencesOf:do:",
            no_references,
            ptr::null_mut(),
        );
        let owner = allocate(id, owner_behavior, 0);
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(nodes), Operand::object(owner)],
        );
        send(id, gc, b"activate", &[]);
        collect(id, gc);
        assert_eq!(
            send(id, id.receiver, b"identityOf:", &[Operand::object(before)]).as_word(),
            identity
        );
        // Enumeration follows current inherited attachment and parent messages.
        let inherited = object(send(id, nodes, b"delegated", &[]));
        let inherited_object = allocate(id, inherited, 8);
        let inherited_root = retain(id, roots, inherited_object);
        collect(id, gc);
        assert!(present(id, inherited_object));
        send(
            id,
            id.receiver,
            b"parentOf:put:",
            &[Operand::object(inherited), Operand::unit()],
        );
        assert!(id.send_named(gc, b"collectPrecisely", &[]).is_err());
        assert!(present(id, inherited_object));
        // A parent cycle also terminates rather than hanging hook selection.
        send(
            id,
            id.receiver,
            b"parentOf:put:",
            &[Operand::object(inherited), Operand::object(inherited)],
        );
        assert!(id.send_named(gc, b"collectPrecisely", &[]).is_err());
        send(
            id,
            id.receiver,
            b"parentOf:put:",
            &[Operand::object(inherited), Operand::object(nodes)],
        );
        collect(id, gc);
        release(id, roots, inherited_root);
        // Allocations made after activation must obey the same coverage rule.
        let uncovered_behavior = behavior(id);
        let uncovered = allocate(id, uncovered_behavior, 8);
        let uncovered_root = retain(id, roots, uncovered);
        assert!(id.send_named(gc, b"collectPrecisely", &[]).is_err());
        assert!(present(id, before) && present(id, uncovered));
        release(id, roots, uncovered_root);
        release(id, roots, token);
        collect(id, gc);
        assert!(!present(id, before) && !present(id, uncovered));
    }
}
