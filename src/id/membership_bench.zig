//! ReleaseSafe selected-floor benchmark for the exact-address registries.
//! This is deliberately outside the floor implementation: the same executable
//! will measure Zig and Rust through the frozen IdApi.

const std = @import("std");
const builtin = @import("builtin");
const idapi = @import("idapi.zig");
const floor = @import("floor_adapter_test.zig");

const Oop = idapi.Oop;
const populations = [_]usize{ 1024, 16_384, 131_072 };
const probe_count = 1024;
const iterations = 1 << 19;
const repetitions = 5;

const Case = enum {
    object_membership,
    vtable_of,
    vtable_of_reject,
    vtable_owner,
    is_small_int,
    is_float,
    is_word,

    fn name(case: Case) []const u8 {
        return switch (case) {
            .object_membership => "objectPayloadSize",
            .vtable_of => "vtableOf",
            .vtable_of_reject => "vtableOf.reject",
            .vtable_owner => "vtableOwner",
            .is_small_int => "isSmallInt",
            .is_float => "isFloat",
            .is_word => "isWord",
        };
    }
};

const cases = std.enums.values(Case);

const Probes = struct {
    objects: [probe_count]Oop,
    vtables: [probe_count]Oop,
    integers: [probe_count]Oop,
    floats: [probe_count]Oop,
    words: [probe_count]Oop,
};

const Sample = struct {
    live_objects: usize,
    vtables: usize,
    raw_ns_per_call: f64,
    loop_ns_per_call: f64,
    ns_per_call: f64,
};

fn foreignMethod(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return null;
}

fn pointerBits(value: Oop) usize {
    return @intFromPtr(value orelse return 0);
}

fn runOperation(case: Case, api: *idapi.IdApi, probes: *const Probes) u64 {
    const foreign: Oop = @ptrCast(@constCast(&foreignMethod));
    var result: u64 = 0;
    var i: usize = 0;
    while (i < iterations) : (i += 1) {
        const probe = i & (probe_count - 1);
        result +%= switch (case) {
            .object_membership => api.objectPayloadSize(probes.objects[probe]),
            .vtable_of => pointerBits(api.vtableOf(probes.objects[probe])),
            .vtable_of_reject => pointerBits(api.vtableOf(foreign)),
            .vtable_owner => pointerBits(api.vtableOwner(probes.vtables[probe])),
            .is_small_int => @intCast(api.isSmallInt(probes.integers[probe])),
            .is_float => @intCast(api.isFloat(probes.floats[probe])),
            .is_word => @intCast(api.isWord(probes.words[probe])),
        };
    }
    std.mem.doNotOptimizeAway(&result);
    return result;
}

fn runLoop(probes: *const Probes) u64 {
    var result: u64 = 0;
    var i: usize = 0;
    while (i < iterations) : (i += 1) {
        result +%= pointerBits(probes.objects[i & (probe_count - 1)]);
    }
    std.mem.doNotOptimizeAway(&result);
    return result;
}

fn median(values: *[repetitions]u64) u64 {
    var i: usize = 1;
    while (i < values.len) : (i += 1) {
        const value = values[i];
        var j = i;
        while (j > 0 and values[j - 1] > value) : (j -= 1) {
            values[j] = values[j - 1];
        }
        values[j] = value;
    }
    return values[values.len / 2];
}

fn measure(case: Case, api: *idapi.IdApi, probes: *const Probes) Sample {
    var raw: [repetitions]u64 = undefined;
    var loop: [repetitions]u64 = undefined;
    var net: [repetitions]u64 = undefined;

    _ = runOperation(case, api, probes);
    var rep: usize = 0;
    while (rep < repetitions) : (rep += 1) {
        var loop_timer = std.time.Timer.start() catch @panic("monotonic timer unavailable");
        _ = runLoop(probes);
        loop[rep] = loop_timer.read();

        var raw_timer = std.time.Timer.start() catch @panic("monotonic timer unavailable");
        _ = runOperation(case, api, probes);
        raw[rep] = raw_timer.read();
        net[rep] = raw[rep] -| loop[rep];
    }

    const raw_median = median(&raw);
    const loop_median = median(&loop);
    const net_median = median(&net);
    const divisor: f64 = @floatFromInt(iterations);
    return .{
        .live_objects = api.gcLiveCount(),
        .vtables = api.vtableCount(),
        .raw_ns_per_call = @as(f64, @floatFromInt(raw_median)) / divisor,
        .loop_ns_per_call = @as(f64, @floatFromInt(loop_median)) / divisor,
        .ns_per_call = @as(f64, @floatFromInt(net_median)) / divisor,
    };
}

