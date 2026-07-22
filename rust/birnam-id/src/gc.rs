//! Collector metadata retained while R3 keeps marking and sweeping disabled.

use core::cell::Cell;

use crate::abi::{MarkFn, Oop, RootProvider};
use crate::boundary;
use crate::heap::Heap;

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

/// Flush callee-saved registers and conservatively mark the downward-growing
/// native stack up to the host-provided outer anchor. Returns false rather than
/// scanning when the anchor is absent, reversed, or implausibly distant.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn mark_native_roots(heap: &mut Heap, marker: &mut Marker, stack_bottom: usize) -> bool {
    if stack_bottom == 0 {
        return false;
    }
    let mut registers = RegisterSave([0; 24]);
    // SAFETY: RegisterSave provides more storage and at least the alignment
    // required by Darwin arm64's _setjmp implementation.
    unsafe { _setjmp(registers.0.as_mut_ptr()) };
    let start = core::ptr::addr_of!(registers).addr();
    let Some(distance) = stack_bottom.checked_sub(start) else {
        return false;
    };
    if distance > MAX_STACK_SCAN_BYTES {
        return false;
    }
    let mut address = start.next_multiple_of(size_of::<usize>());
    while address
        .checked_add(size_of::<usize>())
        .is_some_and(|end| end <= stack_bottom)
    {
        // SAFETY: a validated downward-growing stack interval from this live
        // frame to the caller-supplied outer anchor remains mapped for the
        // synchronous collection call. Reads are aligned machine words.
        let candidate = unsafe { (address as *const usize).read() };
        marker.mark_address(heap, candidate);
        address += size_of::<usize>();
    }
    true
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub(crate) fn mark_native_roots(_: &mut Heap, _: &mut Marker, _: usize) -> bool {
    false
}

/// One non-reentrant mark phase. Candidate membership is exact: only the
/// registered payload start of a collectable block can enter the worklist.
pub(crate) struct Marker {
    worklist: Vec<usize>,
}

impl Marker {
    pub(crate) fn new() -> Self {
        Self {
            worklist: Vec::new(),
        }
    }

    pub(crate) fn mark_address(&mut self, heap: &mut Heap, address: usize) {
        if heap.mark_collectable_address(address) {
            self.worklist.push(address);
        }
    }

    pub(crate) fn mark_oop(&mut self, heap: &mut Heap, object: crate::abi::Oop) {
        if !object.is_null() {
            self.mark_address(heap, object.addr());
        }
    }

    /// Drain the transitive closure by conservatively interpreting each padded
    /// payload word as an exact-address candidate. Headers and permanent-object
    /// payloads are never scanned.
    #[cfg(test)]
    pub(crate) fn drain_conservative(&mut self, heap: &mut Heap) {
        while let Some(address) = self.worklist.pop() {
            self.trace_conservative_payload(heap, address);
        }
    }

    pub(crate) fn pop(&mut self) -> Option<usize> {
        self.worklist.pop()
    }

    pub(crate) fn trace_conservative_payload(&mut self, heap: &mut Heap, address: usize) {
        let words = heap
            .payload_words(address)
            .unwrap_or_else(|| boundary::fatal_invariant("marked GC block disappeared"));
        for candidate in words {
            self.mark_address(heap, candidate);
        }
    }

    pub(crate) fn trace_selected_payload(
        &mut self,
        heap: &mut Heap,
        address: usize,
        mut traces_word: impl FnMut(usize) -> bool,
    ) {
        let words = heap
            .payload_words(address)
            .unwrap_or_else(|| boundary::fatal_invariant("marked GC block disappeared"));
        for (index, candidate) in words.into_iter().enumerate() {
            if traces_word(index) {
                self.mark_address(heap, candidate);
            }
        }
    }
}

struct ActiveMarkContext {
    heap: *mut Heap,
    marker: *mut Marker,
}

thread_local! {
    static ACTIVE_MARK_CONTEXT: Cell<*mut ActiveMarkContext> = const { Cell::new(core::ptr::null_mut()) };
}

struct ActiveMarkGuard;

impl Drop for ActiveMarkGuard {
    fn drop(&mut self) {
        ACTIVE_MARK_CONTEXT.set(core::ptr::null_mut());
    }
}

unsafe extern "C" fn mark_from_foreign(object: Oop) {
    ACTIVE_MARK_CONTEXT.with(|active| {
        let context = active.get();
        boundary::require_invariant(
            !context.is_null(),
            "GC mark callback escaped its invocation",
        );
        // SAFETY: invoke_with_mark_callback installs live, uniquely used raw
        // pointers for exactly the synchronous foreign callback extent.
        let context = unsafe { &mut *context };
        // SAFETY: the dynamic guard guarantees both owners remain live and no
        // Rust references to them overlap this reentrant mark operation.
        unsafe { (&mut *context.marker).mark_oop(&mut *context.heap, object) };
    });
}

/// Invoke one foreign provider or trace hook with a dynamically scoped mark
/// callback. The callback must be used synchronously and must not be retained.
///
/// # Safety
///
/// `heap` and `marker` must remain live and exclusively available until
/// `operation` returns. No Rust reference to either may overlap the call.
pub(crate) unsafe fn invoke_with_mark_callback(
    heap: *mut Heap,
    marker: *mut Marker,
    operation: impl FnOnce(MarkFn),
) {
    let mut context = ActiveMarkContext { heap, marker };
    ACTIVE_MARK_CONTEXT.with(|active| {
        boundary::require_invariant(active.get().is_null(), "GC mark callback reentered");
        active.set(core::ptr::from_mut(&mut context));
    });
    let guard = ActiveMarkGuard;
    operation(mark_from_foreign);
    drop(guard);
}

pub(crate) const MAX_ROOT_PROVIDERS: usize = 32;

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
            auto_requested: false,
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

    /// R3 retains all metadata but deliberately performs no mark or sweep.
    pub(crate) fn collect_disabled(&self) -> usize {
        0
    }

    pub(crate) fn stack_bottom(&self) -> usize {
        self.stack_bottom
    }

    #[cfg(test)]
    pub(crate) fn auto_requested(&self) -> bool {
        self.auto_requested
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
    fn shell_retains_settings_and_maximum_provider_mix_without_collecting() {
        let mut metadata = MetadataShell::new();
        metadata.set_stack_bottom(0x1234_5678);
        metadata.set_auto_requested(true);
        metadata.set_precise(true);
        for _ in 0..MAX_ROOT_PROVIDERS {
            metadata.register_root_provider(provider);
        }
        assert_eq!(metadata.stack_bottom(), 0x1234_5678);
        assert!(metadata.auto_requested());
        assert!(metadata.precise());
        assert_eq!(metadata.root_provider_count(), MAX_ROOT_PROVIDERS);
        assert_eq!(metadata.collect_disabled(), 0);
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

        let mut marker = Marker::new();
        marker.mark_oop(&mut heap, first);
        marker.drain_conservative(&mut heap);

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

        let mut marker = Marker::new();
        marker.mark_oop(&mut heap, permanent_carrier);
        marker.drain_conservative(&mut heap);

        assert_eq!(heap.is_marked(permanent_carrier), Some(false));
        assert_eq!(heap.is_marked(owner), Some(false));
        heap.clear_marks();
        assert_eq!(heap.is_marked(owner), Some(false));
    }
}
