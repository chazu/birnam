//! Continuation / stack reification (Phase 11.6 reify-stack API).
//!
//! Materializes the live Vm call stack as fact-entities — an L3+ concern (it
//! *writes* facts), kept out of `dispatch.zig` so the dispatch layer (L2) is
//! free of fact-store writes (W6 cut #4, ROADMAP D5.2 / `docs/20a`). Core
//! dispatch (`runMethod`) never writes facts; reification is an explicit,
//! on-demand path (debug / non-local-return introspection), not part of any
//! per-call flow. It reads the Vm (L2) and writes the store (L3); a bridge like
//! this belongs above both, not inside dispatch.

const std = @import("std");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");
const vm_mod = @import("vm.zig");

const FactStore = @import("fact_store.zig").FactStore;
const ObjectTable = @import("object_table.zig").ObjectTable;

/// Materialize the current Vm frame stack as transient fact-entities.
/// Returns the EIDs of the asserted frame entities, deepest-frame-first
/// (caller frame at index 0, current/innermost at end). Caller owns the
/// returned slice.
///
/// Asserted facts per frame (under `module_eid` provenance):
///   [f :vtable ?]                 (left null — frames have no vt yet)
///   [f :frame-pc <SmallInt>]
///   [f :frame-method <method-eid-Ref>]
///   [f :frame-self <Ref>]         (omitted if NIL — Date discipline)
///   [f :frame-parent <prev-frame-eid-Ref>]  (omitted for stack base)
///
/// Phase 11.6 ships single-thread semantics — the snapshot is taken
/// from the current Vm only. Multi-thread "snapshot under tx" lands
/// when the Datomic transactor introduces the channel boundary.
pub fn reifyStack(
    allocator: std.mem.Allocator,
    store: *FactStore,
    table: *ObjectTable,
    module_eid: u64,
) ![]u64 {
    const vm = vm_mod.current();
    const n = vm.frames.items.len;
    var eids = try allocator.alloc(u64, n);
    errdefer allocator.free(eids);

    for (vm.frames.items, 0..) |f, i| {
        const eid = table.alloc();
        eids[i] = eid;
        const pc_ref = ref_mod.mkSmallInt(@intCast(f.pc)) orelse return error.OutOfMemory;
        _ = try store.assert(eid, kernel_eids.ATTR_FRAME_PC, pc_ref, module_eid);
        _ = try store.assert(eid, kernel_eids.ATTR_FRAME_METHOD, ref_mod.mkEid(f.method_eid), module_eid);
        if (!ref_mod.isNil(f.self_ref)) {
            _ = try store.assert(eid, kernel_eids.ATTR_FRAME_SELF, f.self_ref, module_eid);
        }
        if (i > 0) {
            _ = try store.assert(eid, kernel_eids.ATTR_FRAME_PARENT, ref_mod.mkEid(eids[i - 1]), module_eid);
        }
    }
    return eids;
}
