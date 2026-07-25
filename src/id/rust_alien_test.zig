//! Focused live-owner proof for the Rust Alien R5 slices.
//!
//! This is intentionally narrower than the full Zig Alien corpus: the Rust
//! implementation currently owns loader state and generated wrapper
//! installation, while callbacks, raw buffers, and image replay remain later
//! migration slices.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

const RTLD_NOW: c_int = 0x2;
const RTLD_LOCAL: c_int = 0x4;

extern fn dlopen(path: [*:0]const u8, mode: c_int) ?*anyopaque;
extern fn dlsym(handle: ?*anyopaque, name: [*:0]const u8) ?*anyopaque;

const LivePoint = extern struct { x: c_long, y: c_long };

export fn birnam_rust_alien_apply(
    callback: *const fn (c_long) callconv(.c) c_long,
    value: c_long,
) c_long {
    return callback(value);
}

export fn birnam_rust_alien_apply_point(
    callback: *const fn (LivePoint) callconv(.c) c_long,
) c_long {
    return callback(.{ .x = 20, .y = 22 });
}

fn retainExportedFixtures() void {
    const fixtures = .{ &birnam_rust_alien_apply, &birnam_rust_alien_apply_point };
    inline for (fixtures) |fixture| std.mem.doNotOptimizeAway(fixture);
}

var retained_handles: [3]?*anyopaque = .{ null, null, null };
var retained_count: usize = 0;

fn loadModule(
    api: *idapi.IdApi,
    env_name: []const u8,
    identity_export: [:0]const u8,
    expected_identity: []const u8,
) void {
    const path = std.posix.getenv(env_name) orelse
        std.debug.panic("{s} must select the Rust Alien fixture", .{env_name});
    const handle = dlopen(path.ptr, RTLD_NOW | RTLD_LOCAL) orelse
        std.debug.panic("could not load {s}", .{path});
    const raw_identity = dlsym(handle, identity_export.ptr) orelse
        std.debug.panic("Rust Alien fixture has no identity export", .{});
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    if (!std.mem.eql(u8, std.mem.span(identity()), expected_identity)) {
        std.debug.panic("Rust Alien identity mismatch", .{});
    }
    const raw_init = dlsym(handle, "module_init") orelse
        std.debug.panic("Rust Alien fixture has no module_init", .{});
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
    retained_handles[retained_count] = handle;
    retained_count += 1;
}

fn str(api: *idapi.IdApi, bytes: []const u8) Oop {
    return api.mkString(bytes.ptr, bytes.len);
}

var live_api: *idapi.IdApi = undefined;

fn liveResponder(api: *idapi.IdApi, selector: [*:0]const u8, method: idapi.Method) Oop {
    live_api = api;
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern(selector), method);
    return api.alloc(vt, 0);
}

fn liveDouble(_self: Oop, args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _count;
    return live_api.mkSmallInt(live_api.smallIntValue(args[0]) * 2);
}

fn livePointSum(_self: Oop, args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _count;
    const vector = args[0];
    const size = live_api.smallIntValue(live_api.send(
        vector,
        live_api.intern("size"),
        &[_]Oop{},
        0,
    ));
    if (size != 2) return live_api.nilObj;
    const x = live_api.smallIntValue(live_api.send(
        vector,
        live_api.intern("at:"),
        &[_]Oop{live_api.mkSmallInt(0)},
        1,
    ));
    const y = live_api.smallIntValue(live_api.send(
        vector,
        live_api.intern("at:"),
        &[_]Oop{live_api.mkSmallInt(1)},
        1,
    ));
    return live_api.mkSmallInt(x + y);
}

