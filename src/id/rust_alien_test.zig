//! Focused live-owner proof for the Rust Alien R5 slices.
//!
//! This remains narrower than the full Zig Alien corpus, but covers the live
//! loader, generated wrappers, callbacks, raw buffers, ABI corpus, and
//! kinded hostile-input failures owned by the Rust implementation.

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
const LiveLine = extern struct { base: c_long, values: [3]c_int };
const LiveFloatPair = extern struct { left: f32, right: f32 };
const LiveDoublePair = extern struct { left: f64, right: f64 };
const LiveMixed = extern struct { base: c_long, scale: f64 };
const LiveLarge = extern struct { first: c_long, second: c_long, total: c_long };

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

export fn birnam_rust_alien_identity_long(value: c_long) c_long {
    return value;
}

export fn birnam_rust_alien_identity_pointer(value: ?*anyopaque) ?*anyopaque {
    return value;
}

export fn birnam_rust_alien_sum_double(left: f64, right: f64) f64 {
    return left + right;
}

export fn birnam_rust_alien_void(value: c_long) void {
    _ = value;
}

export fn birnam_rust_alien_sum_line(line: LiveLine) c_long {
    return line.base + line.values[0] + line.values[1] + line.values[2];
}

export fn birnam_rust_alien_sum_float_pair(pair: LiveFloatPair) c_long {
    return @intFromFloat(pair.left + pair.right);
}

export fn birnam_rust_alien_sum_double_pair(pair: LiveDoublePair) f64 {
    return pair.left + pair.right;
}

export fn birnam_rust_alien_sum_mixed(pair: LiveMixed) c_long {
    return pair.base + @as(c_long, @intFromFloat(pair.scale));
}

export fn birnam_rust_alien_make_large(first: c_long, second: c_long) LiveLarge {
    return .{ .first = first, .second = second, .total = first + second };
}

fn retainExportedFixtures() void {
    const fixtures = .{
        &birnam_rust_alien_apply,
        &birnam_rust_alien_apply_point,
        &birnam_rust_alien_identity_long,
        &birnam_rust_alien_identity_pointer,
        &birnam_rust_alien_sum_double,
        &birnam_rust_alien_sum_double_pair,
        &birnam_rust_alien_sum_float_pair,
        &birnam_rust_alien_sum_mixed,
        &birnam_rust_alien_make_large,
        &birnam_rust_alien_sum_line,
        &birnam_rust_alien_void,
    };
    inline for (fixtures) |fixture| std.mem.doNotOptimizeAway(fixture);
}

var retained_handles: [4]?*anyopaque = .{ null, null, null, null };
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
        "BIRNAM_RUST_ERROR_DYLIB",
        "birnam_error_rust_identity",
        "rust-error-r2-core",
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

test "Rust Alien owns raw buffer allocation and typed access" {
    const api = floor.api();
    const alien = api.import("Alien") orelse return error.AlienExportMissing;
    const buffer = api.send(alien, api.intern("alloc:"), &[_]Oop{api.mkSmallInt(64)}, 1);
    try std.testing.expect(api.isWord(buffer) != 0);

    const poke_int = api.send(
        buffer,
        api.intern("pokeInt:at:"),
        &[_]Oop{ api.mkSmallInt(-42), api.mkSmallInt(0) },
        2,
    );
    try std.testing.expect(poke_int == buffer);
    const peek_int = api.send(buffer, api.intern("peekIntAt:"), &[_]Oop{api.mkSmallInt(0)}, 1);
    try std.testing.expectEqual(@as(i64, -42), api.smallIntValue(peek_int));

    _ = api.send(
        buffer,
        api.intern("pokeDouble:at:"),
        &[_]Oop{ api.mkFloat(3.5), api.mkSmallInt(16) },
        2,
    );
    const peek_double = api.send(buffer, api.intern("peekDoubleAt:"), &[_]Oop{api.mkSmallInt(16)}, 1);
    try std.testing.expectEqual(@as(f64, 3.5), api.floatValue(peek_double));

    const text = "hello";
    for (text, 0..) |byte, index| {
        _ = api.send(
            buffer,
            api.intern("pokeByte:at:"),
            &[_]Oop{ api.mkSmallInt(byte), api.mkSmallInt(@intCast(32 + index)) },
            2,
        );
    }
    const copied = api.send(buffer, api.intern("peekStringAt:"), &[_]Oop{api.mkSmallInt(32)}, 1);
    var copied_len: usize = 0;
    const copied_bytes = api.stringBytes(copied, &copied_len);
    try std.testing.expectEqualStrings(text, copied_bytes[0..copied_len]);

    const pointer_value = api.mkWord(@intFromPtr(buffer));
    _ = api.send(
        buffer,
        api.intern("pokePtr:at:"),
        &[_]Oop{ pointer_value, api.mkSmallInt(48) },
        2,
    );
    const peek_pointer = api.send(buffer, api.intern("peekPtrAt:"), &[_]Oop{api.mkSmallInt(48)}, 1);
    try std.testing.expectEqual(api.wordValue(pointer_value), api.wordValue(peek_pointer));

    try std.testing.expect(api.send(buffer, api.intern("free"), &[_]Oop{}, 0) == api.nilObj);
}

