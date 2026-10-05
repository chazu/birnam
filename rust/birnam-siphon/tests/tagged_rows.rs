//! Generic tagged-table operations use current Id messages and survive GC.
mod support;
use support::*;
use birnam_abi::{Oop, id_protocol::{Protocol, NativeEntry, Operand, Outcome}};
use core::{cell::Cell, ffi::c_void};
struct Probe { id: Protocol, gc: Oop, original: NativeEntry, calls: Cell<usize> }
unsafe extern "C" fn collecting(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let p = unsafe { &*c.cast::<Probe>() };
    p.calls.set(p.calls.get()+1);
    unsafe { collect(p.id, p.gc); p.original.method.unwrap()(p.original.context, r, a, n) }
}
unsafe extern "C" fn zero(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(0))
}
#[test]
#[ignore = "requires native prefix"]
fn tagged_tables_preserve_positions_and_live_numeric_owners() {
    unsafe {
        let id = id(); let ns = load(id, "bootstrap", &[Operand::bytes(directory().as_os_str().as_encoded_bytes())]);
        let named = |name: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(name).unwrap())]));
        let roots = named(b"Roots"); let gc = named(b"GC"); let mut tokens = Vec::new();
        let mut keep = |o| { tokens.push(retain(id, roots, o)); o };
        let text = |b: &[u8]| object(send(id, named(b"String"), b"fromBytes:", &[Operand::bytes(b)]));
        let number = |v| object(send(id, named(b"Integer"), b"fromWord:", &[Operand::word(v)]));
        let mut bytes = 2u32.to_le_bytes().to_vec();
        bytes.extend([0]); bytes.extend(2u32.to_le_bytes()); bytes.extend([0,2]); bytes.extend(1u32.to_le_bytes());
        bytes.extend([2]); bytes.extend(1u32.to_le_bytes()); bytes.extend(1u32.to_le_bytes()); bytes.extend([2]); bytes.extend(0u32.to_le_bytes());
        let blob = keep(object(send(id, named(b"Bytes"), b"fromBytes:", &[Operand::bytes(&bytes)])));
        let schema = keep(text(b"012")); let mask = keep(text(b"100")); let refs = keep(text(b"001"));
        let two = keep(number(2));
        let rows = keep(object(send(id, blob, b"readTaggedTableWithSchema:limit:", &[Operand::object(schema), Operand::object(two)])));
        let nil = named(b"nil");
        let failed = keep(object(send(id, rows, b"firstLocatorOutside:", &[Operand::object(mask)])));
        assert_eq!(send(id, named(b"Integer"), b"wordOf:", &[Operand::object(failed)]).as_word(), Some(1));
        assert_eq!(object(send(id, rows, b"firstIndexForTags:atLeast:", &[Operand::object(refs), Operand::object(two)])), nil);
        let selected = keep(object(send(id, rows, b"whereLocatorTags:", &[Operand::object(refs)])));
        let z = keep(number(0));
        let row = keep(object(send(id, selected, b"at:", &[Operand::object(z)])));
        let position = keep(object(send(id, row, b"sourceIndex", &[])));
        assert_eq!(send(id, named(b"Integer"), b"wordOf:", &[Operand::object(position)]).as_word(), Some(1));
        // Concat creates a new base ordering; selection retains its source index.
        let joined = keep(object(send(id, selected, b"concat:", &[Operand::object(rows)])));
        let position = keep(object(send(id, joined, b"firstLocatorOutside:", &[Operand::object(mask)])));
        assert_eq!(send(id, named(b"Integer"), b"wordOf:", &[Operand::object(position)]).as_word(), Some(0));
        let vt = named(b"IntegerVt");
        let selector = id.intern_name(b"wordOf:").unwrap();
        let original = send(id, id.receiver, b"bindingOf:in:", &[Operand::object(selector), Operand::object(vt)]).as_entry().unwrap();
        let mut probe = Probe { id, gc, original, calls: Cell::new(0) };
        install(id, vt, b"wordOf:", collecting, core::ptr::from_mut(&mut probe).cast());
        assert_eq!(object(send(id, rows, b"firstIndexForTags:atLeast:", &[Operand::object(refs), Operand::object(two)])), nil);
        assert!(probe.calls.get() > 0);
        assert!(present(id, rows));
        install(id, vt, b"wordOf:", zero, core::ptr::null_mut());
        let changed = keep(object(send(id, rows, b"firstIndexForTags:atLeast:", &[Operand::object(refs), Operand::object(two)])));
        assert_ne!(changed, nil, "the bound is obtained through the current Integer owner");
        install(id, vt, b"wordOf:", original.method.unwrap(), original.context);
        let selector = id.intern_name(b"fromWord:").unwrap();
        let original = send(id, id.receiver, b"bindingOf:in:", &[Operand::object(selector), Operand::object(vt)]).as_entry().unwrap();
        let mut probe = Probe { id, gc, original, calls: Cell::new(0) };
        install(id, vt, b"fromWord:", collecting, core::ptr::from_mut(&mut probe).cast());
        let result = keep(object(send(id, row, b"sourceIndex", &[])));
        assert!(probe.calls.get() > 0);
        assert!(present(id, result));
        install(id, vt, b"fromWord:", original.method.unwrap(), original.context);
        for token in tokens { release(id, roots, token); }
    }
}
