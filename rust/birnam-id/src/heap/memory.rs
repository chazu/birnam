//! Upper allocation client. Production retains native Id operations, never a
//! Rust allocation registry. The local backend is for legacy isolated harnesses.
use crate::abi::Oop;
#[cfg(any(test, feature = "ffi-test-hooks"))]
use crate::{object_memory::{Allocation, ObjectMemory}, owned::Owned};
use super::PayloadWords;
use birnam_abi::{
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    id_primitives::PrimitivesV1,
};
use core::{ffi::c_void, ptr::NonNull};

#[derive(Clone, Copy)]
pub(super) struct View {
    pub base: NonNull<u8>,
    pub total_size: usize,
    identity: u64,
    requested: usize,
}
impl View {
    fn new(object: Oop, requested: usize, identity: u64) -> Self {
        Self {
            base: unsafe {
                NonNull::new_unchecked(object.cast::<u8>().sub(super::VTABLE_HEADER_SIZE))
            },
            total_size: super::Heap::allocation_total_size(requested).expect("invalid Id extent"),
            identity,
            requested,
        }
    }
    pub(super) fn payload(&self) -> NonNull<u8> {
        unsafe { NonNull::new_unchecked(self.base.as_ptr().add(super::VTABLE_HEADER_SIZE)) }
    }
    pub(super) fn identity(&self) -> u64 {
        self.identity
    }
    pub(super) fn requested_size(&self) -> usize {
        self.requested
    }
}
pub(super) enum Detached {
    #[cfg(any(test, feature = "ffi-test-hooks"))]
    Local(Allocation),
    Native { object: Oop, release: NativeEntry },
}
impl Detached {
    #[cfg(test)]
    pub(super) fn identity(&self) -> u64 {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(allocation) => allocation.identity(),
            Self::Native { .. } => panic!("test requires local detached allocation"),
        }
    }
    pub(super) fn object(&self) -> Oop {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(allocation) => allocation.payload().as_ptr().cast(),
            Self::Native { object, .. } => *object,
        }
    }
}
impl Drop for Detached {
    fn drop(&mut self) {
        let release = match self {
            Self::Native { release, .. } => release,
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(_) => return,
        };
        {
            loaded(unsafe {
                release.method.unwrap()(
                    release.context,
                    core::ptr::null_mut(),
                    core::ptr::null(),
                    0,
                )
            });
        }
    }
}
pub(super) enum Memory {
    #[cfg(any(test, feature = "ffi-test-hooks"))]
    Local(Owned<ObjectMemory>),
    Id(Client),
}
pub(super) struct Client {
    receiver: Oop,
    behavior: Oop,
    primitives: PrimitivesV1,
    at: NativeEntry,
    size: NativeEntry,
    identity: NativeEntry,
    allocations: NativeEntry,
    count: NativeEntry,
    behavior_of: NativeEntry,
    reassociate: NativeEntry,
    detach: NativeEntry,
}
impl Client {
    pub(super) fn new(id: Protocol, behavior: Oop, primitives: PrimitivesV1) -> Self {
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
            .expect("missing Id allocation operation")
        };
        Self {
            receiver: id.receiver,
            behavior,
            primitives,
            at: select(b"allocationAt:"),
            size: select(b"allocationSizeOf:"),
            identity: select(b"identityOf:"),
            allocations: select(b"allocationsDo:"),
            count: select(b"allocationCount"),
            behavior_of: select(b"behaviorOf:"),
            reassociate: select(b"reassociateBehaviorOf:with:"),
            detach: select(b"detachAllocationAt:"),
        }
    }
    fn invoke(&self, entry: NativeEntry, receiver: Oop, args: &[Operand]) -> Operand {
        loaded(unsafe { entry.method.unwrap()(entry.context, receiver, args.as_ptr(), args.len()) })
    }
    fn object(&self, address: usize) -> Option<Oop> {
        self.invoke(self.at, self.receiver, &[Operand::word(address)])
            .as_object()
    }
    fn view(&self, address: usize) -> Option<View> {
        let object = self.object(address)?;
        let args = [Operand::object(object)];
        Some(View::new(
            object,
            self.invoke(self.size, self.receiver, &args)
                .as_word()
                .expect("live allocation has no extent"),
            self.invoke(self.identity, self.receiver, &args)
                .as_word()
                .expect("live allocation has no identity") as u64,
        ))
    }
    fn views(&self) -> Vec<(usize, View)> {
        unsafe extern "C" fn visit(
            context: *mut c_void,
            _: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            let Some([object, size, identity]) =
                (unsafe { birnam_abi::borrowed_slice(args, count) })
            else {
                return Outcome::failed(native::INVALID_REQUEST, 0);
            };
            let (Some(object), Some(size), Some(identity)) =
                (object.as_object(), size.as_word(), identity.as_word())
            else {
                return Outcome::failed(native::INVALID_REQUEST, 0);
            };
            unsafe { &mut *context.cast::<Vec<(usize, View)>>() }
                .push((object.addr(), View::new(object, size, identity as u64)));
            Outcome::loaded(Operand::unit())
        }
        let mut views = Vec::new();
        self.invoke(
            self.allocations,
            self.receiver,
            &[Operand::entry(NativeEntry::new(
                visit,
                (&raw mut views).cast(),
            ))],
        );
        views
    }
}
fn loaded(outcome: Outcome) -> Operand {
    assert_eq!(
        outcome.status,
        native::LOADED,
        "Id allocation operation failed"
    );
    outcome.value
}
impl Memory {
    pub(super) fn allocate(&mut self, bytes: usize, behavior: Option<Oop>) -> Oop {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => {
                let object = memory.allocate(bytes);
                if let Some(behavior) = behavior {
                    assert!(memory.set_vtable(object, behavior));
                }
                object
            }
            Self::Id(client) => client
                .invoke(
                    client.primitives.allocate,
                    behavior.unwrap_or(client.behavior),
                    &[Operand::word(bytes)],
                )
                .as_object()
                .expect("allocation returned no object"),
        }
    }
    pub(super) fn intern(
        &mut self,
        _symbols: &mut crate::id_registries::Symbols,
        _behavior: Oop,
        bytes: &[u8],
    ) -> Result<Oop, crate::id_registries::EmbeddedNul> {
        if bytes.contains(&0) {
            return Err(crate::id_registries::EmbeddedNul);
        }
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => _symbols.local().intern(memory, _behavior, bytes).map_err(|_| crate::id_registries::EmbeddedNul),
            Self::Id(client) => Ok(client
                .invoke(
                    client.primitives.intern,
                    client.receiver,
                    &[Operand::bytes(bytes)],
                )
                .as_object()
                .expect("intern returned no Symbol")),
        }
    }
    pub(super) fn get(&self, address: usize) -> Option<View> {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.get(address).map(|entry| {
                View::new(
                    entry.payload().as_ptr().cast(),
                    entry.requested_size(),
                    entry.identity(),
                )
            }),
            Self::Id(client) => client.view(address),
        }
    }
    pub(super) fn iter(&self) -> std::vec::IntoIter<(usize, View)> {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory
                .iter()
                .map(|(address, entry)| {
                    (
                        address,
                        View::new(
                            entry.payload().as_ptr().cast(),
                            entry.requested_size(),
                            entry.identity(),
                        ),
                    )
                })
                .collect::<Vec<_>>(),
            Self::Id(client) => client.views(),
        }
        .into_iter()
    }
    pub(super) fn len(&self) -> usize {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.len(),
            Self::Id(client) => client
                .invoke(client.count, client.receiver, &[])
                .as_word()
                .unwrap(),
        }
    }
    pub(super) fn contains(&self, object: Oop) -> bool {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.get(object.addr()).is_some(),
            Self::Id(client) => client.object(object.addr()).is_some(),
        }
    }
    pub(super) fn payload_size(&self, object: Oop) -> Option<usize> {
        let size = match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.get(object.addr())?.requested_size(),
            Self::Id(client) => client
                .invoke(client.size, client.receiver, &[Operand::object(object)])
                .as_word()?,
        };
        size.checked_add(7).map(|size| size & !7)
    }
    pub(super) fn detach(&mut self, address: usize) -> Option<Detached> {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.detach(address).map(Detached::Local),
            Self::Id(client) => {
                let object = client.object(address)?;
                let release = client
                    .invoke(client.detach, client.receiver, &[Operand::word(address)])
                    .as_entry()?;
                Some(Detached::Native { object, release })
            }
        }
    }
    pub(super) fn vtable_of(&self, object: Oop) -> Option<Oop> {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.vtable_of(object),
            Self::Id(client) => {
                client.object(object.addr())?;
                Some(
                    client
                        .invoke(
                            client.behavior_of,
                            client.receiver,
                            &[Operand::object(object)],
                        )
                        .as_object()
                        .unwrap_or(core::ptr::null_mut()),
                )
            }
        }
    }
    pub(super) fn set_vtable(&mut self, object: Oop, behavior: Oop) -> bool {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.set_vtable(object, behavior),
            Self::Id(client) => {
                if client.object(object.addr()).is_none()
                    || client.object(behavior.addr()).is_none()
                {
                    return false;
                }
                client
                    .invoke(
                        client.reassociate,
                        client.receiver,
                        &[Operand::object(object), Operand::object(behavior)],
                    )
                    .as_object()
                    == Some(object)
            }
        }
    }
    pub(super) fn payload_words(&self, address: usize) -> Option<PayloadWords> {
        let object = match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.get(address)?.payload().as_ptr().cast(),
            Self::Id(client) => client.object(address)?,
        };
        Some(unsafe {
            PayloadWords::from_raw_parts(
                object.cast(),
                self.payload_size(object)? / size_of::<usize>(),
            )
        })
    }
    pub(super) fn read_payload_word(&self, object: Oop, index: usize) -> Option<usize> {
        let words = self.payload_words(object.addr())?;
        (index < words.len()).then(|| unsafe { words.read(index) })
    }
    pub(super) fn write_payload_word(&mut self, object: Oop, index: usize, value: usize) -> bool {
        let Some(view) = self.get(object.addr()) else {
            return false;
        };
        if index >= (view.total_size - super::VTABLE_HEADER_SIZE) / size_of::<usize>() {
            return false;
        }
        unsafe {
            view.payload()
                .as_ptr()
                .cast::<usize>()
                .add(index)
                .write(value)
        };
        true
    }
    #[cfg(test)]
    pub(super) fn geometry(&self, address: usize) -> u64 {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.get(address).unwrap().geometry,
            Self::Id(_) => panic!("packed geometry is private to Id"),
        }
    }
    #[cfg(test)]
    pub(super) fn local(&mut self) -> &mut ObjectMemory {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory,
            Self::Id(_) => panic!("local test requires local memory"),
        }
    }
    #[cfg(test)]
    pub(super) fn set_last_identity(&mut self, value: u64) {
        self.local().set_last_identity(value);
    }
    #[cfg(test)]
    pub(super) fn last_identity(&self) -> u64 {
        match self {
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            Self::Local(memory) => memory.last_identity(),
            Self::Id(client) => client
                .views()
                .iter()
                .map(|(_, view)| view.identity())
                .max()
                .unwrap_or(0),
        }
    }
}
