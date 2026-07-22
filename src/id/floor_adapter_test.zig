//! Test-only selected-floor loader. This module imports the language-neutral
//! IdApi declaration, never the Zig floor implementation.

const std = @import("std");
const idapi = @import("idapi.zig");

const RTLD_NOW: c_int = 0x2;
const RTLD_LOCAL: c_int = 0x4;

extern fn dlopen(path: [*:0]const u8, mode: c_int) ?*anyopaque;
extern fn dlsym(handle: ?*anyopaque, name: [*:0]const u8) ?*anyopaque;
extern fn dlerror() ?[*:0]const u8;

const IdInit = *const fn () callconv(.c) *idapi.IdApi;
const FloorIdentity = *const fn () callconv(.c) [*:0]const u8;

var retained_handle: ?*anyopaque = null;
var cached_api: ?*idapi.IdApi = null;
var init_call_count: usize = 0;
var loading = false;

fn loaderPanic(operation: []const u8) noreturn {
    const detail = if (dlerror()) |message| std.mem.span(message) else "unknown loader error";
    std.debug.panic("selected-floor {s}: {s}", .{ operation, detail });
}

fn assertFloorDigest(path: []const u8, expected: []const u8) void {
    if (expected.len == 0) return;
    const file = if (std.fs.path.isAbsolute(path))
        std.fs.openFileAbsolute(path, .{}) catch @panic("selected-floor digest: open failed")
    else
        std.fs.cwd().openFile(path, .{}) catch @panic("selected-floor digest: open failed");
    defer file.close();
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    var buffer: [16 * 1024]u8 = undefined;
    while (true) {
        const count = file.read(&buffer) catch @panic("selected-floor digest: read failed");
        if (count == 0) break;
        hasher.update(buffer[0..count]);
    }
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    hasher.final(&digest);
    const actual = std.fmt.bytesToHex(digest, .lower);
    if (!std.mem.eql(u8, &actual, expected)) {
        std.debug.panic("selected-floor digest mismatch: expected {s}, got {s}", .{
            expected,
            &actual,
        });
    }
}

/// Load exactly the requested floor, assert its diagnostic identity, call
/// exported id_init once, and retain the dylib for the test process lifetime.
pub fn api() *idapi.IdApi {
    if (cached_api) |loaded| return loaded;
    if (loading) @panic("selected-floor initialization re-entered");
    loading = true;

    const path = std.posix.getenv("BIRNAM_ID_DYLIB") orelse
        @panic("BIRNAM_ID_DYLIB must select exactly one floor dylib");
    const expected_identity = std.posix.getenv("BIRNAM_ID_FLOOR_IDENTITY") orelse
        @panic("BIRNAM_ID_FLOOR_IDENTITY must name the selected floor");
    const expected_digest = std.posix.getenv("BIRNAM_ID_FLOOR_SHA256") orelse "";
    assertFloorDigest(path, expected_digest);
    const handle = dlopen(path.ptr, RTLD_NOW | RTLD_LOCAL) orelse loaderPanic("dlopen failed");

    const raw_identity = dlsym(handle, "birnam_id_floor_identity") orelse
        loaderPanic("missing birnam_id_floor_identity");
    const identity: FloorIdentity = @ptrCast(@alignCast(raw_identity));
    const actual_identity = std.mem.span(identity());
    if (!std.mem.eql(u8, actual_identity, expected_identity)) {
        std.debug.panic("selected-floor identity mismatch: expected {s}, got {s}", .{
            expected_identity,
            actual_identity,
        });
    }

    const raw_init = dlsym(handle, "id_init") orelse loaderPanic("missing id_init");
    const init: IdInit = @ptrCast(@alignCast(raw_init));
    const loaded = init();
    init_call_count += 1;
    retained_handle = handle;
    cached_api = loaded;
    loading = false;
    return loaded;
}

pub fn initCallCount() usize {
    return init_call_count;
}

pub fn retainedHandle() ?*anyopaque {
    return retained_handle;
}
