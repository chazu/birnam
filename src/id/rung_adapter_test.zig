//! Test-only backend-neutral native-rung loader.
//!
//! A process selects one implementation path for one singleton-owning rung,
//! verifies its explicit implementation name and content digest before dlopen,
//! calls module_init once, and retains the handle for process lifetime.

const std = @import("std");
const idapi = @import("idapi.zig");

const RTLD_NOW: c_int = 0x2;
const RTLD_LOCAL: c_int = 0x4;

extern fn dlopen(path: [*:0]const u8, mode: c_int) ?*anyopaque;
extern fn dlsym(handle: ?*anyopaque, name: [*:0]const u8) ?*anyopaque;
extern fn dlerror() ?[*:0]const u8;

const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

pub const Implementation = enum { zig, rust };

pub const Selection = struct {
    rung: []const u8,
    implementation: Implementation,
    canonical_path: [:0]const u8,
    explicit_path: [:0]const u8,
};

const Loaded = struct {
    rung: []const u8,
    implementation: Implementation,
    canonical_path: []const u8,
    explicit_path: []const u8,
    digest: [std.crypto.hash.sha2.Sha256.digest_length]u8,
    handle: ?*anyopaque,
    reload_state: ?*anyopaque,
};

var loaded: ?Loaded = null;

fn loaderPanic(operation: []const u8) noreturn {
    const detail = if (dlerror()) |message| std.mem.span(message) else "unknown loader error";
    std.debug.panic("selected native rung {s}: {s}", .{ operation, detail });
}

fn digest(path: []const u8) [std.crypto.hash.sha2.Sha256.digest_length]u8 {
    const file = if (std.fs.path.isAbsolute(path))
        std.fs.openFileAbsolute(path, .{}) catch @panic("selected native rung digest: open failed")
    else
        std.fs.cwd().openFile(path, .{}) catch @panic("selected native rung digest: open failed");
    defer file.close();

    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    var buffer: [16 * 1024]u8 = undefined;
    while (true) {
        const count = file.read(&buffer) catch @panic("selected native rung digest: read failed");
        if (count == 0) break;
        hasher.update(buffer[0..count]);
    }
    var result: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    hasher.final(&result);
    return result;
}

fn assertExplicitName(selection: Selection) void {
    const canonical_name = std.fs.path.basename(selection.canonical_path);
    if (!std.mem.endsWith(u8, canonical_name, ".dylib"))
        @panic("selected native rung canonical artifact is not a dylib");
    const stem = canonical_name[0 .. canonical_name.len - ".dylib".len];
    const expected = std.fmt.allocPrint(
        std.heap.c_allocator,
        "{s}_{s}.dylib",
        .{ stem, @tagName(selection.implementation) },
    ) catch @panic("selected native rung identity: oom");
    defer std.heap.c_allocator.free(expected);
    const actual = std.fs.path.basename(selection.explicit_path);
    if (!std.mem.eql(u8, expected, actual)) {
        std.debug.panic(
            "selected native rung implementation identity mismatch: expected {s}, got {s}",
            .{ expected, actual },
        );
    }
}

/// Load exactly one implementation of one native rung. Repeating the identical
/// selection is idempotent; selecting another implementation or rung aborts
/// before opening the second dylib.
pub fn load(api: *idapi.IdApi, selection: Selection) *idapi.IdApi {
    if (loaded) |active| {
        if (!std.mem.eql(u8, active.rung, selection.rung) or
            active.implementation != selection.implementation or
            !std.mem.eql(u8, active.canonical_path, selection.canonical_path) or
            !std.mem.eql(u8, active.explicit_path, selection.explicit_path))
        {
            @panic("singleton native rung implementation co-existence refused");
        }
        return api;
    }

    assertExplicitName(selection);
    const canonical_digest = digest(selection.canonical_path);
    const explicit_digest = digest(selection.explicit_path);
    if (!std.mem.eql(u8, &canonical_digest, &explicit_digest))
        @panic("selected native rung digest mismatch");

    const handle = dlopen(selection.canonical_path.ptr, RTLD_NOW | RTLD_LOCAL) orelse
        loaderPanic("dlopen failed");
    const raw_init = dlsym(handle, "module_init") orelse
        loaderPanic("missing module_init");
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    const reload_state = init(api, null);
    loaded = .{
        .rung = selection.rung,
        .implementation = selection.implementation,
        .canonical_path = selection.canonical_path,
        .explicit_path = selection.explicit_path,
        .digest = canonical_digest,
        .handle = handle,
        .reload_state = reload_state,
    };
    return api;
}

/// Resolve the selected rung from the clean-process harness environment.
pub fn loadSelected(api: *idapi.IdApi, expected_rung: []const u8) *idapi.IdApi {
    const rung = std.posix.getenv("BIRNAM_NATIVE_RUNG") orelse
        @panic("BIRNAM_NATIVE_RUNG must name exactly one selected rung");
    if (!std.mem.eql(u8, rung, expected_rung))
        @panic("selected native rung identity does not match the test suite");
    const implementation_text = std.posix.getenv("BIRNAM_NATIVE_RUNG_IMPLEMENTATION") orelse
        @panic("BIRNAM_NATIVE_RUNG_IMPLEMENTATION is required");
    const implementation = std.meta.stringToEnum(Implementation, implementation_text) orelse
        @panic("unknown selected native rung implementation");
    const canonical_path = std.posix.getenv("BIRNAM_NATIVE_RUNG_DYLIB") orelse
        @panic("BIRNAM_NATIVE_RUNG_DYLIB is required");
    const explicit_path = std.posix.getenv("BIRNAM_NATIVE_RUNG_EXPLICIT_DYLIB") orelse
        @panic("BIRNAM_NATIVE_RUNG_EXPLICIT_DYLIB is required");
    return load(api, .{
        .rung = rung,
        .implementation = implementation,
        .canonical_path = canonical_path,
        .explicit_path = explicit_path,
    });
}

pub fn retainedHandle() ?*anyopaque {
    return if (loaded) |active| active.handle else null;
}