fn makeVector(api: *idapi.IdApi, elements: []const Oop) Oop {
    const factory = api.import("Vector") orelse return api.nilObj;
    var result = api.send(factory, api.intern("new"), &[_]Oop{}, 0);
    for (elements) |element| {
        result = api.send(result, api.intern("add:"), &[_]Oop{element}, 1);
    }
    return result;
}

test "Rust Alien live ABI corpus covers scalar, pointer, void, and fixed-array aggregate" {
    const api = floor.api();
    const alien = api.import("Alien") orelse return error.AlienExportMissing;
    const wrap_selector = api.intern("wrap:signature:as:on:");

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_identity_long"), str(api, "l l"), api.intern("rustIdentity"), api.smallint_vt },
        4,
    ) != api.nilObj);
    const identity = api.send(api.mkSmallInt(-9), api.intern("rustIdentity"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, -9), api.smallIntValue(identity));

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_identity_pointer"), str(api, "p p"), api.intern("rustPointer"), api.word_vt },
        4,
    ) != api.nilObj);
    const pointer = api.mkWord(0x1234);
    const pointer_result = api.send(pointer, api.intern("rustPointer"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(u64, 0x1234), api.wordValue(pointer_result));

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_sum_double"), str(api, "d d d"), api.intern("rustAdd:"), api.float_vt },
        4,
    ) != api.nilObj);
    const sum = api.send(api.mkFloat(1.25), api.intern("rustAdd:"), &[_]Oop{api.mkFloat(2.75)}, 1);
    try std.testing.expectEqual(@as(f64, 4.0), api.floatValue(sum));

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_void"), str(api, "v l"), api.intern("rustVoid"), api.smallint_vt },
        4,
    ) != api.nilObj);
    try std.testing.expect(api.send(api.mkSmallInt(3), api.intern("rustVoid"), &[_]Oop{}, 0) == api.nilObj);

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_sum_line"), str(api, "l {l3i}"), api.intern("rustLine"), api.import("VectorVt").? },
        4,
    ) != api.nilObj);
    const line = makeVector(api, &[_]Oop{
        api.mkSmallInt(7),
        makeVector(api, &[_]Oop{ api.mkSmallInt(1), api.mkSmallInt(2), api.mkSmallInt(3) }),
    });
    const line_sum = api.send(line, api.intern("rustLine"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 13), api.smallIntValue(line_sum));
}

test "Rust Alien live ABI corpus covers HFA, mixed, and large aggregates" {
    const api = floor.api();
    const alien = api.import("Alien") orelse return error.AlienExportMissing;
    const wrap_selector = api.intern("wrap:signature:as:on:");
    const vector_vt = api.import("VectorVt").?;

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_sum_float_pair"), str(api, "l {ff}"), api.intern("rustFloatPair"), vector_vt },
        4,
    ) != api.nilObj);
    const float_sum = api.send(
        makeVector(api, &[_]Oop{ api.mkFloat(1.25), api.mkFloat(2.75) }),
        api.intern("rustFloatPair"),
        &[_]Oop{},
        0,
    );
    try std.testing.expectEqual(@as(i64, 4), api.smallIntValue(float_sum));

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_sum_double_pair"), str(api, "d {dd}"), api.intern("rustDoublePair"), vector_vt },
        4,
    ) != api.nilObj);
    const double_sum = api.send(
        makeVector(api, &[_]Oop{ api.mkFloat(1.5), api.mkFloat(2.25) }),
        api.intern("rustDoublePair"),
        &[_]Oop{},
        0,
    );
    try std.testing.expectEqual(@as(f64, 3.75), api.floatValue(double_sum));

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_sum_mixed"), str(api, "l {ld}"), api.intern("rustMixed"), vector_vt },
        4,
    ) != api.nilObj);
    const mixed_sum = api.send(
        makeVector(api, &[_]Oop{ api.mkSmallInt(7), api.mkFloat(2.5) }),
        api.intern("rustMixed"),
        &[_]Oop{},
        0,
    );
    try std.testing.expectEqual(@as(i64, 9), api.smallIntValue(mixed_sum));

    try std.testing.expect(api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_make_large"), str(api, "{lll} l l"), api.intern("rustLarge:"), api.smallint_vt },
        4,
    ) != api.nilObj);
    const large = api.send(
        api.mkSmallInt(3),
        api.intern("rustLarge:"),
        &[_]Oop{api.mkSmallInt(4)},
        1,
    );
    try std.testing.expect(api.vtableOf(large) == vector_vt);
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(api.send(
        large,
        api.intern("at:"),
        &[_]Oop{api.mkSmallInt(0)},
        1,
    )));
    try std.testing.expectEqual(@as(i64, 4), api.smallIntValue(api.send(
        large,
        api.intern("at:"),
        &[_]Oop{api.mkSmallInt(1)},
        1,
    )));
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(api.send(
        large,
        api.intern("at:"),
        &[_]Oop{api.mkSmallInt(2)},
        1,
    )));
}

