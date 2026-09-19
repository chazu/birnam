//! Root discovery, marking, sweeping, and finalization for the Rust floor.

use core::cell::Cell;

use crate::abi::{MarkFn, Oop, RootProvider};
use crate::boundary;
#[cfg(test)]
use crate::heap::Heap;
use birnam_abi::storage::OK;
use birnam_abi::trace_live::SinkV1;

const MAX_STACK_SCAN_BYTES: usize = 128 << 20;

#[repr(align(16))]
struct RegisterSave([usize; 24]);

const _: () = {
    // Darwin arm64 setjmp.h defines jmp_buf as 48 ints: 192 bytes. Keep the
    // buffer exact and strengthen its natural alignment for word scanning.
    assert!(size_of::<RegisterSave>() == 192);
    assert!(align_of::<RegisterSave>() == 16);
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe extern "C" {
    fn _setjmp(buffer: *mut usize) -> core::ffi::c_int;
}

/// Flush callee-saved registers in a shallow outer frame, validate the
/// downward-growing native stack interval, and run collection below that frame.
/// Keeping the collector's own frames below `start` prevents stale mark and
/// sweep locals from becoming roots on the next collection.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn with_native_root_range<T>(
    stack_bottom: usize,
    operation: impl FnOnce(usize) -> T,
) -> Option<T> {
    if stack_bottom == 0 {
        return None;
    }
    let mut registers = RegisterSave([0; 24]);
    // SAFETY: RegisterSave provides more storage and at least the alignment
    // required by Darwin arm64's _setjmp implementation.
    unsafe { _setjmp(registers.0.as_mut_ptr()) };
    let start = core::ptr::addr_of!(registers).addr();
    let distance = stack_bottom.checked_sub(start)?;
    if distance > MAX_STACK_SCAN_BYTES {
        return None;
    }
    Some(operation(start))
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub(crate) fn with_native_root_range<T>(_: usize, _: impl FnOnce(usize) -> T) -> Option<T> {
    None
}

/// Emit native words in their existing low-to-high order. The tracer performs
/// immediate first-mark admission; its frames stay below the captured interval.
pub(crate) fn emit_native_root_range(sink: &SinkV1, start: usize, stack_bottom: usize) {
    let mut address = start.next_multiple_of(size_of::<usize>());
    while address
        .checked_add(size_of::<usize>())
        .is_some_and(|end| end <= stack_bottom)
    {
        // SAFETY: the caller supplies a validated live stack interval.
        emit_candidate(sink, unsafe { (address as *const usize).read() });
        address += size_of::<usize>();
    }
}

pub(crate) fn emit_candidate(sink: &SinkV1, candidate: usize) {
    // SAFETY: each floor adapter uses the sink only during its synchronous call.
    let status = unsafe { (sink.emit)(sink.context, candidate) };
    boundary::require_invariant(status == OK, "GC tracer rejected a candidate");
}

thread_local! {
    static ACTIVE_MARK_SINK: Cell<Option<SinkV1>> = const { Cell::new(None) };
}
struct ActiveMarkGuard;
impl Drop for ActiveMarkGuard {
    fn drop(&mut self) {
        ACTIVE_MARK_SINK.set(None);
    }
}
unsafe extern "C" fn mark_from_foreign(object: Oop) {
    let sink = ACTIVE_MARK_SINK
        .get()
        .unwrap_or_else(|| boundary::fatal_invariant("GC mark callback escaped its invocation"));
    if !object.is_null() {
        emit_candidate(&sink, object.addr());
    }
}

/// Adapt the legacy synchronous MarkFn without owning a queue or heap borrow.
pub(crate) fn invoke_with_mark_callback(sink: &SinkV1, operation: impl FnOnce(MarkFn)) {
    boundary::require_invariant(
        ACTIVE_MARK_SINK.get().is_none(),
        "GC mark callback reentered",
    );
    ACTIVE_MARK_SINK.set(Some(*sink));
    let _guard = ActiveMarkGuard;
    operation(mark_from_foreign);
}

/// Heap-only tests use the real upper traversal with a conservative source.
#[cfg(test)]
pub(crate) fn trace_conservative_for_test(heap: &mut Heap, roots: &[Oop]) {
    use birnam_abi::trace_live::{SourceV1, VERSION};
    use core::ffi::c_void;
    struct Source<'a> {
        heap: *mut Heap,
        roots: &'a [Oop],
    }
    unsafe extern "C" fn emit_roots(context: *mut c_void, sink: *const SinkV1) -> i32 {
        let source = unsafe { &*context.cast::<Source<'_>>() };
        for root in source.roots {
            emit_candidate(unsafe { &*sink }, root.addr());
        }
        OK
    }
    unsafe extern "C" fn admit(context: *mut c_void, address: usize) -> i32 {
        let source = unsafe { &*context.cast::<Source<'_>>() };
        i32::from(unsafe { (&mut *source.heap).mark_collectable_address(address) })
    }
    unsafe extern "C" fn edges(context: *mut c_void, address: usize, sink: *const SinkV1) -> i32 {
        let source = unsafe { &*context.cast::<Source<'_>>() };
        let words = unsafe { (&*source.heap).payload_words(address) }.unwrap();
        for index in 0..words.len() {
            emit_candidate(unsafe { &*sink }, unsafe { words.read(index) });
        }
        OK
    }
    let mut context = Source { heap, roots };
    let source = SourceV1 {
        version: VERSION,
        byte_size: size_of::<SourceV1>(),
        context: (&raw mut context).cast(),
        roots: Some(emit_roots),
        admit: Some(admit),
        edges: Some(edges),
    };
    assert_eq!(
        unsafe { birnam_trace::live::birnam_trace_live_v1(&source) },
        OK
    );
}

pub(crate) const MAX_ROOT_PROVIDERS: usize = 32;
pub(crate) const AUTO_COLLECTION_THRESHOLD: usize = 16 << 20;

pub(crate) struct MetadataShell {
    stack_bottom: usize,
    auto_requested: bool,
    precise: bool,
    root_providers: Vec<RootProvider>,
}

impl MetadataShell {
    pub(crate) fn new() -> Self {
        Self {
            stack_bottom: 0,
            auto_requested: true,
            precise: false,
            root_providers: Vec::new(),
        }
    }

    pub(crate) fn set_stack_bottom(&mut self, address: usize) {
        self.stack_bottom = address;
    }

    pub(crate) fn set_auto_requested(&mut self, enabled: bool) {
        self.auto_requested = enabled;
    }

    pub(crate) fn set_precise(&mut self, enabled: bool) {
        self.precise = enabled;
    }

    pub(crate) fn register_root_provider(&mut self, provider: RootProvider) {
        boundary::require_invariant(
            self.root_providers.len() < MAX_ROOT_PROVIDERS,
            "gc: too many root providers",
        );
        self.root_providers.push(provider);
    }

    pub(crate) fn root_providers(&self) -> Vec<RootProvider> {
        self.root_providers.clone()
    }

    pub(crate) fn stack_bottom(&self) -> usize {
        self.stack_bottom
    }

    #[cfg(test)]
    pub(crate) fn auto_requested(&self) -> bool {
        self.auto_requested
    }

    pub(crate) fn automatic_collection_due(&self, allocated_bytes: usize) -> bool {
        self.auto_requested
            && self.stack_bottom != 0
            && allocated_bytes >= AUTO_COLLECTION_THRESHOLD
    }

    pub(crate) fn precise(&self) -> bool {
        self.precise
    }

    #[cfg(test)]
    pub(crate) fn root_provider_count(&self) -> usize {
        self.root_providers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::MarkFn;
    use crate::heap::Permanence;

    unsafe extern "C" fn provider(_: MarkFn) {}

    #[test]
    fn shell_retains_settings_and_maximum_provider_mix() {
        let mut metadata = MetadataShell::new();
        assert!(metadata.auto_requested());
        assert!(!metadata.automatic_collection_due(AUTO_COLLECTION_THRESHOLD));
        metadata.set_stack_bottom(0x1234_5678);
        metadata.set_precise(true);
        for _ in 0..MAX_ROOT_PROVIDERS {
            metadata.register_root_provider(provider);
        }
        assert_eq!(metadata.stack_bottom(), 0x1234_5678);
        assert!(metadata.auto_requested());
        assert!(metadata.precise());
        assert_eq!(metadata.root_provider_count(), MAX_ROOT_PROVIDERS);
        assert!(!metadata.automatic_collection_due(AUTO_COLLECTION_THRESHOLD - 1));
        assert!(metadata.automatic_collection_due(AUTO_COLLECTION_THRESHOLD));
        metadata.set_auto_requested(false);
        assert!(!metadata.automatic_collection_due(usize::MAX));
    }

    fn write_word(object: crate::abi::Oop, index: usize, value: usize) {
        // SAFETY: each test allocates enough aligned payload words and keeps the
        // owning Heap live for the write and subsequent mark phase.
        unsafe { object.cast::<usize>().add(index).write(value) };
    }

    #[test]
    fn exact_address_marking_traces_cycles_and_rejects_false_candidates() {
        let mut heap = Heap::new();
        let permanent_vtable = heap.allocate(0, Permanence::Permanent);
        let first = heap.allocate(2 * size_of::<usize>(), Permanence::Collectable);
        let second = heap.allocate(size_of::<usize>(), Permanence::Collectable);
        let false_target = heap.allocate(0, Permanence::Collectable);
        assert!(heap.set_vtable(first, permanent_vtable));
        assert!(heap.set_vtable(second, permanent_vtable));
        assert!(heap.set_vtable(false_target, permanent_vtable));
        write_word(first, 0, second.addr());
        write_word(first, 1, false_target.addr() + 1);
        write_word(second, 0, first.addr());

        trace_conservative_for_test(&mut heap, &[first]);

        assert_eq!(heap.is_marked(first), Some(true));
        assert_eq!(heap.is_marked(second), Some(true));
        assert_eq!(heap.is_marked(false_target), Some(false));
        assert_eq!(heap.is_marked(permanent_vtable), Some(false));
    }

    #[test]
    fn permanent_payloads_do_not_keep_collectable_owners_alive() {
        let mut heap = Heap::new();
        let owner = heap.allocate(0, Permanence::Collectable);
        let permanent_carrier = heap.allocate(size_of::<usize>(), Permanence::Permanent);
        write_word(permanent_carrier, 0, owner.addr());

        trace_conservative_for_test(&mut heap, &[permanent_carrier]);

        assert_eq!(heap.is_marked(permanent_carrier), Some(false));
        assert_eq!(heap.is_marked(owner), Some(false));
        heap.clear_marks();
        assert_eq!(heap.is_marked(owner), Some(false));
    }
}