test "Rust Alien owns loader state and installs a generated scalar wrapper" {
    retainExportedFixtures();
    const api = floor.api();
    loadModule(
        api,
        "BIRNAM_RUST_TCC_DYLIB",
        "birnam_tcc_rust_identity",
        "rust-tcc-n4.2-state",
    );
    loadModule(
        api,
        "BIRNAM_RUST_COLLECTION_DYLIB",
        "birnam_collection_rust_identity",
        "rust-collection-r1-vector",
    );
    loadModule(
        api,
        "BIRNAM_RUST_ALIEN_DYLIB",
        "birnam_alien_rust_identity",
        "rust-alien-r5-loader",
    );

    const alien = api.import("Alien") orelse return error.AlienExportMissing;
    const dlclose = api.intern("dlclose:");
    const system_name = if (@import("builtin").os.tag == .macos)
        "libSystem.B.dylib"
    else
        "libc.so.6";
    try std.testing.expect(
        api.send(alien, api.intern("dlopen:"), &[_]Oop{str(api, system_name)}, 1) == api.trueObj,
    );
    try std.testing.expect(
        api.send(alien, dlclose, &[_]Oop{str(api, system_name)}, 1) == api.trueObj,
    );

    const selector = api.intern("rustStrlen");
    const wrapped = api.send(
        alien,
        api.intern("wrap:signature:as:on:"),
        &[_]Oop{ str(api, "strlen"), str(api, "l s"), selector, api.string_vt },
        4,
    );
    try std.testing.expect(wrapped != api.nilObj);
    const answer = api.send(str(api, "hello"), selector, &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 5), api.smallIntValue(answer));

    const aggregate_selector = api.intern("rustDiv");
    const aggregate_wrap = api.send(
        alien,
        api.intern("wrap:signature:as:on:"),
        &[_]Oop{ str(api, "div"), str(api, "{ii} i i"), aggregate_selector, api.smallint_vt },
        4,
    );
    try std.testing.expect(aggregate_wrap != api.nilObj);
    const aggregate = api.send(
        api.mkSmallInt(20),
        aggregate_selector,
        &[_]Oop{api.mkSmallInt(3)},
        1,
    );
    try std.testing.expect(api.vtableOf(aggregate) == api.import("VectorVt").?);
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(api.send(
        aggregate,
        api.intern("size"),
        &[_]Oop{},
        0,
    )));
    try std.testing.expectEqual(@as(i64, 6), api.smallIntValue(api.send(
        aggregate,
        api.intern("at:"),
        &[_]Oop{api.mkSmallInt(0)},
        1,
    )));
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(api.send(
        aggregate,
        api.intern("at:"),
        &[_]Oop{api.mkSmallInt(1)},
        1,
    )));
}

test "Rust Alien owns callback code, aggregate marshalling, GC roots, and release" {
    retainExportedFixtures();
    const api = floor.api();
    const alien = api.import("Alien") orelse return error.AlienExportMissing;
    const callback = api.send(
        alien,
        api.intern("callback:signature:"),
        &[_]Oop{ liveResponder(api, "value:", &liveDouble), str(api, "l l") },
        2,
    );
    try std.testing.expect(callback != api.nilObj);

    _ = api.send(
        alien,
        api.intern("wrap:signature:as:on:"),
        &[_]Oop{ str(api, "birnam_rust_alien_apply"), str(api, "l p l"), api.intern("rustApply:"), api.smallint_vt },
        4,
    );
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    var index: usize = 0;
    while (index < 10_000) : (index += 1) _ = api.mkString("callback-churn", 14);
    _ = api.gcCollect();
    const doubled = api.send(
        callback,
        api.intern("rustApply:"),
        &[_]Oop{api.mkSmallInt(21)},
        1,
    );
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(doubled));

    const point_callback = api.send(
        alien,
        api.intern("callback:signature:"),
        &[_]Oop{ liveResponder(api, "value:", &livePointSum), str(api, "l {ll}") },
        2,
    );
    try std.testing.expect(point_callback != api.nilObj);
    _ = api.send(
        alien,
        api.intern("wrap:signature:as:on:"),
        &[_]Oop{ str(api, "birnam_rust_alien_apply_point"), str(api, "l p"), api.intern("rustApplyPoint"), api.smallint_vt },
        4,
    );
    const point_sum = api.send(
        point_callback,
        api.intern("rustApplyPoint"),
        &[_]Oop{},
        0,
    );
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(point_sum));

    try std.testing.expect(
        api.send(alien, api.intern("releaseCallback:"), &[_]Oop{callback}, 1) == api.trueObj,
    );
    try std.testing.expect(
        api.send(alien, api.intern("releaseCallback:"), &[_]Oop{callback}, 1) != api.trueObj,
    );
}
