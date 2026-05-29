//! Phase 7.3: pull syntax.
//!
//! A pull spec describes how to project attributes off an entity, returning
//! a hierarchical map (Refs nested with sub-pulls / recursion). Designed for
//! reading graph-shaped data without writing an explicit query per level.
//!
//! Phase 7.3 design (single-cardinality per Q10):
//! - Each `:attr` element fetches the current value of that attribute on
//!   the entity.
//! - `recurse` element walks an EID-valued attribute up to `depth` levels.
//!   Cycle detection via visited set; on revisit, a `cycle_marker` is
//!   emitted instead of looping (Q8 lean B).
//! - `sub` element fetches an attribute, expects its value to be an EID,
//!   and pulls that EID with the sub-spec.
//! - Default max depth: 100 (Q1).
//! - Multi-value pull deferred to Phase 9.5 (Q10): pulls only ever fetch
//!   one Ref per attribute (single-cardinality).

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");

pub const Ref = ref_mod.Ref;

pub const PullElement = union(enum) {
    /// Fetch attribute's current value; leaf in the result tree.
    attr: u64,
    /// Walk attribute (must be EID-valued) recursively up to depth.
    recurse: RecurseSpec,
    /// Fetch attribute (EID-valued), then pull the value with sub-spec.
    sub: SubSpec,
};

pub const RecurseSpec = struct {
    attr: u64,
    /// Maximum levels to walk. 0 = use PullSpec.max_depth.
    depth: u32 = 0,
};

pub const SubSpec = struct {
    attr: u64,
    spec: []const PullElement,
};

pub const PullSpec = struct {
    elements: []const PullElement,
    max_depth: u32 = 100,
};

pub const PullValue = union(enum) {
    /// Attribute absent on entity (Date discipline: no NIL value).
    null_value,
    /// Single Ref value.
    leaf: Ref,
    /// Nested pull result.
    nested: *PullResult,
    /// Recursion hit a previously-visited entity (cycle).
    cycle_marker: u64,
    /// Recursion bottomed out at depth limit.
    depth_marker,
};

pub const PullEntry = struct {
    attr: u64,
    value: PullValue,
};

pub const PullResult = struct {
    entity: u64,
    entries: []PullEntry,

    pub fn deinit(self: *PullResult, allocator: std.mem.Allocator) void {
        for (self.entries) |entry| {
            switch (entry.value) {
                .nested => |p| {
                    p.deinit(allocator);
                    allocator.destroy(p);
                },
                else => {},
            }
        }
        allocator.free(self.entries);
    }
};

pub const PullError = error{
    OutOfMemory,
};

/// Execute a pull spec against an entity, returning a heap-allocated
/// PullResult. Caller owns the result and must call `deinit`.
pub fn executePull(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    entity: u64,
    spec: PullSpec,
) PullError!*PullResult {
    var visited = std.AutoHashMap(u64, void).init(allocator);
    defer visited.deinit();
    return executePullInner(allocator, store, entity, spec, &visited, spec.max_depth);
}

fn executePullInner(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    entity: u64,
    spec: PullSpec,
    visited: *std.AutoHashMap(u64, void),
    remaining_depth: u32,
) PullError!*PullResult {
    try visited.put(entity, {});
    defer _ = visited.remove(entity);

    const entries = try allocator.alloc(PullEntry, spec.elements.len);
    for (spec.elements, 0..) |elem, i| {
        switch (elem) {
            .attr => |a| {
                const v = store.currentValue(entity, a);
                entries[i] = .{ .attr = a, .value = if (v) |val| .{ .leaf = val } else .null_value };
            },
            .recurse => |rs| {
                const eff_depth = if (rs.depth == 0) remaining_depth else rs.depth;
                entries[i] = try executeRecurse(allocator, store, entity, rs, spec, visited, eff_depth);
            },
            .sub => |ss| {
                const v = store.currentValue(entity, ss.attr) orelse {
                    entries[i] = .{ .attr = ss.attr, .value = .null_value };
                    continue;
                };
                if (!ref_mod.isEid(v)) {
                    entries[i] = .{ .attr = ss.attr, .value = .{ .leaf = v } };
                    continue;
                }
                const sub_entity = ref_mod.untagEid(v);
                if (visited.contains(sub_entity)) {
                    entries[i] = .{ .attr = ss.attr, .value = .{ .cycle_marker = sub_entity } };
                    continue;
                }
                const sub_spec = PullSpec{ .elements = ss.spec, .max_depth = spec.max_depth };
                const nested = try executePullInner(allocator, store, sub_entity, sub_spec, visited, remaining_depth);
                entries[i] = .{ .attr = ss.attr, .value = .{ .nested = nested } };
            },
        }
    }

    const result = try allocator.create(PullResult);
    result.* = .{ .entity = entity, .entries = entries };
    return result;
}

fn executeRecurse(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    entity: u64,
    rs: RecurseSpec,
    parent_spec: PullSpec,
    visited: *std.AutoHashMap(u64, void),
    remaining_depth: u32,
) PullError!PullEntry {
    const v = store.currentValue(entity, rs.attr) orelse {
        return .{ .attr = rs.attr, .value = .null_value };
    };
    if (!ref_mod.isEid(v)) {
        return .{ .attr = rs.attr, .value = .{ .leaf = v } };
    }
    if (remaining_depth == 0) {
        return .{ .attr = rs.attr, .value = .depth_marker };
    }
    const next_entity = ref_mod.untagEid(v);
    if (visited.contains(next_entity)) {
        return .{ .attr = rs.attr, .value = .{ .cycle_marker = next_entity } };
    }
    // Recurse: pull the next entity with a spec carrying a decremented
    // user-budget rs.depth so the inner pull's recurse element respects
    // both the parent remaining_depth AND the user's explicit cap.
    const inner_rs = RecurseSpec{ .attr = rs.attr, .depth = if (rs.depth == 0) 0 else rs.depth - 1 };
    const inner_elements = try allocator.alloc(PullElement, 1);
    inner_elements[0] = .{ .recurse = inner_rs };
    const inner_spec = PullSpec{ .elements = inner_elements, .max_depth = parent_spec.max_depth };
    const nested = try executePullInner(allocator, store, next_entity, inner_spec, visited, remaining_depth - 1);
    return .{ .attr = rs.attr, .value = .{ .nested = nested } };
}
