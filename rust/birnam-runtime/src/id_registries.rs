//! Upper clients of Id's native Symbol and dictionary protocol.
//! Only production Id owns these registries; local test backends are deleted.
use crate::abi::Oop;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EmbeddedNul;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AddMethod { Added, Overwrote(NativeEntry) }
use birnam_abi::id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol};
use core::ffi::{c_char, c_void};
use std::rc::Rc;
pub(crate) struct Client {
    id: Protocol,
    // Captured before upper modules load, not inferred from current bindings.
    primitives: birnam_abi::id_primitives::PrimitivesV1,
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
    pub(crate) fn new(id: Protocol, primitives: birnam_abi::id_primitives::PrimitivesV1) -> Rc<Self> {
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
            primitives,
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
    Id(Rc<Client>),
}
impl Symbols {
    pub(crate) fn existing(&self, bytes: &[u8]) -> Option<Oop> {
        match self {
            Self::Id(id) => id
                .call(id.symbol_named, &[Operand::bytes(bytes)])
                .as_object(),
        }
    }
    pub(crate) fn count(&self) -> usize {
        match self {
            Self::Id(id) => id.call(id.symbol_count, &[]).as_word().unwrap(),
        }
    }
    pub(crate) fn at(&self, index: usize) -> Oop {
        match self {
            Self::Id(id) => query_object(id.call(id.symbol_at, &[Operand::word(index)])),
        }
    }
    pub(crate) fn owned_name_bytes(&self) -> usize {
        match self {
            Self::Id(id) => id.call(id.symbol_name_bytes, &[]).as_word().unwrap(),
        }
    }
    pub(crate) fn contains_bytes(&self, bytes: &[u8]) -> bool {
        self.existing(bytes).is_some()
    }
    pub(crate) fn bytes_of(&self, symbol: Oop) -> Option<&[u8]> {
        match self {
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
}
pub(crate) enum Dictionaries {
    Id(Rc<Client>),
}
impl Dictionaries {
    pub(crate) fn client(&self) -> Option<Rc<Client>> {
        match self {
            Self::Id(id) => Some(id.clone()),
        }
    }
    pub(crate) fn encode_legacy(&self, method: crate::abi::Method, lookup: bool) -> NativeEntry {
        if let Some(primitives) = self.primitives()
            && method as usize == crate::legacy_method::lookup_method as crate::abi::Method as usize {
            return primitives.lookup;
        }
        crate::legacy_method::encode(method, lookup)
    }
    pub(crate) fn primitives(&self) -> Option<birnam_abi::id_primitives::PrimitivesV1> {
        match self { Self::Id(id) => Some(id.primitives) }
    }
    pub(crate) fn is_id_add_method(&self, entry: NativeEntry) -> bool {
        self.primitives().is_some_and(|primitives| entry.method.map(|f| f as usize)
            == primitives.add_method.method.map(|f| f as usize))
    }
    pub(crate) fn core_legacy_method(&self, entry: NativeEntry) -> Option<crate::raw::MethodAddress> {
        let primitives = self.primitives()?;
        // Identify the calling convention even for an entry from another Id;
        // invocation always preserves that entry's own dictionary context.
        (entry.method.map(|f| f as usize) == primitives.lookup.method.map(|f| f as usize))
            .then(|| crate::raw::MethodAddress::from_method(crate::legacy_method::lookup_method))
    }
    pub(crate) fn try_decode(&self, entry: NativeEntry) -> Option<crate::raw::MethodAddress> {
        self.core_legacy_method(entry).or_else(|| crate::legacy_method::try_decode(entry))
    }
    pub(crate) fn decode(&self, entry: NativeEntry) -> crate::raw::MethodAddress {
        self.try_decode(entry).unwrap_or_else(|| crate::boundary::fatal_invariant("typed native entry has no legacy Method adapter"))
    }
    pub(crate) fn is_default_lookup(&self, entry: Option<NativeEntry>) -> bool {
        entry == Some(self.encode_legacy(crate::legacy_method::lookup_method, true))
    }
    pub(crate) fn mutation_epoch(&self) -> u64 {
        match self {
            Self::Id(id) => id.call(id.epoch, &[]).as_word().unwrap() as u64,
        }
    }
    pub(crate) fn note_mutation(&mut self) {
        match self {
            Self::Id(id) => {
                let parent = id.call(id.parent, &[Operand::object(id.id.receiver)]);
                id.call(id.set_parent, &[Operand::object(id.id.receiver), parent]);
            }
        }
    }
    pub(crate) fn count(&self) -> usize {
        match self {
            Self::Id(id) => id.call(id.count, &[]).as_word().unwrap(),
        }
    }
    pub(crate) fn at(&self, index: usize) -> Oop {
        match self {
            Self::Id(id) => query_object(id.call(id.at, &[Operand::word(index)])),
        }
    }
    pub(crate) fn view(&self) -> birnam_abi::vtable_enumeration::ViewV1 {
        match self {
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
            Self::Id(id) => {
                id.call(id.membership, &[Operand::object(behavior)])
                    .as_word()
                    == Some(1)
            }
        }
    }
    pub(crate) fn parent_of(&self, behavior: Oop) -> Oop {
        match self {
            Self::Id(id) => query_object(id.call(id.parent, &[Operand::object(behavior)])),
        }
    }
    pub(crate) fn set_parent(&mut self, behavior: Oop, parent: Oop) {
        match self {
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
    pub(crate) fn remove_objects(&mut self, _dead: &[Oop]) {
        match self {
            Self::Id(_) => panic!("Id retires production dictionaries"),
        }
    }
    pub(crate) fn remove_weak_keys(&mut self, _dead: &[usize]) -> usize {
        match self {
            Self::Id(_) => panic!("Id retires production weak keys"),
        }
    }
}
