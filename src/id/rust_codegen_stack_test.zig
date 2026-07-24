//! Exercise the Rust Codegen ↔ Rust Store callback contract without a Zig
//! Codegen export. This is deliberately a separate process from the mixed
//! runtime suite so the Store single-owner law is tested on both sides.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const collection = @import("collection.zig");
const lang = @import("lang.zig");
const idapi = @import("idapi.zig");

const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

fn load(api: *idapi.IdApi, variable: []const u8) !void {
    const path = std.posix.getenv(variable) orelse return error.MissingDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.DlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.ModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

fn setup() !*idapi.IdApi {
    const api = floor.api();
    try load(api, "BIRNAM_RUST_PEG_DYLIB");
    lang.bringUp(api);
    try load(api, "BIRNAM_RUST_TCC_DYLIB");
    // Store bring-up resets its private callback slots. Keep the first loaded
    // provider alive across the two tests so Rust Codegen's single-owner
    // registration remains attached to the same Store instance.
    if (api.isExported("Store") == 0) try load(api, "BIRNAM_CALLBACK_STORE_DYLIB");
    collection.bringUp(api);
    try load(api, "BIRNAM_RUST_CODEGEN_DYLIB");
    // Do not bring up Zig Codegen here. Rust Codegen must become the sole
    // Store callback owner in this process.
    return api;
}

fn probeAnswer(_: idapi.Oop, _: [*]const idapi.Oop, _: usize) callconv(.c) idapi.Oop {
    return @ptrFromInt(77);
}

test "Rust Codegen owns Rust Store callbacks after collection" {
    const api = try setup();
    const rust_codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const store = api.import("Store") orelse return error.MissingStore;

    // Rust-only hosts make callback ownership explicit after the complete
    // stack is selected. Mixed hosts omit this call and retain Zig Codegen's
    // already-registered Store owner.
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("enterRunning"), &.{}, 0));

    // The lifecycle transition installs Rust Codegen as the single Store
    // observer and raw authorizer. Force collection before invoking either callback: the
    // callback receiver and candidate registry must remain rooted by the Rust
    // provider/export environment, not by a transient Rust or Zig stack frame.
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = api.gcCollect();

    const integer_vt = api.vtableOf(api.mkSmallInt(3));
    const selector = api.intern("rustOnlyCallbackProbe");
    const method: idapi.Oop = @ptrCast(@constCast(&probeAnswer));

    // Running-phase mutation is denied by the Rust authorizer before either
    // dictionary or Store mirror changes.
    try std.testing.expectEqual(
        api.nilObj,
        api.send(store, api.intern("addMethod:on:with:"), &.{ selector, integer_vt, method }, 3),
    );
    try std.testing.expectEqual(
        null,
        api.send(store, api.intern("rawCurrentValue:attr:"), &.{ integer_vt, selector }, 2),
    );

    // Quiescence authorizes the same mutation; completion calls back into the
    // surviving Rust observer, then dispatch observes the exact installed fn.
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0));
    try std.testing.expectEqual(
        store,
        api.send(store, api.intern("addMethod:on:with:"), &.{ selector, integer_vt, method }, 3),
    );
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    try std.testing.expectEqual(@as(usize, 77), @intFromPtr(api.send(api.mkSmallInt(3), selector, &.{}, 0)));
}

