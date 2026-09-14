use std::sync::Once;

static INSTALLED: Once = Once::new();

pub(super) fn install() {
    INSTALLED.call_once(|| unsafe {
        unsafe extern "C" {
            fn birnam_tcc_install_allocation_metrics();
        }
        // Every raw state constructor passes here before allocating any state.
        // The hook remains installed for this linked compiler instance's life.
        birnam_tcc_install_allocation_metrics();
    });
}

/// Requested live heap storage in this compiler instance, including transient
/// compiler allocations. Excludes accounting headers and allocator rounding.
/// Relocation storage may overlap this total; do not add the two measurements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapAllocationTotals {
    pub blocks: usize,
    pub requested_bytes: usize,
}

/// Read at a quiescent compiler boundary for a coherent pair. Atomics prevent
/// data races, but two fields read during mutation need not describe one instant.
pub fn heap_allocation_totals() -> HeapAllocationTotals {
    install();
    let mut result = HeapAllocationTotals {
        blocks: 0,
        requested_bytes: 0,
    };
    unsafe extern "C" {
        fn birnam_tcc_heap_allocation_totals(blocks: *mut usize, bytes: *mut usize);
    }
    unsafe { birnam_tcc_heap_allocation_totals(&mut result.blocks, &mut result.requested_bytes) };
    result
}
