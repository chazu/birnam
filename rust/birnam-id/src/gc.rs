//! Collector metadata retained while R3 keeps marking and sweeping disabled.

use crate::abi::RootProvider;
use crate::boundary;
use crate::heap::Heap;

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
    pub(crate) fn drain_conservative(&mut self, heap: &mut Heap) {
        while let Some(address) = self.worklist.pop() {
            let words = heap
                .payload_words(address)
                .unwrap_or_else(|| boundary::fatal_invariant("marked GC block disappeared"));
            for candidate in words {
                self.mark_address(heap, candidate);
            }
        }
    }
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

    /// R3 retains all metadata but deliberately performs no mark or sweep.
    pub(crate) fn collect_disabled(&self) -> usize {
        0
    }

    #[cfg(test)]
    pub(crate) fn stack_bottom(&self) -> usize {
        self.stack_bottom
    }

    #[cfg(test)]
    pub(crate) fn auto_requested(&self) -> bool {
        self.auto_requested
    }

    #[cfg(test)]
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
