//! The initial Id object graph, constructed before any upper runtime service.
use crate::abi::id_protocol::{Operand, LOADED};
#[cfg(test)]
use crate::abi::Oop;
use crate::abi::id_protocol::NativeEntry;
use crate::id::object_memory::ObjectMemory;
use crate::id::symbol::Symbols;
use crate::id::vtable::Vtables;

pub(crate) struct InitialObjects {
    pub(crate) protocol: Box<crate::id::native_id::State>,
    #[cfg(test)]
    pub(crate) vtable_vt: Oop,
    #[cfg(test)]
    pub(crate) object_vt: Oop,
    #[cfg(test)]
    pub(crate) lookup: Oop,
    #[cfg(test)]
    pub(crate) does_not_understand: Oop,
    #[cfg(test)]
    pub(crate) object_prototype: Oop,
    #[cfg(test)]
    pub(crate) symbol_prototype: Oop,
}

impl InitialObjects {
    pub(crate) fn new() -> Self {
        let mut memory = crate::owned::Owned::new(ObjectMemory::new());
        let mut dictionaries = crate::owned::Owned::new(Vtables::new());
        let mut symbols = crate::owned::Owned::new(Symbols::new());
        let vtable_vt = dictionaries.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(memory.set_vtable(vtable_vt, vtable_vt));
        let object_vt = dictionaries.create(&mut memory, core::ptr::null_mut(), vtable_vt);
        dictionaries.set_parent(vtable_vt, object_vt);
        let symbol_vt = dictionaries.create(&mut memory, object_vt, vtable_vt);
        let object_prototype = memory.allocate(0);
        assert!(memory.set_vtable(object_prototype, object_vt));
        let lookup = symbols.intern(&mut memory, symbol_vt, b"lookup").unwrap();
        let does_not_understand = symbols.intern(&mut memory, symbol_vt, b"doesNotUnderstand:").unwrap();
        let _symbol_prototype = symbols.intern(&mut memory, symbol_vt, b"Symbol").unwrap();
        let add_method = symbols.intern(&mut memory, symbol_vt, b"addMethod").unwrap();
        let remove_method = symbols.intern(&mut memory, symbol_vt, b"removeMethod:").unwrap();
        let context = crate::id::id_dispatch::Context {
            dictionaries: dictionaries.pointer(), metavtable: vtable_vt,
            lookup, does_not_understand,
            #[cfg(test)]
            size_probe_behavior: core::ptr::null_mut(),
        };
        let entry = |method| NativeEntry::new(method, context.dictionaries.cast());
        dictionaries.add_method(vtable_vt, lookup, entry(crate::id::id_dispatch::vtable_lookup_method));
        dictionaries.add_method(vtable_vt, add_method, entry(crate::id::id_dispatch::vtable_add_method));
        // Only the first two bindings are ad hoc. The next uses Id's message path.
        unsafe {
            assert_eq!(crate::id::id_dispatch::send(context, vtable_vt, add_method, &[
                Operand::object(remove_method), Operand::entry(entry(crate::id::id_dispatch::vtable_remove_method)),
            ]).status, LOADED);
        }
        let protocol = unsafe { crate::id::native_id::State::new(memory, dictionaries, symbols, vtable_vt, symbol_vt, object_prototype, lookup, does_not_understand) };
        Self { protocol,
            #[cfg(test)]
            vtable_vt,
            #[cfg(test)]
            object_vt,
            #[cfg(test)]
            lookup,
            #[cfg(test)]
            does_not_understand,
            #[cfg(test)]
            object_prototype,
            #[cfg(test)]
            symbol_prototype: _symbol_prototype,
        }
    }
}
