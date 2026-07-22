//! Selected-floor GC count probe for the R0.5 compatibility baseline.

const std = @import("std");
const builtin = @import("builtin");
const idapi = @import("idapi.zig");
const floor = @import("floor_adapter_test.zig");

const Oop = idapi.Oop;
var finalizer_calls: usize = 0;

fn countFinalizer(_: Oop) callconv(.c) void {
    finalizer_calls += 1;
}

noinline fn makeGarbage(api: *idapi.IdApi, count: usize, payload_size: usize) void {
    var last: Oop = null;
    for (0..count) |_| last = api.alloc(api.object_vt, payload_size);
    std.mem.doNotOptimizeAway(&last);
}

noinline fn makeFinalizableGarbage(api: *idapi.IdApi, count: usize) void {
    var last: Oop = null;
    for (0..count) |_| {
        last = api.alloc(api.object_vt, 0);
        api.setFinalizer(last, &countFinalizer);
    }
    std.mem.doNotOptimizeAway(&last);
}

noinline fn scrubConservativeStack() void {
    var scratch: [4096]usize = @splat(0);
    for (&scratch, 0..) |*word, i| word.* = i;
    std.mem.doNotOptimizeAway(&scratch);
}

fn settle(api: *idapi.IdApi) usize {
    var reclaimed: usize = 0;
    for (0..4) |_| {
        scrubConservativeStack();
        reclaimed += api.gcCollect();
    }
    return reclaimed;
}

pub fn main() !void {
    if (builtin.mode != .ReleaseSafe) {
        std.debug.print(
            "GC baseline requires -Doptimize=ReleaseSafe (got {s})\n",
            .{@tagName(builtin.mode)},
        );
        return error.ReleaseSafeRequired;
    }

    const api = floor.api();
    const identity = std.posix.getenv("BIRNAM_ID_FLOOR_IDENTITY") orelse
        @panic("BIRNAM_ID_FLOOR_IDENTITY must name the selected floor");
    var stack_bottom: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_bottom));
    api.gcSetAuto(0);
    _ = settle(api);

    const manual_before = api.gcLiveCount();
    makeGarbage(api, 2000, 0);
    const manual_peak = api.gcLiveCount();
    const manual_reclaimed = settle(api);
    const manual_after = api.gcLiveCount();
    if (manual_peak < manual_before + 2000 or
        manual_reclaimed < 1900 or
        manual_after > manual_before + 100)
        return error.ManualCollectionInvariant;

    _ = settle(api);
    const finalizer_before = api.gcLiveCount();
    finalizer_calls = 0;
    makeFinalizableGarbage(api, 1000);
    const finalizer_peak = api.gcLiveCount();
    const finalizer_reclaimed = settle(api);
    const finalizer_after = api.gcLiveCount();
    if (finalizer_calls != 1000 or finalizer_reclaimed < 1000 or
        finalizer_peak < finalizer_before + 1000 or
        finalizer_after > finalizer_before + 100)
        return error.FinalizerInvariant;

    _ = settle(api);
    const auto_before = api.gcLiveCount();
    api.gcSetAuto(1);
    makeGarbage(api, 6000, 4096);
    api.gcSetAuto(0);
    const auto_after = api.gcLiveCount();
    if (auto_after >= auto_before + 5000) return error.AutoCollectionInvariant;

    const stdout = std.fs.File.stdout().deprecatedWriter();
    try stdout.print(
        "{{\"schema\":1,\"floor_identity\":\"{s}\",\"mode\":\"ReleaseSafe\"," ++
            "\"manual\":{{\"before\":{},\"peak\":{},\"reclaimed\":{},\"after\":{}}}," ++
            "\"finalizers\":{{\"before\":{},\"peak\":{},\"reclaimed\":{}," ++
            "\"after\":{},\"calls\":{}}},\"auto_gc\":{{\"before\":{},\"allocations\":6000," ++
            "\"payload_bytes\":4096,\"after\":{},\"live_delta\":{}}}}}\n",
        .{
            identity,
            manual_before,
            manual_peak,
            manual_reclaimed,
            manual_after,
            finalizer_before,
            finalizer_peak,
            finalizer_reclaimed,
            finalizer_after,
            finalizer_calls,
            auto_before,
            auto_after,
            auto_after - auto_before,
        },
    );
    std.mem.doNotOptimizeAway(&stack_bottom);
}