test "Rust Codegen candidate uses Rust-owned shims and reenters Store" {
    const api = try setup();
    const rust_codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const store = api.import("Store") orelse return error.MissingStore;
    const object = api.import("Object") orelse return error.MissingObject;

    // Tests share the process-lifetime Rust runtime. Normalize either a fresh
    // bootstrapping phase or the running phase left by the callback test.
    if (api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0) == api.trueObj) {
        try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    } else {
        try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("enterRunning"), &.{}, 0));
    }
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const path = try std.fmt.allocPrint(std.testing.allocator, "{s}/rust_only_drift.dylib", .{directory});
    defer std.testing.allocator.free(path);
    const source_text = "[Module named: #rustOnlyDrift requires: #[]]; [Reflect defineMethod: #rustOnlyDriftAnswer on: Integer as: [:self | 71]]";
    const source = api.mkString(source_text.ptr, source_text.len);
    const output = api.mkString(path.ptr, path.len);
    try std.testing.expect(api.send(
        rust_codegen,
        api.intern("compileCandidate:named:output:"),
        &.{ source, api.intern("rustOnlyDrift"), output },
        3,
    ) != api.nilObj);

    const unit = api.send(object, api.intern("clone"), &.{}, 0);
    try std.testing.expectEqual(unit, api.send(
        rust_codegen,
        api.intern("inspectCandidate:into:"),
        &.{ output, unit },
        2,
    ));
    try std.testing.expectEqual(unit, api.send(rust_codegen, api.intern("mapCandidate:"), &.{unit}, 1));
    const token = api.send(rust_codegen, api.intern("prepareActivation:"), &.{unit}, 1);
    try std.testing.expect(token != api.nilObj);
    try std.testing.expectEqual(unit, api.send(rust_codegen, api.intern("commitPrepared:"), &.{token}, 1));
    try std.testing.expectEqual(
        api.intern("active"),
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2),
    );
    try std.testing.expectEqual(@as(i64, 71), api.smallIntValue(api.send(api.mkSmallInt(3), api.intern("rustOnlyDriftAnswer"), &.{}, 0)));

    // The external mutation synchronously reenters Rust Codegen's observer,
    // whose rawCurrentValue:attr: send observes the replacement before the
    // candidate is marked drifted and rollback is disabled.
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0));
    const integer_vt = api.vtableOf(api.mkSmallInt(3));
    const selector = api.intern("rustOnlyDriftAnswer");
    const drift_method: idapi.Oop = @ptrCast(@constCast(&probeAnswer));
    try std.testing.expectEqual(
        store,
        api.send(store, api.intern("addMethod:on:with:"), &.{ selector, integer_vt, drift_method }, 3),
    );
    try std.testing.expectEqual(
        api.intern("drifted"),
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2),
    );
    try std.testing.expectEqual(
        api.falseObj,
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("rollbackAvailable") }, 2),
    );
    try std.testing.expectEqual(api.nilObj, api.send(rust_codegen, api.intern("prepareRollback:"), &.{unit}, 1));
    try std.testing.expectEqual(@as(usize, 77), @intFromPtr(api.send(api.mkSmallInt(3), selector, &.{}, 0)));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
}

