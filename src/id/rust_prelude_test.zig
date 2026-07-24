//! Cross-language smoke for the first Rust N7.1 primitive-prelude slice.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

fn valueSelf(self: Oop, _args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _args;
    _ = _count;
    return self;
}

var loop_true: Oop = undefined;
var loop_nil: Oop = undefined;
var condition_calls: usize = 0;
var body_calls: usize = 0;

fn loopCondition(_self: Oop, _args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _args;
    _ = _count;
    condition_calls += 1;
    return if (condition_calls < 3) loop_true else loop_nil;
}

fn loopBody(_self: Oop, _args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _args;
    _ = _count;
    body_calls += 1;
    return loop_nil;
}

test "Rust Prelude N7.1 installs integer protocol through a dynamic rung" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_PRELUDE_DYLIB") orelse
        return error.MissingRustPreludeDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustPreludeDlopenFailed;

    const raw_identity = api.dlsym(handle, "birnam_prelude_rust_identity") orelse
        return error.RustPreludeIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    try std.testing.expectEqualStrings("rust-prelude-n7.1-core", std.mem.span(identity()));

    const raw_init = api.dlsym(handle, "module_init") orelse
        return error.RustPreludeModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const three = api.mkSmallInt(3);
    const five = api.mkSmallInt(5);
    try std.testing.expectEqual(@as(i64, 8), api.smallIntValue(send(api, three, "+", &.{five})));
    try std.testing.expectEqual(@as(i64, -2), api.smallIntValue(send(api, three, "-", &.{five})));
    try std.testing.expectEqual(@as(i64, 15), api.smallIntValue(send(api, three, "*", &.{five})));
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, three, "/", &.{five})));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, api.mkSmallInt(13), "mod:", &.{five})));
    try std.testing.expectEqual(api.trueObj, send(api, three, "<", &.{five}));
    try std.testing.expectEqual(api.trueObj, send(api, five, ">=", &.{three}));
    try std.testing.expectEqual(api.trueObj, send(api, three, "=", &.{api.mkSmallInt(3)}));

    const two_point_five = api.mkFloat(2.5);
    const mixed_sum = send(api, three, "+", &.{two_point_five});
    try std.testing.expect(api.isFloat(mixed_sum) != 0);
    try std.testing.expectApproxEqAbs(@as(f64, 5.5), api.floatValue(mixed_sum), 0.000001);
    try std.testing.expectApproxEqAbs(
        @as(f64, -0.5),
        api.floatValue(send(api, two_point_five, "-", &.{three})),
        0.000001,
    );
    try std.testing.expectApproxEqAbs(
        @as(f64, 7.5),
        api.floatValue(send(api, two_point_five, "*", &.{three})),
        0.000001,
    );
    try std.testing.expectApproxEqAbs(
        @as(f64, 1.25),
        api.floatValue(send(api, two_point_five, "/", &.{api.mkSmallInt(2)})),
        0.000001,
    );
    try std.testing.expectEqual(api.nilObj, send(api, two_point_five, "/", &.{api.mkSmallInt(0)}));
    try std.testing.expectEqual(api.falseObj, send(api, two_point_five, "=", &.{api.mkSmallInt(5)}));
    try std.testing.expectEqual(api.falseObj, send(api, two_point_five, ">", &.{three}));
    try std.testing.expectEqual(api.trueObj, send(api, two_point_five, "<", &.{api.mkFloat(3.0)}));

    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(send(api, api.mkFloat(2.9), "asInteger", &.{})));
    try std.testing.expectEqual(@as(i64, -2), api.smallIntValue(send(api, api.mkFloat(-1.2), "floor", &.{})));
    try std.testing.expectEqual(@as(i64, -1), api.smallIntValue(send(api, api.mkFloat(-1.2), "ceiling", &.{})));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, api.mkFloat(2.5), "rounded", &.{})));
    try std.testing.expectApproxEqAbs(
        @as(f64, 1.5),
        api.floatValue(send(api, api.mkFloat(-1.5), "abs", &.{})),
        0.000001,
    );
    try std.testing.expectApproxEqAbs(
        @as(f64, 3.0),
        api.floatValue(send(api, api.mkFloat(9.0), "sqrt", &.{})),
        0.000001,
    );
    try std.testing.expectEqual(api.nilObj, send(api, api.mkFloat(-1.0), "sqrt", &.{}));

    const rendered = send(api, api.mkSmallInt(-42), "printString", &.{});
    var rendered_len: usize = 0;
    const rendered_bytes = api.stringBytes(rendered, &rendered_len);
    try std.testing.expectEqualStrings("-42", rendered_bytes[0..rendered_len]);
    const float_rendered = send(api, api.mkFloat(4.0), "printString", &.{});
    var float_rendered_len: usize = 0;
    const float_rendered_bytes = api.stringBytes(float_rendered, &float_rendered_len);
    try std.testing.expectEqualStrings("4.0", float_rendered_bytes[0..float_rendered_len]);
    try std.testing.expect(api.isFloat(send(api, three, "asFloat", &.{})) != 0);

    const branch_vt = api.proto(api.object_vt);
    const branch = api.alloc(branch_vt, 0);
    api.method(branch_vt, api.intern("value"), &valueSelf);
    try std.testing.expectEqual(branch, send(api, api.trueObj, "ifTrue:ifFalse:", &.{ branch, api.nilObj }));
    try std.testing.expectEqual(api.nilObj, send(api, api.falseObj, "ifTrue:", &.{branch}));
    try std.testing.expectEqual(branch, send(api, api.falseObj, "ifFalse:", &.{branch}));
    try std.testing.expectEqual(api.falseObj, send(api, api.falseObj, "&", &.{api.trueObj}));
    try std.testing.expectEqual(api.trueObj, send(api, api.trueObj, "|", &.{api.falseObj}));
    try std.testing.expectEqual(branch, send(api, api.trueObj, "and:", &.{branch}));
    try std.testing.expectEqual(api.falseObj, send(api, api.falseObj, "and:", &.{branch}));
    try std.testing.expectEqual(api.trueObj, send(api, api.trueObj, "or:", &.{branch}));
    try std.testing.expectEqual(branch, send(api, api.falseObj, "or:", &.{branch}));
    try std.testing.expectEqual(api.falseObj, send(api, api.trueObj, "not", &.{}));
    try std.testing.expectEqual(api.trueObj, send(api, api.falseObj, "not", &.{}));
    const true_rendered = send(api, api.trueObj, "printString", &.{});
    var true_rendered_len: usize = 0;
    const true_rendered_bytes = api.stringBytes(true_rendered, &true_rendered_len);
    try std.testing.expectEqualStrings("true", true_rendered_bytes[0..true_rendered_len]);

    const system = api.import("System");
    try std.testing.expect(system != null);
    const system_vt = api.import("SystemVt");
    try std.testing.expect(system_vt != null);
    var system_selector_count: usize = 0;
    var saw_print = false;
    var saw_println = false;
    while (api.vtableSelectorAt(system_vt, system_selector_count)) |selector| : (system_selector_count += 1) {
        const name = std.mem.span(api.symbolName(selector));
        saw_print = saw_print or std.mem.eql(u8, name, "print:");
        saw_println = saw_println or std.mem.eql(u8, name, "println:");
    }
    try std.testing.expect(saw_print);
    try std.testing.expect(saw_println);
    try std.testing.expectEqual(api.nilObj, send(api, system, "print:", &.{}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "print:", &.{api.trueObj}));

    loop_true = api.trueObj;
    loop_nil = api.nilObj;
    condition_calls = 0;
    body_calls = 0;
    const block_vt = api.import("BlockVt");
    const condition_vt = api.proto(block_vt);
    const body_vt = api.proto(block_vt);
    const condition = api.alloc(condition_vt, 0);
    const body = api.alloc(body_vt, 0);
    api.method(condition_vt, api.intern("value"), &loopCondition);
    api.method(body_vt, api.intern("value"), &loopBody);
    try std.testing.expectEqual(api.nilObj, send(api, condition, "whileTrue:", &.{body}));
    try std.testing.expectEqual(@as(usize, 3), condition_calls);
    try std.testing.expectEqual(@as(usize, 2), body_calls);
}
