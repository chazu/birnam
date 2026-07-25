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

test "Rust Alien owns loader state and installs a generated scalar wrapper" {
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