fn scalingExponent(first: Sample, last: Sample) f64 {
    if (first.ns_per_call <= 0 or last.ns_per_call <= 0) return 0;
    const population_ratio =
        @as(f64, @floatFromInt(last.live_objects)) /
        @as(f64, @floatFromInt(first.live_objects));
    return @log(last.ns_per_call / first.ns_per_call) / @log(population_ratio);
}

pub fn main() !void {
    if (builtin.mode != .ReleaseSafe) {
        std.debug.print(
            "membership benchmark requires -Doptimize=ReleaseSafe (got {s})\n",
            .{@tagName(builtin.mode)},
        );
        return error.ReleaseSafeRequired;
    }

    const api = floor.api();
    const identity = std.posix.getenv("BIRNAM_ID_FLOOR_IDENTITY") orelse
        @panic("BIRNAM_ID_FLOOR_IDENTITY must name the selected floor");
    api.gcSetAuto(0);

    var probes: Probes = undefined;
    for (0..probe_count) |i| {
        probes.integers[i] = api.mkSmallInt(@intCast(i));
        probes.floats[i] = api.mkFloat(@floatFromInt(i));
        probes.words[i] = api.mkWord(i);
        probes.objects[i] = api.alloc(api.object_vt, @sizeOf(usize));
        probes.vtables[i] = api.proto(api.object_vt);
        api.setOwner(probes.vtables[i], probes.objects[i]);
    }

    var samples: [cases.len][populations.len]Sample = undefined;
    var ring: usize = 0;
    for (populations, 0..) |population, population_index| {
        while (api.vtableCount() < population) {
            const vt = api.proto(api.object_vt);
            const object = api.alloc(vt, @sizeOf(usize));
            api.setOwner(vt, object);
            probes.vtables[ring] = vt;
            probes.objects[ring] = object;
            ring = (ring + 1) & (probe_count - 1);
        }
        for (cases, 0..) |case, case_index| {
            samples[case_index][population_index] = measure(case, api, &probes);
        }
    }

    const stdout = std.fs.File.stdout().deprecatedWriter();
    try stdout.print(
        "{{\"schema\":1,\"floor_identity\":\"{s}\"," ++
            "\"mode\":\"ReleaseSafe\",\"iterations\":524288,\"repetitions\":5,\"cases\":[",
        .{identity},
    );
    for (cases, 0..) |case, case_index| {
        if (case_index != 0) try stdout.writeByte(',');
        try stdout.print("{{\"name\":\"{s}\",\"samples\":[", .{case.name()});
        for (samples[case_index], 0..) |sample, sample_index| {
            if (sample_index != 0) try stdout.writeByte(',');
            try stdout.print(
                "{{\"live_objects\":{},\"vtables\":{},\"raw_ns_per_call\":{d:.3}," ++
                    "\"loop_ns_per_call\":{d:.3},\"ns_per_call\":{d:.3}}}",
                .{
                    sample.live_objects,
                    sample.vtables,
                    sample.raw_ns_per_call,
                    sample.loop_ns_per_call,
                    sample.ns_per_call,
                },
            );
        }
        try stdout.print(
            "],\"scaling_exponent\":{d:.4}}}",
            .{scalingExponent(samples[case_index][0], samples[case_index][populations.len - 1])},
        );
    }
    try stdout.writeAll("]}\n");
}
