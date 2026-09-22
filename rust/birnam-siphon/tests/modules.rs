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
        boolean_nil(&id, &directory, namespace);
    }
}

struct Branch {
    answer: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn branch_value(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    assert_eq!(count, 0);
    let branch = unsafe { &*context.cast::<Branch>() };
    branch.calls.set(branch.calls.get() + 1);
    Outcome::loaded(Operand::object(branch.answer))
}
unsafe fn boolean_nil(id: &Protocol, directory: &std::path::Path, namespace: Oop) {
    unsafe {
        let truth = object(load(
            id,
            directory,
            "boolean",
            &[Operand::object(namespace)],
        ));
        let get = |name: &[u8]| {
            object(send(
                id,
                namespace,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let falsity = get(b"false");
        let nil = get(b"nil");
        assert_eq!(get(b"Boolean"), truth);
        assert_eq!(get(b"true"), truth);
        assert_eq!(get(b"False"), falsity);
        assert_ne!(truth, falsity);
        assert_ne!(truth, nil);
        assert_eq!(
            load(id, directory, "nil", &[Operand::object(namespace)]).kind(),
            native::UNIT
        );
        for (value, opposite) in [(truth, falsity), (falsity, truth)] {
            assert_eq!(object(send(id, value, b"not", &[])), opposite);
            for other in [truth, falsity, nil] {
                assert_eq!(
                    object(send(id, value, b"==", &[Operand::object(other)])),
                    if value == other { truth } else { falsity }
                );
                assert_eq!(
                    object(send(id, value, b"&", &[Operand::object(other)])),
                    if value == truth { other } else { falsity }
                );
                assert_eq!(
                    object(send(id, value, b"|", &[Operand::object(other)])),
                    if value == truth { truth } else { other }
                );
            }
        }
        let prototype = object(send(id, id.receiver, b"objectPrototype", &[]));
        for value in [truth, falsity, nil, prototype] {
            assert_eq!(
                object(send(id, value, b"isNil", &[])),
                if value == nil { truth } else { falsity }
            );
            assert_eq!(
                object(send(id, value, b"notNil", &[])),
                if value == nil { falsity } else { truth }
            );
        }
        let parent = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(prototype)],
        ));
        let behavior = object(send(id, parent, b"delegated", &[]));
        let block = object(send(id, behavior, b"allocate", &[Operand::word(0)]));
        let branch = Box::leak(Box::new(Branch {
            answer: nil,
            calls: Cell::new(0),
        }));
        let selector = id.intern_name(b"value").unwrap();
        send(
            id,
            behavior,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(NativeEntry::new(
                    branch_value,
                    (branch as *mut Branch).cast(),
                )),
            ],
        );
        let yes = Operand::object(block);
        // The unchosen argument deliberately has no value method.
        let unused = Operand::object(prototype);
        for (receiver, name, args, answer, called) in [
            (
                truth,
                b"ifTrue:ifFalse:".as_slice(),
                vec![yes, unused],
                nil,
                true,
            ),
            (falsity, b"ifTrue:ifFalse:", vec![unused, yes], nil, true),
            (truth, b"ifTrue:", vec![yes], nil, true),
            (falsity, b"ifTrue:", vec![unused], nil, false),
            (truth, b"ifFalse:", vec![unused], nil, false),
            (falsity, b"ifFalse:", vec![yes], nil, true),
            (truth, b"and:", vec![yes], nil, true),
            (falsity, b"and:", vec![unused], falsity, false),
            (truth, b"or:", vec![unused], truth, false),
            (falsity, b"or:", vec![yes], nil, true),
        ] {
            let before = branch.calls.get();
            assert_eq!(object(send(id, receiver, name, &args)), answer);
            assert_eq!(branch.calls.get() - before, usize::from(called));
        }
        let replacement = Box::leak(Box::new(Branch {
            answer: truth,
            calls: Cell::new(0),
        }));
        send(
            id,
            behavior,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(NativeEntry::new(
                    branch_value,
                    (replacement as *mut Branch).cast(),
                )),
            ],
        );
        assert_eq!(object(send(id, falsity, b"or:", &[yes])), truth);
        assert_eq!(replacement.calls.get(), 1);
        assert!(id.send_named(truth, b"ifTrue:", &[]).is_err());
        assert!(id.send_named(truth, b"&", &[Operand::word(0)]).is_err());
        // Loading again must not reinstall methods over application changes.
        let boolean_behavior = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(truth)],
        ));
        let selector = id.intern_name(b"not").unwrap();
        let original = send(
            id,
            boolean_behavior,
            b"lookup",
            &[Operand::object(selector)],
        )
        .as_entry()
        .unwrap();
        let counter = Box::leak(Box::new(Counter {
            original,
            calls: Cell::new(0),
        }));
        send(
            id,
            boolean_behavior,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(NativeEntry::new(counted, (counter as *mut Counter).cast())),
            ],
        );
        assert_eq!(
            object(load(
                id,
                directory,
                "boolean",
                &[Operand::object(namespace)]
            )),
            truth
        );
        assert_eq!(object(send(id, truth, b"not", &[])), falsity);
        assert_eq!(counter.calls.get(), 1);
    }
}
