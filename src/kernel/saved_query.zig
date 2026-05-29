//! Phase 7.3: saved queries.
//!
//! A saved query is a fact-entity binding a name to a `*Query` spec:
//!   [saved_eid :saved-query-name  <mkEid(name_eid)>]
//!   [saved_eid :saved-query-spec  <pointer Ref to *Query>]
//!
//! `name_eid` is the interner's EID for the name string. Two save calls
//! with the same name target the same `name_eid`; under Phase 3 single
//! cardinality the latest `:saved-query-spec` assertion wins (Q11 shadow
//! semantics — confirmed Phase 7.3 lean A).
//!
//! Ownership: `saveQuery` takes ownership of the `*Query` until an
//! explicit retract. The caller must not `free_query` after saving.

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const kernel_eids = @import("kernel_eids.zig");
const query_mod = @import("query.zig");

pub const Ref = ref_mod.Ref;

/// Save (or update) a query under `name`. Returns the saved-query entity EID.
/// If a saved query with the same name already exists, this updates its
/// `:saved-query-spec` fact (Phase 3 single-cardinality replaces the prior
/// spec — Q11 shadow semantics).
pub fn saveQuery(
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    name_eid: u64,
    q: *query_mod.Query,
) !u64 {
    // Look up existing saved-query by name.
    var existing: ?u64 = null;
    var it = store.entitiesMatching(kernel_eids.ATTR_SAVED_QUERY_NAME, ref_mod.mkEid(name_eid));
    while (it.next()) |e| {
        existing = e;
        break;
    }

    const saved_eid = existing orelse table.alloc();
    _ = try store.assert(saved_eid, kernel_eids.ATTR_SAVED_QUERY_NAME, ref_mod.mkEid(name_eid), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(saved_eid, kernel_eids.ATTR_SAVED_QUERY_SPEC, ref_mod.mkPointer(@ptrCast(q)), kernel_eids.M_BOOTSTRAP);
    return saved_eid;
}

/// Look up the `*Query` for a saved name, or null if not found.
pub fn lookupQuery(
    store: *fact_store.FactStore,
    name_eid: u64,
) ?*query_mod.Query {
    var it = store.entitiesMatching(kernel_eids.ATTR_SAVED_QUERY_NAME, ref_mod.mkEid(name_eid));
    const saved_eid = it.next() orelse return null;
    const spec_ref = store.currentValue(saved_eid, kernel_eids.ATTR_SAVED_QUERY_SPEC) orelse return null;
    if (!ref_mod.isPointer(spec_ref)) return null;
    return @ptrCast(@alignCast(ref_mod.untagPointer(spec_ref)));
}