test "Rust Codegen candidate rollback restores the retained predecessor" {
    const api = try setup();
    const rust_codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const store = api.import("Store") orelse return error.MissingStore;
    const object = api.import("Object") orelse return error.MissingObject;

    if (api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0) == api.trueObj) {
        try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    } else {
        try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("enterRunning"), &.{}, 0));
    }
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const selector = api.intern("rustOnlyRollbackAnswer");
    const removed_selector = api.intern("rustOnlyRollbackRemoved");

    const base_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/rust_only_rollback_base.dylib", .{directory});
    defer std.testing.allocator.free(base_path);
    const base_text = "[Module named: #rustOnlyRollbackBase requires: #[]]; [Reflect defineMethod: #rustOnlyRollbackAnswer on: Integer as: [:self | 81]]; [Reflect defineMethod: #rustOnlyRollbackRemoved on: Integer as: [:self | 91]]";
    const base_source = api.mkString(base_text.ptr, base_text.len);
    const base_output = api.mkString(base_path.ptr, base_path.len);
    try std.testing.expect(api.send(
        rust_codegen,
        api.intern("compileCandidate:named:output:"),
        &.{ base_source, api.intern("rustOnlyRollbackBase"), base_output },
        3,
    ) != api.nilObj);
    const base_unit = api.send(object, api.intern("clone"), &.{}, 0);
    try std.testing.expectEqual(base_unit, api.send(rust_codegen, api.intern("inspectCandidate:into:"), &.{ base_output, base_unit }, 2));
    try std.testing.expectEqual(base_unit, api.send(rust_codegen, api.intern("mapCandidate:"), &.{base_unit}, 1));
    const base_token = api.send(rust_codegen, api.intern("prepareActivation:"), &.{base_unit}, 1);
    try std.testing.expect(base_token != api.nilObj);
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0));
    try std.testing.expectEqual(base_unit, api.send(rust_codegen, api.intern("commitPrepared:"), &.{base_token}, 1));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    try std.testing.expectEqual(@as(i64, 81), api.smallIntValue(api.send(api.mkSmallInt(3), selector, &.{}, 0)));
    try std.testing.expectEqual(@as(i64, 91), api.smallIntValue(api.send(api.mkSmallInt(3), removed_selector, &.{}, 0)));

    const successor_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/rust_only_rollback_successor.dylib", .{directory});
    defer std.testing.allocator.free(successor_path);
    const successor_text = "[Module named: #rustOnlyRollbackSuccessor requires: #[]]; [Reflect defineMethod: #rustOnlyRollbackAnswer on: Integer as: [:self | 82]]; [Reflect removeMethod: #rustOnlyRollbackRemoved on: Integer]";
    const successor_source = api.mkString(successor_text.ptr, successor_text.len);
    const successor_output = api.mkString(successor_path.ptr, successor_path.len);
    try std.testing.expect(api.send(
        rust_codegen,
        api.intern("compileCandidate:named:output:"),
        &.{ successor_source, api.intern("rustOnlyRollbackSuccessor"), successor_output },
        3,
    ) != api.nilObj);
    const successor_unit = api.send(object, api.intern("clone"), &.{}, 0);
    try std.testing.expectEqual(successor_unit, api.send(rust_codegen, api.intern("inspectCandidate:into:"), &.{ successor_output, successor_unit }, 2));
    try std.testing.expectEqual(successor_unit, api.send(rust_codegen, api.intern("mapCandidate:"), &.{successor_unit}, 1));
    const successor_token = api.send(rust_codegen, api.intern("prepareActivation:prior:"), &.{ successor_unit, base_unit }, 2);
    try std.testing.expect(successor_token != api.nilObj);
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0));
    try std.testing.expectEqual(successor_unit, api.send(rust_codegen, api.intern("commitPrepared:"), &.{successor_token}, 1));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    try std.testing.expectEqual(@as(i64, 82), api.smallIntValue(api.send(api.mkSmallInt(3), selector, &.{}, 0)));
    try std.testing.expectEqual(
        null,
        api.send(store, api.intern("rawCurrentValue:attr:"), &.{ api.vtableOf(api.mkSmallInt(3)), removed_selector }, 2),
    );

    // The active successor's block capture must survive collection until the
    // rollback path restores the predecessor's exact Store method carrier.
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = api.gcCollect();
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0));
    const rollback_token = api.send(rust_codegen, api.intern("prepareRollback:"), &.{successor_unit}, 1);
    try std.testing.expect(rollback_token != api.nilObj);
    try std.testing.expectEqual(successor_unit, api.send(rust_codegen, api.intern("commitPreparedRollback:"), &.{rollback_token}, 1));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    try std.testing.expectEqual(@as(i64, 81), api.smallIntValue(api.send(api.mkSmallInt(3), selector, &.{}, 0)));
    try std.testing.expectEqual(api.intern("active"), api.send(store, api.intern("currentValue:attr:"), &.{ base_unit, api.intern("state") }, 2));
    try std.testing.expectEqual(api.intern("rolledBack"), api.send(store, api.intern("currentValue:attr:"), &.{ successor_unit, api.intern("state") }, 2));
    try std.testing.expectEqual(@as(i64, 91), api.smallIntValue(api.send(api.mkSmallInt(3), removed_selector, &.{}, 0)));
}

test "Rust Codegen discards a prepared candidate without publication" {
    const api = try setup();
    const rust_codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const store = api.import("Store") orelse return error.MissingStore;
    const object = api.import("Object") orelse return error.MissingObject;

    if (api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0) == api.trueObj) {
        try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    } else {
        try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("enterRunning"), &.{}, 0));
    }
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const path = try std.fmt.allocPrint(std.testing.allocator, "{s}/rust_only_discard.dylib", .{directory});
    defer std.testing.allocator.free(path);
    const source_text = "[Module named: #rustOnlyDiscard requires: #[]]; [Reflect defineMethod: #rustOnlyDiscardAnswer on: Integer as: [:self | 88]]";
    const source = api.mkString(source_text.ptr, source_text.len);
    const output = api.mkString(path.ptr, path.len);
    try std.testing.expect(api.send(
        rust_codegen,
        api.intern("compileCandidate:named:output:"),
        &.{ source, api.intern("rustOnlyDiscard"), output },
        3,
    ) != api.nilObj);

    const unit = api.send(object, api.intern("clone"), &.{}, 0);
    try std.testing.expectEqual(unit, api.send(rust_codegen, api.intern("inspectCandidate:into:"), &.{ output, unit }, 2));
    try std.testing.expectEqual(unit, api.send(rust_codegen, api.intern("mapCandidate:"), &.{unit}, 1));
    const token = api.send(rust_codegen, api.intern("prepareActivation:"), &.{unit}, 1);
    try std.testing.expect(token != api.nilObj);
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("discardPrepared:"), &.{token}, 1));
    try std.testing.expectEqual(api.nilObj, api.send(rust_codegen, api.intern("commitPrepared:"), &.{token}, 1));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("candidateMapped:"), &.{unit}, 1));
    try std.testing.expectEqual(
        api.nilObj,
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2),
    );
}
