//! Phase 7.4: predicates + functions.
//!
//! Predicates: boolean clauses `[(name ?a ?b ...)]`. Pass iff impl returns true.
//! Functions: computed bindings `[(name ?a ?b ...) ?target]`. Bind target to result.
//!
//! Both stored as fact-shaped entities (DATALOG.md §6, Phase 7.4 Q1):
//!   [eid :predicate-name <name-eid Ref>] [eid :predicate-impl <impl Ref>]
//!   [eid :function-name  <name-eid Ref>] [eid :function-impl  <impl Ref>]
//!
//! impl Ref is tag-discriminated at resolve time:
//!   - pointer Ref to NativePredBox / NativeFnBox → fast native path
//!   - EID Ref to closure entity → user-defined (deferred; Phase 10)
//!
//! Builtins: arith (+, -, *, /, mod), comparison-as-result (min2, max2).
//! String ops (concat, substring) deferred — strings need extra plumbing.

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

pub const NativePredFn = *const fn (args: [*]const Ref, n: usize) callconv(.c) bool;
pub const NativeFnFn = *const fn (args: [*]const Ref, n: usize) callconv(.c) Ref;

/// Heap-allocated holder for a native function pointer. Box ensures the
/// Ref payload is 8-byte aligned regardless of fn-pointer alignment quirks.
pub const NativePredBox = struct { f: NativePredFn };
pub const NativeFnBox = struct { f: NativeFnFn };

pub const Resolved = union(enum) {
    native_pred: *const NativePredBox,
    native_fn: *const NativeFnBox,
    closure_ref: u64,
};

/// Look up a predicate by its name-EID. Returns null if not registered.
pub fn resolvePred(store: *fact_store.FactStore, name_eid: u64) ?Resolved {
    var it = store.entitiesMatching(kernel_eids.ATTR_PRED_NAME, ref_mod.mkEid(name_eid));
    while (it.next()) |e_eid| {
        const impl = store.currentValue(e_eid, kernel_eids.ATTR_PRED_IMPL) orelse continue;
        if (ref_mod.isPointer(impl)) {
            const box: *const NativePredBox = @ptrCast(@alignCast(ref_mod.untagPointer(impl)));
            return .{ .native_pred = box };
        }
        if (ref_mod.isEid(impl)) return .{ .closure_ref = ref_mod.untagEid(impl) };
    }
    return null;
}

pub fn resolveFn(store: *fact_store.FactStore, name_eid: u64) ?Resolved {
    var it = store.entitiesMatching(kernel_eids.ATTR_FN_NAME, ref_mod.mkEid(name_eid));
    while (it.next()) |e_eid| {
        const impl = store.currentValue(e_eid, kernel_eids.ATTR_FN_IMPL) orelse continue;
        if (ref_mod.isPointer(impl)) {
            const box: *const NativeFnBox = @ptrCast(@alignCast(ref_mod.untagPointer(impl)));
            return .{ .native_fn = box };
        }
        if (ref_mod.isEid(impl)) return .{ .closure_ref = ref_mod.untagEid(impl) };
    }
    return null;
}

/// Invoke a predicate by name-EID with resolved args. Returns null if name
/// not registered or impl unsupported (e.g. closure path until Phase 10).
pub fn invokePred(store: *fact_store.FactStore, name_eid: u64, args: []const Ref) ?bool {
    const r = resolvePred(store, name_eid) orelse return null;
    return switch (r) {
        .native_pred => |box| box.f(args.ptr, args.len),
        .native_fn => null,
        .closure_ref => null, // Phase 10
    };
}

pub fn invokeFn(store: *fact_store.FactStore, name_eid: u64, args: []const Ref) ?Ref {
    const r = resolveFn(store, name_eid) orelse return null;
    return switch (r) {
        .native_fn => |box| box.f(args.ptr, args.len),
        .native_pred => null,
        .closure_ref => null, // Phase 10
    };
}

// -- Registration -----------------------------------------------------------