fn isError(api: *idapi.IdApi, value: Oop) bool {
    return api.send(value, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj;
}

fn errorKind(api: *idapi.IdApi, value: Oop) Oop {
    return api.send(value, api.intern("kind"), &[_]Oop{}, 0);
}

test "Rust Alien hostile signatures fail closed with kinded errors" {
    const api = floor.api();
    const alien = api.import("Alien") orelse return error.AlienExportMissing;
    const wrap_selector = api.intern("wrap:signature:as:on:");
    const object_vt = api.proto(api.object_vt);

    const malformed = [_][]const u8{ "l q", "", "l {ll", "l {s}", "l 3i" };
    for (malformed, 0..) |signature, index| {
        var selector_buf: [32]u8 = undefined;
        const selector = try std.fmt.bufPrintZ(&selector_buf, "badRustAlien{d}", .{index});
        const result = api.send(
            alien,
            wrap_selector,
            &[_]Oop{ str(api, "strlen"), str(api, signature), api.intern(selector), object_vt },
            4,
        );
        try std.testing.expect(isError(api, result));
        try std.testing.expectEqual(api.intern("badSignature"), errorKind(api, result));
    }

    const missing = api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "birnam_rust_alien_missing"), str(api, "l l"), api.intern("missingRustAlien"), object_vt },
        4,
    );
    try std.testing.expect(isError(api, missing));
    try std.testing.expectEqual(api.intern("symbolNotFound"), errorKind(api, missing));

    const embedded_nul = api.send(
        alien,
        wrap_selector,
        &[_]Oop{ str(api, "strlen\x00ignored"), str(api, "l s"), api.intern("nulRustAlien"), object_vt },
        4,
    );
    try std.testing.expect(isError(api, embedded_nul));
    try std.testing.expectEqual(api.intern("typeMismatch"), errorKind(api, embedded_nul));
}

test "Rust Alien hostile callback signatures fail closed with kinded errors" {
    const api = floor.api();
    const alien = api.import("Alien") orelse return error.AlienExportMissing;
    const responder = liveResponder(api, "value:", &liveDouble);
    const callback_selector = api.intern("callback:signature:");
    const malformed = [_][]const u8{ "s l", "q l", "", "l {ll" };

    for (malformed) |signature| {
        const result = api.send(
            alien,
            callback_selector,
            &[_]Oop{ responder, str(api, signature) },
            2,
        );
        try std.testing.expect(isError(api, result));
        try std.testing.expectEqual(api.intern("badSignature"), errorKind(api, result));
    }

    const nil_block = api.send(
        alien,
        callback_selector,
        &[_]Oop{ api.nilObj, str(api, "l l") },
        2,
    );
    try std.testing.expect(isError(api, nil_block));
    try std.testing.expectEqual(api.intern("typeMismatch"), errorKind(api, nil_block));
}

test "Rust Alien refuses to unload a library retained by a live wrapper" {
    const api = floor.api();
    const alien = api.import("Alien") orelse return error.AlienExportMissing;
    const system_name = if (@import("builtin").os.tag == .macos)
        "libSystem.B.dylib"
    else
        "libc.so.6";

    try std.testing.expect(api.send(
        alien,
        api.intern("wrap:signature:as:on:"),
        &[_]Oop{ str(api, "strlen"), str(api, "l s"), api.intern("unloadGuard"), api.string_vt },
        4,
    ) != api.nilObj);

    try std.testing.expect(
        api.send(alien, api.intern("dlopen:"), &[_]Oop{str(api, system_name)}, 1) == api.trueObj,
    );
    const close = api.send(alien, api.intern("dlclose:"), &[_]Oop{str(api, system_name)}, 1);
    try std.testing.expect(isError(api, close));
    try std.testing.expectEqual(api.intern("inUse"), errorKind(api, close));
}
