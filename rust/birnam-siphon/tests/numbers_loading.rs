//! Bootstrap must tolerate collection during construction and preserve replacements.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Loading {
    id: Protocol,
    entry: NativeEntry,
    integer: Cell<bool>,
    text: Cell<bool>,
}
unsafe extern "C" fn receiver(_: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(r))
}
unsafe extern "C" fn load_hook(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Loading>() };
    let result = (|| unsafe {
        let args = borrowed_slice(a, n).unwrap();
        let path = args[0].as_bytes().unwrap();
        let path = borrowed_bytes(path.pointer, path.length).unwrap().to_vec();
        if path.ends_with(b"libbirnam_object_model.dylib") {
            let gc =
                s.id.send_named(s.id.receiver, b"memoryManager", &[])?
                    .as_object()
                    .unwrap();
            s.id.send_named(gc, b"budget:", &[Operand::word(1)])?;
        }
        let loaded = s.entry.method.unwrap()(s.entry.context, r, a, n).into_result()?;
        if path.ends_with(b"libbirnam_generation.dylib") {
            // Every value constructor and extension above ran with collection on
            // each allocation. Authored-tool startup has separate GC coverage;
            // keeping budget=1 there would make this constructor test quadratic.
            let gc=s.id.send_named(s.id.receiver,b"memoryManager",&[])?.as_object().unwrap();
            s.id.send_named(gc,b"budget:",&[Operand::word(1024*1024)])?;
        }
        for (library, selector, flag) in [
            (
                b"libbirnam_integer.dylib".as_slice(),
                b"+".as_slice(),
                &s.integer,
            ),
            (b"libbirnam_string.dylib", b"size", &s.text),
            (b"libbirnam_integer.dylib", b"to:", &s.integer),
            (b"libbirnam_string.dylib", b"split:", &s.text),
        ] {
            if path.ends_with(library) {
                let behavior =
                    s.id.send_named(s.id.receiver, b"behaviorOf:", &[loaded])?
                        .as_object()
                        .unwrap();
                s.id.send_named(
                    behavior,
                    b"addMethod",
                    &[
                        Operand::object(s.id.intern_name(selector)?),
                        Operand::entry(NativeEntry::new(receiver, ptr::null_mut())),
                    ],
                )?;
                flag.set(true);
            }
        }
        Ok(loaded)
    })();
    result.map_or_else(|e| e, Outcome::loaded)
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production dylibs"]
fn complete_extensions_preserve_replacements_during_collecting_bootstrap() {
    unsafe {
        let id = id();
        let entry = send(id, id.receiver, b"lookup", &[Operand::object(id.load)])
            .as_entry()
            .unwrap();
        let context = Box::leak(Box::new(Loading {
            id,
            entry,
            integer: Cell::new(false),
            text: Cell::new(false),
        }));
        send(
            id,
            id.receiver,
            b"addMethod",
            &[
                Operand::object(id.load),
                Operand::entry(NativeEntry::new(load_hook, ptr::from_mut(context).cast())),
            ],
        );
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        send(
            id,
            id.receiver,
            b"addMethod",
            &[Operand::object(id.load), Operand::entry(entry)],
        );
        assert!(context.integer.get() && context.text.get());
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        // Explicit Integer policy admission must preserve the replacement +
        // installed by the load hook, while migrating untouched arithmetic.
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let module = object(send(id, named(b"Globals"), b"at:",
            &[Operand::object(id.intern_name(b"Module").unwrap())]));
        let path = object(send(id, named(b"String"), b"fromBytes:",
            &[Operand::bytes(directory().join("libbirnam_integerValues.dylib").as_os_str().as_bytes())]));
        let path_token = retain(id, roots, path);
        assert_eq!(send(id, named(b"Codegen"), b"load:", &[Operand::object(path)]).as_object(), Some(module));
        release(id, roots, path_token);
        let integer = named(b"Integer");
        let float = named(b"Float");
        let string = named(b"String");
        let left = object(send(id, integer, b"fromWord:", &[Operand::word(3)]));
        let a = retain(id, roots, left);
        let right = object(send(
            id,
            float,
            b"fromBits:",
            &[Operand::word(0.5f64.to_bits() as usize)],
        ));
        let b = retain(id, roots, right);
        assert_eq!(
            object(send(id, left, b"+", &[Operand::object(right)])),
            left
        );
        assert_eq!(
            object(send(id, left, b"to:", &[Operand::object(left)])),
            left
        );
        let difference = object(send(id, left, b"-", &[Operand::object(right)]));
        let c = retain(id, roots, difference);
        assert_eq!(
            send(id, difference, b"bits", &[]).as_word(),
            Some(2.5f64.to_bits() as usize)
        );
        let text = object(send(id, string, b"fromBytes:", &[Operand::bytes(b"123")]));
        let d = retain(id, roots, text);
        assert_eq!(object(send(id, text, b"size", &[])), text);
        assert_eq!(
            object(send(id, text, b"split:", &[Operand::object(text)])),
            text
        );
        let parsed = object(send(id, text, b"asInteger", &[]));
        let e = retain(id, roots, parsed);
        assert_eq!(send(id, parsed, b"word", &[]).as_word(), Some(123));
        for token in [a, b, c, d, e] {
            release(id, roots, token);
        }
        collect(id, gc);
    }
}