pub fn definePredNative(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    name_eid: u64,
    f: NativePredFn,
) !u64 {
    const box = try allocator.create(NativePredBox);
    box.* = .{ .f = f };
    const e = table.alloc();
    _ = try store.assert(e, kernel_eids.ATTR_PRED_NAME, ref_mod.mkEid(name_eid), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(e, kernel_eids.ATTR_PRED_IMPL, ref_mod.mkPointer(@ptrCast(box)), kernel_eids.M_BOOTSTRAP);
    return e;
}

pub fn defineFnNative(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    name_eid: u64,
    f: NativeFnFn,
) !u64 {
    const box = try allocator.create(NativeFnBox);
    box.* = .{ .f = f };
    const e = table.alloc();
    _ = try store.assert(e, kernel_eids.ATTR_FN_NAME, ref_mod.mkEid(name_eid), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(e, kernel_eids.ATTR_FN_IMPL, ref_mod.mkPointer(@ptrCast(box)), kernel_eids.M_BOOTSTRAP);
    return e;
}

// -- Builtins --------------------------------------------------------------

fn binSmallInt(args: [*]const Ref, n: usize) ?[2]i64 {
    if (n != 2) return null;
    if (!ref_mod.isSmallInt(args[0]) or !ref_mod.isSmallInt(args[1])) return null;
    return .{ ref_mod.untagSmallInt(args[0]), ref_mod.untagSmallInt(args[1]) };
}

fn fnAdd(args: [*]const Ref, n: usize) callconv(.c) Ref {
    const p = binSmallInt(args, n) orelse return ref_mod.NIL;
    return ref_mod.mkSmallInt(p[0] + p[1]) orelse ref_mod.NIL;
}

fn fnSub(args: [*]const Ref, n: usize) callconv(.c) Ref {
    const p = binSmallInt(args, n) orelse return ref_mod.NIL;
    return ref_mod.mkSmallInt(p[0] - p[1]) orelse ref_mod.NIL;
}

fn fnMul(args: [*]const Ref, n: usize) callconv(.c) Ref {
    const p = binSmallInt(args, n) orelse return ref_mod.NIL;
    return ref_mod.mkSmallInt(p[0] * p[1]) orelse ref_mod.NIL;
}

fn fnDiv(args: [*]const Ref, n: usize) callconv(.c) Ref {
    const p = binSmallInt(args, n) orelse return ref_mod.NIL;
    if (p[1] == 0) return ref_mod.NIL;
    return ref_mod.mkSmallInt(@divTrunc(p[0], p[1])) orelse ref_mod.NIL;
}

fn fnMod(args: [*]const Ref, n: usize) callconv(.c) Ref {
    const p = binSmallInt(args, n) orelse return ref_mod.NIL;
    if (p[1] == 0) return ref_mod.NIL;
    return ref_mod.mkSmallInt(@mod(p[0], p[1])) orelse ref_mod.NIL;
}

fn fnMin2(args: [*]const Ref, n: usize) callconv(.c) Ref {
    const p = binSmallInt(args, n) orelse return ref_mod.NIL;
    return ref_mod.mkSmallInt(@min(p[0], p[1])) orelse ref_mod.NIL;
}

fn fnMax2(args: [*]const Ref, n: usize) callconv(.c) Ref {
    const p = binSmallInt(args, n) orelse return ref_mod.NIL;
    return ref_mod.mkSmallInt(@max(p[0], p[1])) orelse ref_mod.NIL;
}

/// Register all built-in functions. Called once during bootstrap.
pub fn registerBuiltins(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
) !void {
    const entries = [_]struct { name: []const u8, f: NativeFnFn }{
        .{ .name = "+", .f = &fnAdd },
        .{ .name = "-", .f = &fnSub },
        .{ .name = "*", .f = &fnMul },
        .{ .name = "/", .f = &fnDiv },
        .{ .name = "mod", .f = &fnMod },
        .{ .name = "min2", .f = &fnMin2 },
        .{ .name = "max2", .f = &fnMax2 },
    };
    for (entries) |e| {
        const name_eid = try interner.intern(store, table, e.name);
        _ = try defineFnNative(allocator, store, table, name_eid, e.f);
    }
}
