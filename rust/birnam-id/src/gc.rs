//! Root discovery, marking, sweeping, and finalization for the Rust floor.

use crate::abi::MarkFn;
#[cfg(test)]
use crate::abi::Oop;
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
    #[cfg(test)]
    let factory = unsafe { *birnam_native_stack::birnam_native_stack_factory_v2() };
    #[cfg(not(test))]
    let factory = unsafe { birnam_abi::native_stack::operations() };
    unsafe { factory.scan.unwrap()(sink, start, stack_bottom) };
}

pub(crate) fn emit_candidate(sink: &SinkV1, candidate: usize) {
    // SAFETY: each floor adapter uses the sink only during its synchronous call.
    let status = unsafe { (sink.emit)(sink.context, candidate) };
    boundary::require_invariant(status == OK, "GC tracer rejected a candidate");
}

// The TLS key remains local to each loaded floor; the module owns the state.
struct NativeMarkOwner {
    context: core::ptr::NonNull<core::ffi::c_void>,
    api: birnam_abi::native_mark::ApiV1,
}
impl NativeMarkOwner {
    fn new() -> Self {
        #[cfg(test)]
        let api = unsafe { *birnam_native_mark::birnam_native_mark_provider_v1() };
        #[cfg(not(test))]
        let api = unsafe { birnam_abi::native_mark::operations() };
        let context = core::ptr::NonNull::new(unsafe { api.create.unwrap()() })
            .expect("native mark scope creation failed");
        Self { context, api }
    }
}
impl Drop for NativeMarkOwner {
    fn drop(&mut self) { unsafe { self.api.destroy.unwrap()(self.context.as_ptr()) }; }
}
thread_local! {
    static NATIVE_MARK: NativeMarkOwner = NativeMarkOwner::new();
}
struct ActiveMarkGuard {
    context: *mut core::ffi::c_void,
    end: unsafe extern "C" fn(*mut core::ffi::c_void),
}
impl Drop for ActiveMarkGuard {
    fn drop(&mut self) { unsafe { (self.end)(self.context) }; }
}
unsafe extern "C" fn mark_from_foreign(object: crate::abi::Oop) {
    // A callback has no context argument. This floor-local shim selects only
    // its own thread's module-owned scope, even with multiple loaded floors.
    let (context, mark) = NATIVE_MARK.with(|owner| (owner.context.as_ptr(), owner.api.mark.unwrap()));
    unsafe { mark(context, object) };
}

/// The module owns scope state; retain the floor's callback identity and unwind.
pub(crate) fn invoke_with_mark_callback(sink: &SinkV1, operation: impl FnOnce(MarkFn)) {
    let (context, api) = NATIVE_MARK.with(|owner| (owner.context.as_ptr(), owner.api));
    unsafe { api.begin.unwrap()(context, sink) };
    let _guard = ActiveMarkGuard { context, end: api.end.unwrap() };
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

#[cfg(any(test, feature = "ffi-test-hooks"))]
pub(crate) use birnam_abi::gc_settings::AUTO_COLLECTION_THRESHOLD;

pub(crate) struct MetadataShell {
    native_stack: crate::native_stack::Service,
    settings: crate::gc_settings::Service,
}

impl MetadataShell {
    pub(crate) fn new() -> Self {
        Self {
            native_stack: crate::native_stack::Service::new(),
            settings: crate::gc_settings::Service::new(),
        }
    }

    pub(crate) fn set_stack_bottom(&mut self, address: usize) {
        self.native_stack.set(address);
    }

    pub(crate) fn set_auto_requested(&mut self, enabled: bool) {
        self.settings.set_auto(enabled);
    }

    pub(crate) fn set_precise(&mut self, enabled: bool) {
        self.settings.set_precise(enabled);
    }

    pub(crate) fn stack_bottom(&self) -> usize {
        self.native_stack.get()
    }

    #[cfg(test)]
    pub(crate) fn auto_requested(&self) -> bool {
        self.settings.automatic_due(true, AUTO_COLLECTION_THRESHOLD)
    }

    pub(crate) fn automatic_collection_due(&self, allocated_bytes: usize) -> bool {
        self.settings.automatic_due(self.native_stack.get() != 0, allocated_bytes)
    }

    pub(crate) fn precise(&self) -> bool {
        self.settings.precise()
    }

    pub(crate) fn native_stack(&self) -> *mut birnam_abi::native_stack::ApiV1 {
        self.native_stack.api()
    }

    pub(crate) fn settings(&self) -> *mut birnam_abi::gc_settings::ApiV1 {
        self.settings.api()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::Permanence;

    #[test]
    fn shell_retains_collection_settings() {
        let mut metadata = MetadataShell::new();
        assert!(metadata.auto_requested());
        assert!(!metadata.automatic_collection_due(AUTO_COLLECTION_THRESHOLD));
        metadata.set_stack_bottom(0x1234_5678);
        metadata.set_precise(true);
        assert_eq!(metadata.stack_bottom(), 0x1234_5678);
        assert!(metadata.auto_requested());
        assert!(metadata.precise());
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

#[cfg(test)]
mod native_mark_tests {
    use super::*;
    #[test]
    fn floor_callback_selects_thread_local_scopes_and_later_sinks() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (7..9).map(|value| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                unsafe extern "C" fn emit(context: *mut core::ffi::c_void, value: usize) -> i32 {
                    unsafe { &mut *context.cast::<Vec<usize>>() }.push(value); OK
                }
                let mut values = Vec::<usize>::new();
                let sink = SinkV1 { context: (&raw mut values).cast(), emit };
                let mut saved = None;
                invoke_with_mark_callback(&sink, |mark| {
                    saved = Some(mark); barrier.wait(); unsafe { mark(value as Oop) }; barrier.wait();
                });
                invoke_with_mark_callback(&sink, |_| unsafe { saved.unwrap()((value + 10) as Oop) });
                assert_eq!(values, [value, value + 10]);
            })
        }).collect();
        for handle in handles { handle.join().unwrap(); }
    }
    #[test]
    fn scope_resets_after_rust_closure_unwind() {
        unsafe extern "C" fn emit(context: *mut core::ffi::c_void, value: usize) -> i32 {
            unsafe { &mut *context.cast::<Vec<usize>>() }.push(value);
            OK
        }
        let mut values = Vec::<usize>::new();
        let sink = SinkV1 { context: (&raw mut values).cast(), emit };
        let result = std::panic::catch_unwind(|| {
            invoke_with_mark_callback(&sink, |mark| {
                unsafe { mark(17usize as Oop) };
                panic!("ordinary Rust closure unwind");
            });
        });
        assert!(result.is_err());
        invoke_with_mark_callback(&sink, |mark| unsafe { mark(19usize as Oop) });
        assert_eq!(values, [17, 19]);
    }
}
