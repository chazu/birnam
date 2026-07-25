//! Cross-language smoke for the first Rust N7.1 primitive-prelude slice.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const artifact = @import("artifact_inspector.zig");
const store = @import("store.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const codegen = @import("codegen.zig");
const collection = @import("collection.zig");
const errmod = @import("error.zig");
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
var loop_error: Oop = undefined;
var condition_calls: usize = 0;
var body_calls: usize = 0;
var integer_loop_sum: i64 = 0;

fn loadModule(api: *idapi.IdApi, path: []const u8) !void {
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.ModuleDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.ModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

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

fn loopBodyError(_self: Oop, _args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _args;
    _ = _count;
    return loop_error;
}

fn loopConditionError(_self: Oop, _args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _args;
    _ = _count;
    return loop_error;
}

fn collectInteger(_self: Oop, args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _count;
    integer_loop_sum += floor.api().smallIntValue(args[0]);
    return floor.api().nilObj;
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

    // Error owns its representation and the Object DNU swap. The Rust Prelude
    // must consume that standalone rung rather than minting a private family.
    const error_path = std.posix.getenv("BIRNAM_ERROR_DYLIB") orelse
        return error.MissingErrorDylib;
    const error_path_z = try std.heap.c_allocator.dupeZ(u8, error_path);
    defer std.heap.c_allocator.free(error_path_z);
    const error_handle = api.dlopen(error_path_z.ptr, 1) orelse return error.ErrorDlopenFailed;
    const error_raw_init = api.dlsym(error_handle, "module_init") orelse
        return error.ErrorModuleInitMissing;
    const error_init: ModuleInit = @ptrCast(@alignCast(error_raw_init));
    _ = error_init(api, null);
    try std.testing.expect(api.import("Error") != null);
    if (std.posix.getenv("BIRNAM_ERROR_EXPECTED_IDENTITY")) |expected| {
        const raw_error_identity = api.dlsym(error_handle, "birnam_error_rust_identity") orelse
            return error.ErrorIdentityMissing;
        const error_identity: Identity = @ptrCast(@alignCast(raw_error_identity));
        try std.testing.expectEqualStrings(expected, std.mem.span(error_identity()));
    }

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

    integer_loop_sum = 0;
    const loop_vt = api.proto(api.object_vt);
    api.method(loop_vt, api.intern("value:"), &collectInteger);
    const loop_block = api.alloc(loop_vt, 0);
    const loop_result = send(api, three, "to:do:", &.{ api.mkSmallInt(5), loop_block });
    try std.testing.expectEqual(three, loop_result);
    try std.testing.expectEqual(@as(i64, 12), integer_loop_sum);

    const word = api.mkWord(0xDEAD_BEEF_CAFE_F00D);
    try std.testing.expect(api.isWord(word) != 0);
    var word_text_len: usize = 0;
    const word_text = api.stringBytes(send(api, word, "printString", &.{}), &word_text_len);
    try std.testing.expectEqualStrings("0xdeadbeefcafef00d", word_text[0..word_text_len]);
    const word_equal = send(api, word, "=", &.{api.mkWord(0xDEAD_BEEF_CAFE_F00D)});
    const word_not_equal = send(api, word, "=", &.{three});
    const word_null = send(api, api.mkWord(0), "isNull", &.{});
    try std.testing.expectEqual(api.trueObj, word_equal);
    try std.testing.expectEqual(api.falseObj, word_not_equal);
    try std.testing.expectEqual(api.trueObj, word_null);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(send(api, api.mkWord(42), "asInteger", &.{})));
    const word_overflow = send(api, word, "asInteger", &.{});
    try std.testing.expectEqual(api.intern("overflow"), send(api, word_overflow, "kind", &.{}));

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
    const zero_divide_kind = api.intern("zeroDivide");
    const integer_zero_divide = send(api, three, "/", &.{api.mkSmallInt(0)});
    try std.testing.expectEqual(api.trueObj, send(api, integer_zero_divide, "isError", &.{}));
    try std.testing.expectEqual(zero_divide_kind, send(api, integer_zero_divide, "kind", &.{}));
    var integer_zero_message_len: usize = 0;
    const integer_zero_message = api.stringBytes(
        send(api, integer_zero_divide, "message", &.{}),
        &integer_zero_message_len,
    );
    try std.testing.expectEqualStrings("division by zero", integer_zero_message[0..integer_zero_message_len]);
    const integer_mod_zero = send(api, three, "mod:", &.{api.mkSmallInt(0)});
    try std.testing.expectEqual(zero_divide_kind, send(api, integer_mod_zero, "kind", &.{}));
    const mixed_zero_divide = send(api, two_point_five, "/", &.{api.mkSmallInt(0)});
    try std.testing.expectEqual(zero_divide_kind, send(api, mixed_zero_divide, "kind", &.{}));
    const integer_float_zero_divide = send(api, three, "/", &.{api.mkFloat(0.0)});
    try std.testing.expectEqual(zero_divide_kind, send(api, integer_float_zero_divide, "kind", &.{}));
    const float_zero_divide = send(api, two_point_five, "/", &.{api.mkFloat(0.0)});
    try std.testing.expectEqual(zero_divide_kind, send(api, float_zero_divide, "kind", &.{}));
    const overflow = send(api, api.mkSmallInt(1 << 61), "*", &.{api.mkSmallInt(1 << 61)});
    try std.testing.expectEqual(api.intern("overflow"), send(api, overflow, "kind", &.{}));
    const type_mismatch = send(api, three, "+", &.{api.import("System").?});
    try std.testing.expectEqual(api.intern("typeMismatch"), send(api, type_mismatch, "kind", &.{}));
    const propagated = send(api, three, "+", &.{integer_zero_divide});
    try std.testing.expectEqual(zero_divide_kind, send(api, propagated, "kind", &.{}));
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
    const domain_error = send(api, api.mkFloat(-1.0), "sqrt", &.{});
    try std.testing.expectEqual(api.trueObj, send(api, domain_error, "isError", &.{}));
    try std.testing.expectEqual(api.intern("domain"), send(api, domain_error, "kind", &.{}));

    const rendered = send(api, api.mkSmallInt(-42), "printString", &.{});
    var rendered_len: usize = 0;
    const rendered_bytes = api.stringBytes(rendered, &rendered_len);
    try std.testing.expectEqualStrings("-42", rendered_bytes[0..rendered_len]);
    const float_rendered = send(api, api.mkFloat(4.0), "printString", &.{});
    var float_rendered_len: usize = 0;
    const float_rendered_bytes = api.stringBytes(float_rendered, &float_rendered_len);
    try std.testing.expectEqualStrings("4.0", float_rendered_bytes[0..float_rendered_len]);
    try std.testing.expect(api.isFloat(send(api, three, "asFloat", &.{})) != 0);

    const plain_object_vt = api.proto(api.object_vt);
    const plain_object = api.alloc(plain_object_vt, 0);
    const object_rendered = send(api, plain_object, "printString", &.{});
    var object_rendered_len: usize = 0;
    const object_rendered_bytes = api.stringBytes(object_rendered, &object_rendered_len);
    try std.testing.expectEqualStrings("anObject", object_rendered_bytes[0..object_rendered_len]);
    const object_as_string = send(api, plain_object, "asString", &.{});
    var object_as_string_len: usize = 0;
    const object_as_string_bytes = api.stringBytes(object_as_string, &object_as_string_len);
    try std.testing.expectEqualStrings("anObject", object_as_string_bytes[0..object_as_string_len]);
    const symbol_rendered = send(api, api.intern("rustPrelude"), "printString", &.{});
    var symbol_rendered_len: usize = 0;
    const symbol_rendered_bytes = api.stringBytes(symbol_rendered, &symbol_rendered_len);
    try std.testing.expectEqualStrings("#rustPrelude", symbol_rendered_bytes[0..symbol_rendered_len]);

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
    var saw_read_file = false;
    var saw_write_file = false;
    var saw_get_env = false;
    var saw_load = false;
    var saw_load_module = false;
    var saw_read_line = false;
    var saw_gc = false;
    var saw_live_objects = false;
    var saw_auto_gc = false;
    var saw_precise_gc = false;
    while (api.vtableSelectorAt(system_vt, system_selector_count)) |selector| : (system_selector_count += 1) {
        const name = std.mem.span(api.symbolName(selector));
        saw_print = saw_print or std.mem.eql(u8, name, "print:");
        saw_println = saw_println or std.mem.eql(u8, name, "println:");
        saw_read_file = saw_read_file or std.mem.eql(u8, name, "readFile:");
        saw_write_file = saw_write_file or std.mem.eql(u8, name, "writeFile:contents:");
        saw_get_env = saw_get_env or std.mem.eql(u8, name, "getEnv:");
        saw_load = saw_load or std.mem.eql(u8, name, "load:");
        saw_load_module = saw_load_module or std.mem.eql(u8, name, "loadModule:");
        saw_read_line = saw_read_line or std.mem.eql(u8, name, "readLine");
        saw_gc = saw_gc or std.mem.eql(u8, name, "gc");
        saw_live_objects = saw_live_objects or std.mem.eql(u8, name, "liveObjects");
        saw_auto_gc = saw_auto_gc or std.mem.eql(u8, name, "autoGc:");
        saw_precise_gc = saw_precise_gc or std.mem.eql(u8, name, "preciseGc:");
    }
    try std.testing.expect(saw_print);
    try std.testing.expect(saw_println);
    try std.testing.expect(saw_read_file);
    try std.testing.expect(saw_write_file);
    try std.testing.expect(saw_get_env);
    try std.testing.expect(saw_load);
    try std.testing.expect(saw_load_module);
    try std.testing.expect(saw_read_line);
    try std.testing.expect(saw_gc);
    try std.testing.expect(saw_live_objects);
    try std.testing.expect(saw_auto_gc);
    try std.testing.expect(saw_precise_gc);
    try std.testing.expectEqual(api.nilObj, send(api, system, "print:", &.{}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "print:", &.{ api.trueObj, api.nilObj }));
    try std.testing.expectEqual(api.nilObj, send(api, system, "readFile:", &.{api.trueObj}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "writeFile:contents:", &.{ api.trueObj, api.trueObj }));
    try std.testing.expectEqual(api.nilObj, send(api, system, "getEnv:", &.{api.trueObj}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "load:", &.{api.trueObj}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "loadModule:", &.{api.trueObj}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "readLine", &.{api.trueObj}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "gc", &.{api.trueObj}));
    try std.testing.expect(api.isSmallInt(send(api, system, "gc", &.{})) != 0);
    try std.testing.expect(api.isSmallInt(send(api, system, "liveObjects", &.{})) != 0);
    try std.testing.expectEqual(system, send(api, system, "autoGc:", &.{api.falseObj}));
    try std.testing.expectEqual(system, send(api, system, "autoGc:", &.{api.trueObj}));
    try std.testing.expectEqual(system, send(api, system, "preciseGc:", &.{api.trueObj}));
    try std.testing.expectEqual(system, send(api, system, "preciseGc:", &.{api.falseObj}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "autoGc:", &.{api.nilObj}));
    try std.testing.expectEqual(api.nilObj, send(api, system, "preciseGc:", &.{api.nilObj}));

    const rust_prelude_env_name = "BIRNAM_RUST_PRELUDE_DYLIB";
    const rust_prelude_env = send(
        api,
        system,
        "getEnv:",
        &.{api.mkString(rust_prelude_env_name.ptr, rust_prelude_env_name.len)},
    );
    try std.testing.expect(rust_prelude_env != api.nilObj);
    var rust_prelude_env_len: usize = 0;
    const rust_prelude_env_bytes = api.stringBytes(rust_prelude_env, &rust_prelude_env_len);
    try std.testing.expect(rust_prelude_env_len > 0);
    try std.testing.expect(rust_prelude_env_bytes[0] != 0);
    const rust_prelude_module_path = api.mkString(
        rust_prelude_env_bytes[0..rust_prelude_env_len].ptr,
        rust_prelude_env_len,
    );
    const reload_result = send(api, system, "loadModule:", &.{rust_prelude_module_path});
    try std.testing.expectEqual(system, reload_result);
    const not_a_module_path_bytes = "/definitely/missing/birnam-module.dylib";
    const not_a_module_path = api.mkString(not_a_module_path_bytes.ptr, not_a_module_path_bytes.len);
    const not_a_module_error = send(api, system, "loadModule:", &.{not_a_module_path});
    try std.testing.expectEqual(api.intern("ioError"), send(api, not_a_module_error, "kind", &.{}));
    const missing_env_name = "BIRNAM_RUST_PRELUDE_MISSING";
    try std.testing.expectEqual(
        api.nilObj,
        send(api, system, "getEnv:", &.{api.mkString(missing_env_name.ptr, missing_env_name.len)}),
    );

    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const temporary_directory = try temporary.dir.realpathAlloc(std.heap.c_allocator, ".");
    defer std.heap.c_allocator.free(temporary_directory);
    const temporary_path = try std.fs.path.join(
        std.heap.c_allocator,
        &.{ temporary_directory, "rust-prelude.txt" },
    );
    defer std.heap.c_allocator.free(temporary_path);
    const temporary_path_object = api.mkString(temporary_path.ptr, temporary_path.len);
    const temporary_contents = api.mkString("Rust prelude I/O", 16);
    try std.testing.expectEqual(
        system,
        send(api, system, "writeFile:contents:", &.{ temporary_path_object, temporary_contents }),
    );
    const loaded_contents = send(api, system, "readFile:", &.{temporary_path_object});
    var loaded_contents_len: usize = 0;
    const loaded_contents_bytes = api.stringBytes(loaded_contents, &loaded_contents_len);
    try std.testing.expectEqualStrings("Rust prelude I/O", loaded_contents_bytes[0..loaded_contents_len]);
    const missing_path_bytes = "/definitely/missing/birnam-rust-prelude";
    const missing_path = api.mkString(missing_path_bytes.ptr, missing_path_bytes.len);
    const missing_file_error = send(api, system, "readFile:", &.{missing_path});
    try std.testing.expectEqual(api.intern("ioError"), send(api, missing_file_error, "kind", &.{}));

    const error_factory = api.import("Error");
    const error_vt = api.import("ErrorVt");
    const error_factory_vt = api.import("ErrorFactoryVt");
    try std.testing.expect(error_factory != null);
    try std.testing.expect(error_vt != null);
    try std.testing.expect(error_factory_vt != null);
    const error_kind = api.intern("typeMismatch");
    const error_message = api.mkString("Rust prelude type mismatch", 26);
    const error_object = send(api, error_factory, "kind:message:", &.{ error_kind, error_message });
    try std.testing.expectEqual(error_vt, api.vtableOf(error_object));
    try std.testing.expectEqual(api.trueObj, send(api, error_object, "isError", &.{}));
    try std.testing.expectEqual(error_kind, send(api, error_object, "kind", &.{}));
    try std.testing.expectEqual(error_message, send(api, error_object, "message", &.{}));
    try std.testing.expectEqual(error_object, send(api, error_object, "unknownFromRust", &.{}));
    try std.testing.expectEqual(api.falseObj, send(api, three, "isError", &.{}));
    const kind_only_error = send(api, error_factory, "kind:", &.{error_kind});
    try std.testing.expectEqual(api.trueObj, send(api, kind_only_error, "isError", &.{}));
    loop_error = error_object;

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
    const non_boolean_condition = send(api, condition, "whileTrue:", &.{body});
    try std.testing.expectEqual(api.intern("mustBeBoolean"), send(api, non_boolean_condition, "kind", &.{}));
    try std.testing.expectEqual(@as(usize, 3), condition_calls);
    try std.testing.expectEqual(@as(usize, 2), body_calls);

    const error_body_vt = api.proto(block_vt);
    const error_body = api.alloc(error_body_vt, 0);
    api.method(error_body_vt, api.intern("value"), &loopBodyError);
    condition_calls = 0;
    try std.testing.expectEqual(error_object, send(api, condition, "whileTrue:", &.{error_body}));

    const error_condition_vt = api.proto(block_vt);
    const error_condition = api.alloc(error_condition_vt, 0);
    api.method(error_condition_vt, api.intern("value"), &loopConditionError);
    try std.testing.expectEqual(error_object, send(api, error_condition, "whileTrue:", &.{body}));
}

test "Rust Prelude System load: uses the Codegen source-file protocol" {
    const api = floor.api();

    // Assemble the same dependency order as the host, while keeping the
    // prelude itself as the dynamically loaded Rust candidate under test.
    artifact.bringUp(api);
    store.bringUp(api);
    peg.bringUp(api);
    lang.bringUp(api);
    const tcc_path = std.posix.getenv("BIRNAM_TCC_DYLIB") orelse
        return error.MissingTccDylib;
    try loadModule(api, tcc_path);
    codegen.bringUp(api);
    const rust_prelude_path = std.posix.getenv("BIRNAM_RUST_PRELUDE_DYLIB") orelse
        return error.MissingRustPreludeDylib;
    try loadModule(api, rust_prelude_path);
    collection.bringUp(api);
    errmod.bringUp(api);

    const system = api.import("System") orelse return error.SystemMissing;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const allocator = std.heap.c_allocator;
    const directory = try temporary.dir.realpathAlloc(allocator, ".");
    defer allocator.free(directory);
    const source_path = try std.fs.path.join(allocator, &.{ directory, "load.birnam" });
    defer allocator.free(source_path);
    try temporary.dir.writeFile(.{ .sub_path = "load.birnam", .data = "[1 + 2]" });

    const loaded = send(api, system, "load:", &.{api.mkString(source_path.ptr, source_path.len)});
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(loaded));

    try temporary.dir.writeFile(.{ .sub_path = "broken.birnam", .data = "let x := [3 +" });
    const broken_path = try std.fs.path.join(allocator, &.{ directory, "broken.birnam" });
    defer allocator.free(broken_path);
    const broken = send(api, system, "load:", &.{api.mkString(broken_path.ptr, broken_path.len)});
    try std.testing.expectEqual(api.trueObj, send(api, broken, "isError", &.{}));
    try std.testing.expectEqual(api.intern("syntaxError"), send(api, broken, "kind", &.{}));
}
