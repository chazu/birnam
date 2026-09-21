//! Upper clients of Id's native Symbol and dictionary protocol.
//! The local variants support isolated unit harnesses, not production ownership.
use crate::{
    abi::Oop,
    owned::Owned,
    symbol::Symbols as LocalSymbols,
    vtable::{AddMethod, Vtables},
};
use birnam_abi::id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol};
use core::ffi::{c_char, c_void};
use std::rc::Rc;
pub(crate) struct Client {
    id: Protocol,
    // Captured before Runtime installs its upper mutations. Lookup guards also
    // verify primitive identity, since an earlier module may replace lookup.
    initial_lookup: NativeEntry,
    primitive_add: NativeEntry,
    primitive_remove: NativeEntry,
    retire: NativeEntry,
    symbol_named: NativeEntry,
    symbol_count: NativeEntry,
    symbol_at: NativeEntry,
    symbol_name_bytes: NativeEntry,
    symbol_bytes: NativeEntry,
    epoch: NativeEntry,
    membership: NativeEntry,
    behaviors: NativeEntry,
    selectors: NativeEntry,
    binding: NativeEntry,
    lookup: NativeEntry,
    bindings: NativeEntry,
    add: NativeEntry,
    remove: NativeEntry,
    weak: NativeEntry,
    new_behavior: NativeEntry,
    bind: NativeEntry,
    count: NativeEntry,
    at: NativeEntry,
    parent: NativeEntry,
    set_parent: NativeEntry,
    references: NativeEntry,
}
impl Client {
    pub(crate) fn new(id: Protocol) -> Rc<Self> {
        let intern = |name| {
            loaded(unsafe {
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
            })
            .as_object()
            .unwrap()
        };
        let lookup = intern(b"lookup");
        let select = |name| {
            loaded(unsafe {
                id.message.unwrap()(
                    id.context,
                    id.receiver,
                    lookup,
                    &Operand::object(intern(name)),
                    1,
                )
            })
            .as_entry()
            .filter(|entry| entry.method.is_some())
            .expect("missing Id registry operation")
        };
        Rc::new(Self {
            id,
            initial_lookup: select(b"lookup"),
            primitive_add: select(b"addMethod"),
            primitive_remove: select(b"removeMethod:"),
            retire: select(b"retireIndexesFor:"),
            symbol_named: select(b"symbolNamed:"),
            symbol_count: select(b"symbolCount"),
            symbol_at: select(b"symbolAt:"),
            symbol_name_bytes: select(b"symbolNameBytes"),
            symbol_bytes: select(b"symbolBytesOf:"),
            epoch: select(b"behaviorEpoch"),
            membership: select(b"isBehavior:"),
            behaviors: select(b"behaviorView"),
            selectors: select(b"selectorsIn:"),
            binding: select(b"bindingOf:in:"),
            lookup: select(b"lookup:in:"),
            bindings: select(b"bindingsIn:do:"),
            add: select(b"addMethod:to:as:"),
            remove: select(b"removeMethod:from:"),
            weak: select(b"addWeakBinding:to:as:"),
            new_behavior: select(b"newBehaviorWithParent:"),
            bind: select(b"bind:to:"),
            count: select(b"behaviorCount"),
            at: select(b"behaviorAt:"),
            parent: select(b"parentOf:"),
            set_parent: select(b"parentOf:put:"),
            references: select(b"referencesOf:do:"),
        })
    }
    fn call(&self, entry: NativeEntry, args: &[Operand]) -> Operand {
        loaded(unsafe {
            entry.method.unwrap()(entry.context, self.id.receiver, args.as_ptr(), args.len())
        })
    }
    pub(crate) fn bind(
        &self,
        receiver: Oop,
        selector: Oop,
    ) -> Result<Option<NativeEntry>, Outcome> {
        let args = [Operand::object(selector), Operand::object(receiver)];
        let result = unsafe {
            self.bind.method.unwrap()(
                self.bind.context,
                self.id.receiver,
                args.as_ptr(),
                args.len(),
            )
        }
        .into_result()?;
        if result.kind() == native::UNIT {
            return Ok(None);
        }
        result
            .as_entry()
            .filter(|entry| entry.method.is_some())
            .map(Some)
            .ok_or_else(|| Outcome::failed(native::INVALID_REQUEST, 0))
    }
}
fn loaded(outcome: Outcome) -> Operand {
    assert_eq!(
        outcome.status,
        native::LOADED,
        "Id registry operation failed"
    );
    outcome.value
}
fn query_object(value: Operand) -> Oop {
    value.as_object().unwrap_or(core::ptr::null_mut())
}
fn query_view(value: Operand) -> birnam_abi::vtable_enumeration::ViewV1 {
    let bytes = value.as_bytes().expect("Id registry returned no view");
    assert_eq!(bytes.length % size_of::<Oop>(), 0);
    birnam_abi::vtable_enumeration::ViewV1 {
        entries: bytes.pointer.cast(),
        count: bytes.length / size_of::<Oop>(),
    }
}
pub(crate) enum Symbols {
    Local(Owned<LocalSymbols>),
    Id(Rc<Client>),
}
impl From<Owned<LocalSymbols>> for Symbols {
    fn from(value: Owned<LocalSymbols>) -> Self {
        Self::Local(value)
    }
}
impl Symbols {
    pub(crate) fn existing(&self, bytes: &[u8]) -> Option<Oop> {
        match self {
            Self::Local(local) => local.existing(bytes),
            Self::Id(id) => id
                .call(id.symbol_named, &[Operand::bytes(bytes)])
                .as_object(),
        }
    }
    pub(crate) fn count(&self) -> usize {
        match self {
            Self::Local(local) => local.count(),
            Self::Id(id) => id.call(id.symbol_count, &[]).as_word().unwrap(),
        }
    }
    pub(crate) fn at(&self, index: usize) -> Oop {
        match self {
            Self::Local(local) => local.at(index),
            Self::Id(id) => query_object(id.call(id.symbol_at, &[Operand::word(index)])),
        }
    }
    pub(crate) fn owned_name_bytes(&self) -> usize {
        match self {
            Self::Local(local) => local.owned_name_bytes(),
            Self::Id(id) => id.call(id.symbol_name_bytes, &[]).as_word().unwrap(),
        }
    }
    pub(crate) fn contains_bytes(&self, bytes: &[u8]) -> bool {
        self.existing(bytes).is_some()
    }
    pub(crate) fn bytes_of(&self, symbol: Oop) -> Option<&[u8]> {
        match self {
            Self::Local(local) => local.bytes_of(symbol),
            Self::Id(id) => {
                let bytes = id
                    .call(id.symbol_bytes, &[Operand::object(symbol)])
                    .as_bytes()?;
                unsafe { crate::abi::borrowed_bytes(bytes.pointer, bytes.length) }
            }
        }
    }
    pub(crate) fn c_name_of(&self, symbol: Oop) -> *const c_char {
        self.bytes_of(symbol)
            .map_or(core::ptr::null(), |bytes| bytes.as_ptr().cast())
    }
    pub(crate) fn remove_objects(&mut self, dead: &[usize]) -> usize {
        match self {
            Self::Local(local) => local.remove_objects(dead),
            Self::Id(id) => {
                let before = id.call(id.symbol_count, &[]).as_word().unwrap();
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        dead.as_ptr().cast::<u8>(),
                        core::mem::size_of_val(dead),
                    )
                };
                id.call(id.retire, &[Operand::bytes(bytes)]);
                before - id.call(id.symbol_count, &[]).as_word().unwrap()
            }
        }
    }
    pub(crate) fn local(&mut self) -> &mut LocalSymbols {
        match self {
            Self::Local(local) => local,
            Self::Id(_) => panic!("production Symbols belong to Id"),
        }
    }
}
pub(crate) enum Dictionaries {
    Local(Owned<Vtables>),
    Id(Rc<Client>),
}
impl From<Owned<Vtables>> for Dictionaries {
    fn from(value: Owned<Vtables>) -> Self {
        Self::Local(value)
    }
}
impl Dictionaries {
    pub(crate) fn client(&self) -> Option<Rc<Client>> {
        match self {
            Self::Local(_) => None,
            Self::Id(id) => Some(id.clone()),
        }
    }
    pub(crate) fn encode_legacy(&self, method: crate::abi::Method, lookup: bool) -> NativeEntry {
        if let Self::Id(id) = self {
            for (legacy, entry) in [
                (crate::legacy_method::lookup_method as crate::abi::Method, id.initial_lookup),
                (crate::legacy_method::add_method as crate::abi::Method, id.primitive_add),
                (crate::legacy_method::remove_method as crate::abi::Method, id.primitive_remove),
            ] {
                if method as usize == legacy as usize { return entry; }
            }
        }
        crate::legacy_method::encode(method, lookup)
    }
    pub(crate) fn is_default_lookup(&self, entry: Option<NativeEntry>) -> bool {
        let expected = self.encode_legacy(crate::legacy_method::lookup_method, true);
        entry == Some(expected)
            && match self {
                Self::Local(_) => true,
                Self::Id(_) => crate::legacy_method::is_id_lookup(expected),
            }
    }
    pub(crate) fn local_pointer(&self) -> Option<*mut Vtables> {
        match self {
            Self::Local(local) => Some(local.pointer()),
            Self::Id(_) => None,
        }
    }
    pub(crate) fn mutation_epoch(&self) -> u64 {
        match self {
            Self::Local(local) => local.mutation_epoch(),
            Self::Id(id) => id.call(id.epoch, &[]).as_word().unwrap() as u64,
        }
    }
    pub(crate) fn note_mutation(&mut self) {
        match self {
            Self::Local(local) => local.note_mutation(),
            Self::Id(id) => {
                let parent = id.call(id.parent, &[Operand::object(id.id.receiver)]);
                id.call(id.set_parent, &[Operand::object(id.id.receiver), parent]);
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn exhaust_mutation_epoch_for_test(&mut self) {
        match self {
            Self::Local(local) => local.exhaust_mutation_epoch_for_test(),
            Self::Id(id) => unsafe { crate::native_id::State::exhaust_epoch(id.id.context) },
        }
    }
    pub(crate) fn count(&self) -> usize {
        match self {
            Self::Local(local) => local.count(),
            Self::Id(id) => id.call(id.count, &[]).as_word().unwrap(),
        }
    }
    pub(crate) fn at(&self, index: usize) -> Oop {
        match self {
            Self::Local(local) => local.at(index),
            Self::Id(id) => query_object(id.call(id.at, &[Operand::word(index)])),
        }
    }
    pub(crate) fn view(&self) -> birnam_abi::vtable_enumeration::ViewV1 {
        match self {
            Self::Local(local) => local.view(),
            Self::Id(id) => query_view(id.call(id.behaviors, &[])),
        }
    }
    pub(crate) fn ordered(&self) -> Vec<Oop> {
        let view = self.view();
        unsafe { crate::abi::borrowed_slice(view.entries, view.count) }
            .unwrap()
            .to_vec()
    }
    pub(crate) fn selector_view(&self, behavior: Oop) -> birnam_abi::vtable_enumeration::ViewV1 {
        match self {
            Self::Local(local) => local.selector_view(behavior),
            Self::Id(id) => query_view(id.call(id.selectors, &[Operand::object(behavior)])),
        }
    }
    pub(crate) fn selector_at(&self, behavior: Oop, index: usize) -> Oop {
        let view = self.selector_view(behavior);
        if index >= view.count {
            core::ptr::null_mut()
        } else {
            unsafe { view.entries.add(index).read() }
        }
    }
    pub(crate) fn is_registered(&self, behavior: Oop) -> bool {
        match self {
            Self::Local(local) => local.is_registered(behavior),
            Self::Id(id) => {
                id.call(id.membership, &[Operand::object(behavior)])
                    .as_word()
                    == Some(1)
            }
        }
    }
    pub(crate) fn parent_of(&self, behavior: Oop) -> Oop {
        match self {
            Self::Local(local) => local.parent_of(behavior),
            Self::Id(id) => query_object(id.call(id.parent, &[Operand::object(behavior)])),
        }
    }
    pub(crate) fn set_parent(&mut self, behavior: Oop, parent: Oop) {
        match self {
            Self::Local(local) => local.set_parent(behavior, parent),
            Self::Id(id) => {
                id.call(
                    id.set_parent,
                    &[Operand::object(behavior), Operand::object(parent)],
                );
            }
        }
    }
    pub(crate) fn direct_lookup(&self, behavior: Oop, key: Oop) -> Option<NativeEntry> {
        match self {
            Self::Local(local) => local.direct_lookup(behavior, key),
            Self::Id(id) => id
                .call(
                    id.binding,
                    &[Operand::object(key), Operand::object(behavior)],
                )
                .as_entry(),
        }
    }
    pub(crate) fn lookup(&self, behavior: Oop, key: Oop) -> Option<NativeEntry> {
        match self {
            Self::Local(local) => local.lookup(behavior, key),
            Self::Id(id) => id
                .call(
                    id.lookup,
                    &[Operand::object(key), Operand::object(behavior)],
                )
                .as_entry(),
        }
    }
    pub(crate) fn lookup_head(&self, behavior: Oop, key: Oop) -> Result<Option<NativeEntry>, Oop> {
        match self {
            Self::Local(local) => local.lookup_head(behavior, key),
            Self::Id(id) => {
                let value = id.call(
                    id.binding,
                    &[Operand::object(key), Operand::object(behavior)],
                );
                if value.kind() == native::UNIT {
                    Err(self.parent_of(behavior))
                } else {
                    Ok(value.as_entry())
                }
            }
        }
    }
    pub(crate) fn add_method(&mut self, behavior: Oop, key: Oop, method: NativeEntry) -> AddMethod {
        match self {
            Self::Local(local) => local.add_method(behavior, key, method),
            Self::Id(id) => id
                .call(
                    id.add,
                    &[
                        Operand::object(key),
                        Operand::word(behavior.addr()),
                        Operand::entry(method),
                    ],
                )
                .as_entry()
                .map_or(AddMethod::Added, AddMethod::Overwrote),
        }
    }
    pub(crate) fn remove_method(&mut self, behavior: Oop, key: Oop) -> Option<NativeEntry> {
        match self {
            Self::Local(local) => local.remove_method(behavior, key),
            Self::Id(id) => id
                .call(
                    id.remove,
                    &[Operand::object(key), Operand::word(behavior.addr())],
                )
                .as_entry(),
        }
    }
    pub(crate) fn add_weak_opaque_binding(&mut self, behavior: Oop, key: Oop, value: Oop) {
        match self {
            Self::Local(local) => local.add_weak_opaque_binding(behavior, key, value),
            Self::Id(id) => {
                id.call(
                    id.weak,
                    &[
                        Operand::object(key),
                        Operand::word(behavior.addr()),
                        Operand::object(value),
                    ],
                );
            }
        }
    }
    pub(crate) fn create(
        &mut self,
        heap: &mut crate::heap::Heap,
        parent: Oop,
        meta: Oop,
        permanence: crate::heap::Permanence,
    ) -> Oop {
        match self {
            Self::Local(local) => {
                let value = heap.allocate_with_behavior(Vtables::PAYLOAD_SIZE, permanence, meta);
                unsafe { local.register(value, parent) };
                value
            }
            Self::Id(id) => {
                assert_eq!(meta, id.id.receiver);
                let value = id
                    .call(id.new_behavior, &[Operand::word(parent.addr())])
                    .as_object()
                    .unwrap();
                heap.track_allocation(value, permanence);
                value
            }
        }
    }
    pub(crate) fn methods(&self, behavior: Oop) -> std::vec::IntoIter<NativeEntry> {
        let entries = match self {
            Self::Local(local) => local.methods(behavior).collect(),
            Self::Id(id) => {
                unsafe extern "C" fn visit(
                    context: *mut c_void,
                    _: Oop,
                    args: *const Operand,
                    count: usize,
                ) -> Outcome {
                    let Some([_, value, _]) = (unsafe { crate::abi::borrowed_slice(args, count) })
                    else {
                        return Outcome::failed(native::INVALID_REQUEST, 0);
                    };
                    if let Some(entry) = value.as_entry() {
                        unsafe { &mut *context.cast::<Vec<NativeEntry>>() }.push(entry);
                    }
                    Outcome::loaded(Operand::unit())
                }
                let mut entries = Vec::new();
                id.call(
                    id.bindings,
                    &[
                        Operand::object(behavior),
                        Operand::entry(NativeEntry::new(visit, (&raw mut entries).cast())),
                    ],
                );
                entries
            }
        };
        entries.into_iter()
    }
    pub(crate) fn strong_edges(&self, behavior: Oop) -> Vec<Oop> {
        match self {
            Self::Local(local) => local.strong_edges(behavior),
            Self::Id(id) => {
                unsafe extern "C" fn visit(
                    context: *mut c_void,
                    _: Oop,
                    args: *const Operand,
                    count: usize,
                ) -> Outcome {
                    let Some([object]) = (unsafe { crate::abi::borrowed_slice(args, count) })
                    else {
                        return Outcome::failed(native::INVALID_REQUEST, 0);
                    };
                    unsafe { &mut *context.cast::<Vec<Oop>>() }.push(object.as_object().unwrap());
                    Outcome::loaded(Operand::unit())
                }
                let mut edges = Vec::new();
                id.call(
                    id.references,
                    &[
                        Operand::object(behavior),
                        Operand::entry(NativeEntry::new(visit, (&raw mut edges).cast())),
                    ],
                );
                edges
                    .into_iter()
                    .skip(1)
                    .filter(|value: &Oop| !value.is_null())
                    .collect()
            }
        }
    }
    pub(crate) fn remove_objects(&mut self, dead: &[Oop]) {
        match self {
            Self::Local(local) => local.remove_objects(dead),
            Self::Id(_) => panic!("Id retires production dictionaries"),
        }
    }
    pub(crate) fn remove_weak_keys(&mut self, dead: &[usize]) -> usize {
        match self {
            Self::Local(local) => local.remove_weak_keys(dead),
            Self::Id(_) => panic!("Id retires production weak keys"),
        }
    }
    #[cfg(test)]
    pub(crate) fn record(&self, behavior: Oop) -> &crate::vtable::VtableRecord {
        match self {
            Self::Local(local) => local.record(behavior),
            Self::Id(_) => panic!("local dictionary test"),
        }
    }
    #[cfg(test)]
    pub(crate) fn record_mut(&mut self, behavior: Oop) -> &mut crate::vtable::VtableRecord {
        match self {
            Self::Local(local) => local.record_mut(behavior),
            Self::Id(_) => panic!("local dictionary test"),
        }
    }
    #[cfg(test)]
    pub(crate) fn dictionary_capacity(&self, behavior: Oop) -> usize {
        match self {
            Self::Local(local) => local.dictionary_capacity(behavior),
            Self::Id(_) => panic!("local dictionary test"),
        }
    }
}
