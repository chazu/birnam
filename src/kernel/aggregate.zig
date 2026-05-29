//! Phase 7.2: aggregation functions for query find clauses.
//!
//! Date-aligned design (locked per Phase 7.2 Q1-Q6):
//! - Distinct-by-default. Relations are sets; "rows" have no duplicates.
//!   `count` is distinct-count. No bag aggregation.
//! - Type-strict with diagnostic: numeric aggregates over non-numeric
//!   values raise `error.NonNumericInAggregate`.
//! - Universal group: find clause with all-aggregates returns a single
//!   row over the entire binding-set.
//! - Wildcards forbidden in aggregate slots (caught at query-build).
//! - `collect` (RRB-of-values) deferred; ships in a 7.2 follow-up
//!   once Result cells can hold RRB Refs.

const std = @import("std");
const ref_mod = @import("ref.zig");

pub const Ref = ref_mod.Ref;

pub const AggregateKind = enum(u32) {
    count = 0,
    sum = 1,
    min = 2,
    max = 3,
    avg = 4,
};

pub const AggregateSpec = struct {
    kind: AggregateKind,
    var_idx: u32,
};

pub const AggregateError = error{
    NonNumericInAggregate,
    EmptyAggregate,
    UnknownAggregateKind,
};

/// Compute an aggregate over a set of distinct values. Caller is responsible
/// for distinct semantics (deduplicating before calling). Numeric aggregates
/// require all values to be SmallInt Refs.
pub fn compute(kind: AggregateKind, values: []const Ref) !Ref {
    return switch (kind) {
        .count => ref_mod.mkSmallInt(@intCast(values.len)).?,
        .sum => try aggSum(values),
        .min => try aggMin(values),
        .max => try aggMax(values),
        .avg => try aggAvg(values),
    };
}

fn requireSmallInt(values: []const Ref) !void {
    for (values) |v| {
        if (!ref_mod.isSmallInt(v)) return error.NonNumericInAggregate;
    }
}

fn aggSum(values: []const Ref) !Ref {
    try requireSmallInt(values);
    var s: i64 = 0;
    for (values) |v| {
        s += ref_mod.untagSmallInt(v);
    }
    return ref_mod.mkSmallInt(s) orelse error.NonNumericInAggregate;
}

fn aggMin(values: []const Ref) !Ref {
    try requireSmallInt(values);
    if (values.len == 0) return error.EmptyAggregate;
    var best: i64 = ref_mod.untagSmallInt(values[0]);
    for (values[1..]) |v| {
        const x = ref_mod.untagSmallInt(v);
        if (x < best) best = x;
    }
    return ref_mod.mkSmallInt(best).?;
}

fn aggMax(values: []const Ref) !Ref {
    try requireSmallInt(values);
    if (values.len == 0) return error.EmptyAggregate;
    var best: i64 = ref_mod.untagSmallInt(values[0]);
    for (values[1..]) |v| {
        const x = ref_mod.untagSmallInt(v);
        if (x > best) best = x;
    }
    return ref_mod.mkSmallInt(best).?;
}

fn aggAvg(values: []const Ref) !Ref {
    try requireSmallInt(values);
    if (values.len == 0) return error.EmptyAggregate;
    var s: i64 = 0;
    for (values) |v| s += ref_mod.untagSmallInt(v);
    return ref_mod.mkSmallInt(@divTrunc(s, @as(i64, @intCast(values.len)))).?;
}
