//! Selected-floor mark-decision parity for precise heap layouts. Trace hooks on
//! target objects make reachability observable before R4.5 enables Rust sweep.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var g_root: Oop = null;
var g_targets: [4]Oop = .{ null, null, null, null };
var g_trace_calls: [4]usize = .{ 0, 0, 0, 0 };

const mixed_layout = [_]u8{ idapi.slot_oop, idapi.slot_raw, idapi.slot_code };

fn provider(mark: idapi.MarkFn) callconv(.c) void {
    mark(g_root);
}

fn trace(instance: Oop, _: idapi.MarkFn) callconv(.c) void {
    for (g_targets, 0..) |target, index| {
        if (instance == target) g_trace_calls[index] += 1;
    }
}

noinline fn installGraph(api: *idapi.IdApi, precise_layout: bool) void {
    const target_vtable = api.proto(api.object_vt);
    api.setTraceHook(target_vtable, trace);
    for (&g_targets) |*target| target.* = api.alloc(target_vtable, 0);
    g_trace_calls = .{ 0, 0, 0, 0 };

    const root_vtable = api.proto(api.object_vt);
    if (precise_layout)
        api.setLayout(root_vtable, &mixed_layout, mixed_layout.len, idapi.slot_oop);
    g_root = api.alloc(root_vtable, 4 * @sizeOf(Oop));
    const words: [*]Oop = @ptrCast(@alignCast(g_root.?));
    for (g_targets, 0..) |target, index| words[index] = target;
}

noinline fn launderStack() void {
    var words: [1024]usize = undefined;
    for (&words, 0..) |*word, index| word.* = index *% 0x9e3779b97f4a7c15;
    std.mem.doNotOptimizeAway(&words);
}

fn setup() *idapi.IdApi {
    const api = floor.api();
    api.gcRegisterRootProvider(provider);
    return api;
}

test "precise layout marks Oop prefix and rest but not raw or code slots" {
    const api = setup();
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    defer api.gcSetPrecise(0);
    installGraph(api, true);
    launderStack();
    api.gcSetPrecise(1);
    _ = api.gcCollect();
    try std.testing.expectEqual([4]usize{ 1, 0, 0, 1 }, g_trace_calls);
    stack_anchor = api.gcLiveCount();
    std.mem.doNotOptimizeAway(&stack_anchor);
}

test "precise mode leaves an unknown layout conservative" {
    const api = setup();
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    defer api.gcSetPrecise(0);
    installGraph(api, false);
    launderStack();
    api.gcSetPrecise(1);
    _ = api.gcCollect();
    try std.testing.expectEqual([4]usize{ 1, 1, 1, 1 }, g_trace_calls);
    stack_anchor = api.gcLiveCount();
    std.mem.doNotOptimizeAway(&stack_anchor);
}
