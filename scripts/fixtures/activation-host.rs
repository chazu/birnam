//! Activate the actual production heap with all Runtime successors absent.
use birnam_abi::id_protocol::{self as native, Operand};
use std::{os::unix::ffi::OsStrExt, path::Path};
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let prefix = Path::new(&args[1]);
    let adopt = args.get(3).is_some_and(|arg| arg == "adopt");
    if !adopt {
        assert_eq!(std::fs::read_dir(prefix).unwrap().count(), 22);
    }
    let mut initial =
        birnam_host::begin_id(&prefix.join("libbirnam_id.dylib"), None, None).unwrap();
    let owner = initial.initialize_collection().unwrap().unwrap();
    let roots = initial.roots().unwrap();
    let lifetime = initial.initialize_lifetime().unwrap().unwrap();
    let bootstrap = initial.bootstrap().unwrap();
    let integer = initial.initialize_integer().unwrap().unwrap();
    let fail = args.get(3).is_some_and(|arg| arg == "missing-layout");
    let arguments = [
        Operand::word(owner as usize),
        Operand::word(bootstrap as usize),
        Operand::word(roots as usize),
        Operand::word(lifetime as usize),
        Operand::word(usize::from(fail)),
        Operand::word(integer as usize),
    ];
    let answer =
        unsafe { initial.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &arguments) };
    assert_eq!(answer.status, native::LOADED);
    let collector = unsafe { *(*owner).collector };
    if fail {
        let owner = unsafe { &*owner };
        let records = unsafe { &*owner.records };
        let count = unsafe { (records.snapshot)(records.context).count };
        assert!(initial.activate_collection().is_err());
        assert_eq!(unsafe { (collector.active)(collector.context) }, 0);
        assert_eq!(unsafe { (records.snapshot)(records.context).count }, count);
        assert_eq!(
            unsafe { (*owner.phase_factory).read.unwrap()(owner.phase_context) },
            0
        );
        println!("activation rejects missing payload descriptions without marking or reclamation");
        return;
    }
    initial.activate_collection().unwrap();
    assert_eq!(unsafe { (collector.active)(collector.context) }, 1);
    let id = initial.id_protocol();
    for name in [
        b"saveActivationConstructors".as_slice(),
        b"exerciseActivationConstructors",
    ] {
        let selector = unsafe {
            id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
        };
        assert_eq!(selector.status, native::LOADED);
        let result = unsafe {
            id.message.unwrap()(
                id.context,
                id.receiver,
                selector.value.as_object().unwrap(),
                core::ptr::null(),
                0,
            )
        };
        assert_eq!(result.status, native::LOADED);
    }
    println!(
        "production activation before Runtime: allocation, delegation, intern and clone collect with live roots"
    );
    if adopt {
        let state = unsafe { &*owner };
        unsafe { (*state.budget_factory).replace.unwrap()(state.budget_context, 16 << 20) };
        let _floor = initial.finish().unwrap();
        assert!(unsafe { (*state.budget_factory).read.unwrap()(state.budget_context) } < 16 << 20);
        let selector = unsafe {
            id.message.unwrap()(
                id.context,
                id.receiver,
                id.intern,
                &Operand::bytes(b"exerciseActivationConstructors"),
                1,
            )
        };
        let result = unsafe {
            id.message.unwrap()(
                id.context,
                id.receiver,
                selector.value.as_object().unwrap(),
                core::ptr::null(),
                0,
            )
        };
        assert_eq!(result.status, native::LOADED);
        println!(
            "automatic collection during Runtime construction and saved constructors after adoption pass"
        );
    }
}
