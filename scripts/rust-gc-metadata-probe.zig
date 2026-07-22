//! External R4.2 collector callback fixture; sweeping remains disabled.

const std = @import("std");
const idapi = @import("idapi");

var provider_calls: usize = 0;
var trace_calls: usize = 0;
var finalizer_calls: usize = 0;
var provider_root: idapi.Oop = null;
var trace_child: idapi.Oop = null;
var retained_mark: ?idapi.MarkFn = null;
var active_api: ?*idapi.IdApi = null;

fn provider(mark: idapi.MarkFn) callconv(.c) void {
    provider_calls += 1;
    const api = active_api.?;
    _ = api.gcLiveCount();
    _ = api.vtableOf(provider_root);
    mark(provider_root);
}

fn trace(instance: idapi.Oop, mark: idapi.MarkFn) callconv(.c) void {
    trace_calls += 1;
    const api = active_api.?;
    _ = api.gcLiveCount();
    _ = api.vtableOf(instance);
    mark(trace_child);
}

fn retainingProvider(mark: idapi.MarkFn) callconv(.c) void {
    retained_mark = mark;
}

fn finalize(_: idapi.Oop) callconv(.c) void {
    finalizer_calls += 1;
}

const layout_a = [_]u8{ idapi.slot_oop, idapi.slot_code, 9 };
const layout_b = [_]u8{idapi.slot_raw};

fn require(condition: bool, failure: anyerror) !void {
    if (!condition) return failure;
}

fn registerMaximum(api: *idapi.IdApi) void {
    for (0..32) |_| api.gcRegisterRootProvider(provider);
}

fn runFunctional(api: *idapi.IdApi) !void {
    for ([_]idapi.Oop{
        api.smallint_vt,
        api.string_vt,
        api.float_vt,
        api.word_vt,
        api.vtableOf(api.nilObj),
        api.vtableOf(api.trueObj),
    }) |vtable| {
        try require(api.slotKind(vtable, 0) == idapi.slot_raw, error.MissingFloorLayout);
        try require(api.slotKind(vtable, 99) == idapi.slot_raw, error.MissingFloorLayout);
    }

    const child = api.proto(api.object_vt);
    try require(api.slotKind(child, 0) == idapi.slot_unknown, error.NewVtableHasLayout);
    api.setLayout(child, &layout_a, layout_a.len, idapi.slot_raw);
    try require(api.slotKind(child, 0) == idapi.slot_oop, error.WrongLayoutPrefix);
    try require(api.slotKind(child, 1) == idapi.slot_code, error.WrongLayoutPrefix);
    try require(api.slotKind(child, 2) == 9, error.WrongLayoutPrefix);
    try require(api.slotKind(child, 3) == idapi.slot_raw, error.WrongLayoutRest);
    api.setLayout(child, &layout_b, layout_b.len, idapi.slot_oop);
    try require(api.slotKind(child, 0) == idapi.slot_raw, error.LayoutNotReplaced);
    try require(api.slotKind(child, 1) == idapi.slot_oop, error.LayoutRestNotReplaced);

    api.setTraceHook(child, trace);
    const live_before = api.gcLiveCount();
    const object = api.alloc(child, 16);
    const unreachable_obj = api.alloc(child, 0);
    const external_child = api.alloc(api.object_vt, 0);
    provider_root = object;
    trace_child = external_child;
    active_api = api;
    api.setFinalizer(object, finalize);
    try require(api.gcLiveCount() == live_before + 3, error.LiveCountDidNotGrow);
    registerMaximum(api);
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    api.gcSetAuto(1);
    api.gcSetPrecise(1);
    try require(api.gcCollect() == 0, error.DisabledCollectionReclaimed);
    try require(api.gcLiveCount() == live_before + 3, error.DisabledCollectionChangedLiveSet);
    try require(provider_calls == 32, error.RootProviderNotInvoked);
    try require(trace_calls == 1, error.TraceHookReachabilityWrong);
    try require(finalizer_calls == 0, error.DisabledCollectionInvokedFinalizer);
    try require(api.vtableOf(unreachable_obj) == child, error.UnreachableFixtureChanged);
    try require(api.vtableOf(external_child) == api.object_vt, error.TraceChildChanged);
    stack_anchor = api.gcLiveCount();
    std.mem.doNotOptimizeAway(&stack_anchor);
    std.debug.print("rust-gc-metadata: providers and reachable hooks marked without sweeping\n", .{});
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    const mode = args.next() orelse return error.MissingMode;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    const init = dylib.lookup(idapi.IdInitFn, "id_init") orelse return error.MissingIdInit;
    const api = init();

    if (std.mem.eql(u8, mode, "functional")) return runFunctional(api);
    if (std.mem.eql(u8, mode, "provider-overflow")) {
        registerMaximum(api);
        api.gcRegisterRootProvider(provider);
        return error.ProviderOverflowReturned;
    }
    if (std.mem.eql(u8, mode, "invalid-layout")) {
        const child = api.proto(api.object_vt);
        api.setLayout(child, null, 1, idapi.slot_raw);
        return error.InvalidLayoutReturned;
    }
    if (std.mem.eql(u8, mode, "retained-mark")) {
        active_api = api;
        api.gcRegisterRootProvider(retainingProvider);
        _ = api.gcCollect();
        retained_mark.?(api.nilObj);
        return error.RetainedMarkReturned;
    }
    return error.UnknownMode;
}
