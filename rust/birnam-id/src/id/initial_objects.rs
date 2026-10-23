//! Direct seed construction, then the real message path installs later methods.
use super::{native_id::State, object_memory::ObjectMemory, symbol::Symbols, vtable::Vtables};
use crate::raw::AddressMap;
use core::cell::{Cell, RefCell};
pub(crate) fn construct() -> Box<State> {
    let mut heap = ObjectMemory::new();
    let mut tables = Vtables::new();
    let mut symbols = Symbols::new();
    let nil = core::ptr::null_mut();
    let meta = tables.create(&mut heap, nil, nil);
    assert!(heap.set_vtable(meta, meta));
    let object = tables.create(&mut heap, nil, meta);
    tables.set_parent(meta, object);
    let mut family = || tables.create(&mut heap, object, meta);
    let symbol = family();
    let name = family();
    let size = family();
    let none = family();
    let method = family();
    let fault = family();
    let sequence = family();
    let module = family();
    let root_behavior = family();
    let registry_behavior = family();
    // Native implementations must remain applicable while ordinary lookup is
    // being replaced. This is a second ordinary, self-metavtable bootstrap root.
    let native_meta = tables.create(&mut heap, meta, meta);
    assert!(heap.set_vtable(native_meta, native_meta));
    assert!(heap.set_vtable(method, native_meta));
    let registry = heap.allocate(0);
    assert!(heap.set_vtable(registry, registry_behavior));
    let root = heap.allocate(size_of::<usize>());
    assert!(heap.set_vtable(root, root_behavior));
    let lookup = symbols.intern(&mut heap, symbol, b"lookup").unwrap();
    let dnu = symbols
        .intern(&mut heap, symbol, b"doesNotUnderstand:")
        .unwrap();
    let mut s = Box::new(State {
        protocol: crate::abi::id_protocol::Protocol {
            version: 2,
            byte_size: size_of::<crate::abi::id_protocol::Protocol>() as u32,
            message: super::native_id::message,
            root,
            intern: nil,
            load: nil,
            at: nil,
            seeds: nil,
            invalid: nil,
        },
        heap: RefCell::new(heap),
        tables: RefCell::new(tables),
        symbols: RefCell::new(symbols),
        root,
        root_behavior,
        native_apply: Cell::new(nil),
        native_meta,
        reference_owners: RefCell::new(Vec::new()),
        meta,
        object,
        symbol,
        name,
        size,
        none,
        method,
        fault,
        sequence,
        module,
        lookup,
        dnu,
        modules: super::native_modules::NativeModules::new(registry, registry_behavior),
        names: RefCell::new(std::collections::HashMap::new()),
        detached: RefCell::new(AddressMap::default()),
        active: RefCell::new(Vec::new()),
        permanent: RefCell::new(vec![
            root, meta, object, symbol, name, size, none, method, fault, sequence, module, lookup,
            dnu, registry, native_meta,
        ]),
        default_lookup: Cell::new(nil),
        invalid_fault: Cell::new(nil),
        unbound_fault: Cell::new(nil),
        owner: std::thread::current().id(),
    });
    let pointer = core::ptr::from_mut(&mut *s);
    unsafe { root.cast::<*const State>().write(pointer) };
    s.initialize();
    s.protocol.intern = s.selector(b"intern");
    s.protocol.load = s.selector(b"loadNative:");
    s.protocol.at = s.selector(b"at:");
    s.protocol.invalid = s.invalid();
    s
}
