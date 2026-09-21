//! Upper collector client of Id's message protocol; no Id registry access.
use crate::abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use crate::boundary;
use core::ffi::c_void;

#[derive(Clone, Copy)]
pub(super) struct Storage {
    receiver: Oop,
    roots: NativeEntry,
    allocations: NativeEntry,
    references: NativeEntry,
}

pub(super) struct References {
    pub edges: Vec<Oop>,
    pub kind: usize,
}

impl Storage {
    pub(super) fn new(id: Protocol) -> Self {
        let intern = |name: &[u8]| {
            let answer = unsafe {
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
            };
            loaded(answer)
                .as_object()
                .expect("Id storage selector is not an Object")
        };
        let lookup = intern(b"lookup");
        let bind = |name| {
            let selector = intern(name);
            let answer = loaded(unsafe {
                id.message.unwrap()(
                    id.context,
                    id.receiver,
                    lookup,
                    &Operand::object(selector),
                    1,
                )
            });
            answer
                .as_entry()
                .filter(|entry| entry.method.is_some())
                .expect("Id storage operation is not a native entry")
        };
        Self {
            receiver: id.receiver,
            roots: bind(b"rootsDo:"),
            allocations: bind(b"allocationsDo:"),
            references: bind(b"referencesOf:do:"),
        }
    }

    // Selected during collector construction, while these are pinned native
    // implementations. Marking invokes those exact callables, without another
    // lookup through possibly allocating application code.
    fn invoke(self, entry: NativeEntry, args: &[Operand]) -> Operand {
        loaded(unsafe {
            entry.method.unwrap()(entry.context, self.receiver, args.as_ptr(), args.len())
        })
    }

    pub(super) fn roots(self) -> Vec<Oop> {
        let mut roots = Vec::new();
        self.invoke(
            self.roots,
            &[Operand::entry(NativeEntry::new(
                object,
                (&raw mut roots).cast(),
            ))],
        );
        roots
    }

    pub(super) fn allocations(self) -> Vec<(Oop, usize, usize)> {
        let mut allocations = Vec::new();
        self.invoke(
            self.allocations,
            &[Operand::entry(NativeEntry::new(
                allocation,
                (&raw mut allocations).cast(),
            ))],
        );
        allocations
    }

    pub(super) fn references(self, candidate: Oop) -> Option<References> {
        let mut edges = Vec::new();
        let kind = self.invoke(
            self.references,
            &[
                Operand::object(candidate),
                Operand::entry(NativeEntry::new(object, (&raw mut edges).cast())),
            ],
        );
        if kind.kind() == native::UNIT {
            return None;
        }
        let kind = kind.as_word().expect("Id returned invalid storage kind");
        boundary::require_invariant(
            kind <= 2 && edges.len() >= if kind == 1 { 2 } else { 1 },
            "Id returned invalid reference facts",
        );
        Some(References { edges, kind })
    }
}

fn loaded(answer: Outcome) -> Operand {
    boundary::require_invariant(
        answer.status == native::LOADED,
        "Id storage query failed before sweep",
    );
    answer.value
}
unsafe extern "C" fn object(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(object) = argument.as_object() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    unsafe { &mut *context.cast::<Vec<Oop>>() }.push(object);
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn allocation(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([object, size, identity]) = (unsafe { crate::abi::borrowed_slice(args, count) })
    else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let (Some(object), Some(size), Some(identity)) =
        (object.as_object(), size.as_word(), identity.as_word())
    else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    unsafe { &mut *context.cast::<Vec<(Oop, usize, usize)>>() }.push((object, size, identity));
    Outcome::loaded(Operand::unit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    struct Owner {
        original: NativeEntry,
        extra: Cell<Oop>,
        calls: Cell<usize>,
    }
    unsafe extern "C" fn roots(
        context: *mut c_void,
        receiver: Oop,
        args: *const Operand,
        count: usize,
    ) -> Outcome {
        let owner = unsafe { &*context.cast::<Owner>() };
        owner.calls.set(owner.calls.get() + 1);
        let answer = unsafe {
            owner.original.method.unwrap()(owner.original.context, receiver, args, count)
        };
        if answer.status != native::LOADED || owner.extra.get().is_null() {
            return answer;
        }
        let visitor = unsafe { args.read() }.as_entry().unwrap();
        unsafe {
            visitor.method.unwrap()(
                visitor.context,
                receiver,
                &Operand::object(owner.extra.get()),
                1,
            )
        }
    }

    #[test]
    fn collector_acquires_id_callables_before_application_lookup_and_uses_their_roots() {
        let initial = crate::initial_objects::InitialObjects::new();
        let id = initial.protocol.protocol;
        let intern = |name: &[u8]| {
            loaded(unsafe {
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
            })
            .as_object()
            .unwrap()
        };
        let send = |receiver, selector, args: &[Operand]| {
            loaded(unsafe {
                id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len())
            })
        };
        let roots_selector = intern(b"rootsDo:");
        let add = intern(b"addMethod");
        let lookup = intern(b"lookup");
        let original = send(id.receiver, lookup, &[Operand::object(roots_selector)])
            .as_entry()
            .unwrap();
        let extra = send(initial.object_vt, intern(b"allocate"), &[Operand::word(0)])
            .as_object()
            .unwrap();
        let mut owner = Owner {
            original,
            extra: Cell::new(extra),
            calls: Cell::new(0),
        };
        send(
            id.receiver,
            add,
            &[
                Operand::object(roots_selector),
                Operand::entry(NativeEntry::new(roots, (&raw mut owner).cast())),
            ],
        );
        // Construction selects the root callable above through actual Id lookup.
        let bootstrap = Box::new(crate::dispatch::build_bootstrap_from(initial));
        let runtime = &bootstrap.runtime;
        runtime.install_id_runtime_extensions();
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        runtime.enable_symbol_compaction_for_test();
        // A subsequent send/consumer selects the replacement; the constructed
        // collector retains the exact earlier callable and its live native owner.
        send(
            id.receiver,
            add,
            &[Operand::object(roots_selector), Operand::entry(original)],
        );
        assert!(!Storage::new(id).roots().contains(&extra));
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert!(owner.calls.get() > 0);
        assert!(!runtime.checked_vtable_of(extra).is_null());
        owner.extra.set(core::ptr::null_mut());
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert!(runtime.checked_vtable_of(extra).is_null());
    }
}
