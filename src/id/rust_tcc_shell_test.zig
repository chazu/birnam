//! N4.1 cross-language test for the explicit Rust TCC dynamic-rung shell.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

test "Rust TCC shell freezes the current object export boundary" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_TCC_DYLIB") orelse
        return error.MissingRustTccDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustTccDlopenFailed;

    const raw_identity = api.dlsym(handle, "birnam_tcc_rust_identity") orelse
        return error.RustTccIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    try std.testing.expectEqualStrings("rust-tcc-n4.2-state", std.mem.span(identity()));

    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustTccModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const tcc = api.import("Tcc") orelse return error.TccNotExported;
    const tcc_vt = api.import("TccVt") orelse return error.TccVtNotExported;
    const unit_vt = api.import("TccUnitVt") orelse return error.TccUnitVtNotExported;
    try std.testing.expectEqual(tcc_vt, api.vtableOf(tcc));
    try std.testing.expect(api.vtableOf(unit_vt) != null);

    const runtime_exports = [_][*:0]const u8{
        "birnam_send",                "birnam_intern",
        "birnam_mkSmallInt",          "birnam_smallIntValue",
        "birnam_mkFloat",             "birnam_floatValue",
        "birnam_mkWord",              "birnam_wordValue",
        "birnam_uintValue",           "birnam_mkString",
        "birnam_nil",                 "birnam_true",
        "birnam_false",               "birnam_objectVt",
        "birnam_blockVt",             "birnam_proto",
        "birnam_ensurePrivateVtable", "birnam_method",
        "birnam_alloc",               "birnam_cstr",
        "birnam_free_cstr",           "birnam_layoutAllOops",
    };
    for (runtime_exports) |name| {
        _ = api.dlsym(handle, name) orelse return error.RustTccRuntimeExportMissing;
    }
}

test "Rust TCC state machine compiles, relocates, installs, and guards release" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_TCC_DYLIB") orelse
        return error.MissingRustTccDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustTccDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustTccModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const Tcc = api.import("Tcc") orelse return error.TccNotExported;
    const unit = api.send(Tcc, api.intern("new"), &[_]idapi.Oop{}, 0);
    try std.testing.expect(unit != api.nilObj);
    const source = "void* answer(void* self, void** args, long n) { (void)args; (void)n; return self; }";
    const source_object = api.mkString(source.ptr, source.len);
    try std.testing.expectEqual(unit, api.send(unit, api.intern("compile:"), &[_]idapi.Oop{source_object}, 1));
    try std.testing.expectEqual(unit, api.send(unit, api.intern("relocate"), &[_]idapi.Oop{}, 0));
    try std.testing.expectEqual(unit, api.send(unit, api.intern("relocate"), &[_]idapi.Oop{}, 0));

    const vt = api.proto(api.object_vt);
    const selector = api.intern("rustAnswer");
    const installed = api.send(
        unit,
        api.intern("install:as:on:"),
        &[_]idapi.Oop{ api.mkString("answer".ptr, "answer".len), selector, vt },
        3,
    );
    try std.testing.expectEqual(vt, installed);
    const instance = api.alloc(vt, 0);
    try std.testing.expectEqual(instance, api.send(instance, selector, &[_]idapi.Oop{}, 0));

    // Installed native code is live, so the explicit disposable transition is
    // rejected rather than allowing a finalizer to free a callable method.
    try std.testing.expectEqual(api.nilObj, api.send(unit, api.intern("disposable"), &[_]idapi.Oop{}, 0));
}

test "Rust TCC binds generated-C runtime callbacks before relocation" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_TCC_DYLIB") orelse
        return error.MissingRustTccDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustTccDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustTccModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const Tcc = api.import("Tcc") orelse return error.TccNotExported;
    const unit = api.send(Tcc, api.intern("new"), &[_]idapi.Oop{}, 0);
    const source = "extern void* birnam_mkSmallInt(long); void* answer(void* self, void** args, long n) { (void)self; (void)args; (void)n; return birnam_mkSmallInt(123); }";
    const source_object = api.mkString(source.ptr, source.len);
    try std.testing.expectEqual(unit, api.send(unit, api.intern("compile:"), &[_]idapi.Oop{source_object}, 1));
    try std.testing.expectEqual(unit, api.send(unit, api.intern("bindRuntime"), &[_]idapi.Oop{}, 0));
    try std.testing.expectEqual(unit, api.send(unit, api.intern("relocate"), &[_]idapi.Oop{}, 0));

    const vt = api.proto(api.object_vt);
    const selector = api.intern("runtimeAnswer");
    try std.testing.expectEqual(
        vt,
        api.send(
            unit,
            api.intern("install:as:on:"),
            &[_]idapi.Oop{ api.mkString("answer".ptr, "answer".len), selector, vt },
            3,
        ),
    );
    const instance = api.alloc(vt, 0);
    const result = api.send(instance, selector, &[_]idapi.Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 123), api.smallIntValue(result));
}
