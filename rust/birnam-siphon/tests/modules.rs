//! Production native modules, reached only through Siphon and real Id messages.
use birnam_abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;

unsafe fn send(id: &Protocol, receiver: Oop, name: &[u8], args: &[Operand]) -> Operand {
    unsafe { id.send_named(receiver, name, args) }.expect("production message succeeds")
}
fn object(value: Operand) -> Oop {
    value
        .as_object()
        .expect("module boundary returns an Object")
}
unsafe fn load(
    id: &Protocol,
    directory: &std::path::Path,
    name: &str,
    args: &[Operand],
) -> Operand {
    let path = directory.join(format!("libbirnam_{name}.dylib"));
    let mut request = vec![Operand::bytes(path.as_os_str().as_bytes())];
    request.extend_from_slice(args);
    unsafe { id.send(id.receiver, id.load, &request) }.expect("load production module")
}
struct Counter {
    original: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn counted(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let state = unsafe { &*context.cast::<Counter>() };
    state.calls.set(state.calls.get() + 1);
    let original = state.original;
    unsafe { original.method.unwrap()(original.context, receiver, args, count) }
}
struct Snapshot {
    id: Protocol,
    roots: Oop,
    extra: Oop,
    retained: bool,
    token: usize,
    objects: Vec<Oop>,
}
unsafe extern "C" fn visit(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([value]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let state = unsafe { &mut *context.cast::<Snapshot>() };
    state.objects.push(object(*value));
    if !state.retained {
        state.retained = true;
        state.token = unsafe {
            send(
                &state.id,
                state.roots,
                b"retain:",
                &[Operand::object(state.extra)],
            )
        }
        .as_word()
        .unwrap();
    }
    Outcome::loaded(Operand::unit())
}
#[test]
#[ignore = "scripts/native-message-test.sh builds the production modules"]
fn roots_namespace_use_live_messages_and_preserve_reentrant_snapshots() {
    unsafe {
        let directory = std::path::PathBuf::from(
            std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").expect("production module directory"),
        );
        let id = birnam_siphon::load(directory.join("libbirnam_id.dylib").as_os_str().as_bytes())
            .expect("Siphon loads Id");
        let roots = object(load(&id, &directory, "root_registry", &[]));
        let namespace = object(load(
            &id,
            &directory,
            "namespace",
            &[Operand::object(roots)],
        ));
        let key = id.intern_name(b"sample").unwrap();
        let value = object(send(&id, id.receiver, b"objectPrototype", &[]));
        assert_eq!(
            send(&id, namespace, b"at:", &[Operand::object(key)]).kind(),
            native::UNIT
        );
        assert_eq!(
            object(send(
                &id,
                namespace,
                b"at:put:",
                &[Operand::object(key), Operand::object(value)]
            )),
            value
        );
        assert_eq!(
            object(send(&id, namespace, b"at:", &[Operand::object(key)])),
            value
        );
        assert_eq!(
            object(send(&id, namespace, b"nameOf:", &[Operand::object(value)])),
            key
        );
        assert_eq!(
            send(&id, namespace, b"nameAt:", &[Operand::word(usize::MAX)]).kind(),
            native::UNIT
        );
        assert!(
            id.send_named(namespace, b"at:", &[Operand::object(value)])
                .is_err()
        );

        // Namespace must consult Id's current Symbol validation method each time.
        let selector = id.intern_name(b"symbolBytesOf:").unwrap();
        let original = send(&id, id.receiver, b"lookup", &[Operand::object(selector)])
            .as_entry()
            .unwrap();
        let counter = Box::leak(Box::new(Counter {
            original,
            calls: Cell::new(0),
        }));
        send(
            &id,
            id.receiver,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(NativeEntry::new(counted, (counter as *mut Counter).cast())),
            ],
        );
        assert_eq!(
            object(send(&id, namespace, b"at:", &[Operand::object(key)])),
            value
        );
        assert_eq!(counter.calls.get(), 1);
        send(
            &id,
            id.receiver,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        );

        // Root providers are also called by fresh dispatch, after snapshotting.
        let behavior = object(send(
            &id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(namespace)],
        ));
        let selector = id.intern_name(b"rootsDo:").unwrap();
        let original = send(&id, behavior, b"lookup", &[Operand::object(selector)])
            .as_entry()
            .unwrap();
        let counter = Box::leak(Box::new(Counter {
            original,
            calls: Cell::new(0),
        }));
        send(
            &id,
            behavior,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(NativeEntry::new(counted, (counter as *mut Counter).cast())),
            ],
        );
        let extra = object(send(
            &id,
            object(send(
                &id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(value)],
            )),
            b"allocate",
            &[Operand::word(0)],
        ));
        let mut snapshot = Snapshot {
            id,
            roots,
            extra,
            retained: false,
            token: 0,
            objects: Vec::new(),
        };
        let visitor = Operand::entry(NativeEntry::new(visit, (&raw mut snapshot).cast()));
        send(&id, roots, b"rootsDo:", &[visitor]);
        assert_eq!(counter.calls.get(), 1);
        assert!(
            snapshot.objects.contains(&namespace)
                && snapshot.objects.contains(&key)
                && snapshot.objects.contains(&value)
        );
        assert!(
            !snapshot.objects.contains(&extra),
            "retention during a visitor enters the next snapshot"
        );
        snapshot.objects.clear();
        send(&id, roots, b"rootsDo:", &[visitor]);
        assert!(snapshot.objects.contains(&extra));
        assert_eq!(
            send(&id, roots, b"release:", &[Operand::word(snapshot.token)]).as_word(),
            Some(1)
        );
        assert_eq!(
            send(&id, roots, b"release:", &[Operand::word(snapshot.token)]).as_word(),
            Some(0)
        );
        send(
            &id,
            behavior,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        );
    }
}
