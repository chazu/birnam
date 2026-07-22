//! External R3.1 canonical-environment and loader/reload fixture.

const std = @import("std");
const idapi = @import("idapi");

const FixtureInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const FixtureValue = *const fn (?*anyopaque) callconv(.c) usize;
const Identity = *const fn () callconv(.c) [*:0]const u8;
const RTLD_LAZY: c_int = 1;

fn require(condition: bool, failure: anyerror) !void {
    if (!condition) return failure;
}

fn function(comptime T: type, address: idapi.Oop, failure: anyerror) !T {
    const present = address orelse return failure;
    return @ptrCast(@alignCast(present));
}

fn checkCanonicalExports(api: *idapi.IdApi) !void {
    const expected = [_]struct { [*:0]const u8, idapi.Oop }{
        .{ "Object", api.vtableOwner(api.object_vt) },
        .{ "Integer", api.vtableOwner(api.smallint_vt) },
        .{ "Float", api.vtableOwner(api.float_vt) },
        .{ "Word", api.vtableOwner(api.word_vt) },
        .{ "String", api.vtableOwner(api.string_vt) },
        .{ "Boolean", api.trueObj },
        .{ "Vtable", api.vtable_vt },
        .{ "Symbol", api.vtableOwner(api.symbol_vt) },
        .{ "SymbolVt", api.symbol_vt },
        .{ "ObjectVt", api.object_vt },
        .{ "IntegerVt", api.smallint_vt },
        .{ "FloatVt", api.float_vt },
        .{ "WordVt", api.word_vt },
        .{ "StringVt", api.string_vt },
    };
    try require(api.vtableCount() == 11, error.CanonicalSnapshotChanged);
    for (expected) |entry| {
        const name = entry[0];
        const value = entry[1];
        try require(value != null, error.CanonicalExportIsNull);
        try require(api.isExported(name) == 1, error.CanonicalNameNotExported);
        try require(api.import(name) == value, error.CanonicalExportChangedIdentity);
        const reverse = api.exportedNameOf(value) orelse return error.MissingReverseExportName;
        try require(std.mem.eql(u8, std.mem.span(reverse), std.mem.span(name)), error.WrongReverseExportName);
    }
}

fn checkAliasesAndRebinding(api: *idapi.IdApi) !void {
    const object = api.import("Object");
    const integer = api.import("Integer");
    try require(api.exportObj("ObjectAlias", object) == object, error.AliasExportFailed);
    try require(
        std.mem.eql(u8, std.mem.span(api.exportedNameOf(object).?), "Object"),
        error.FirstAliasLostAuthority,
    );
    try require(api.exportObj("Object", integer) == integer, error.RebindFailed);
    try require(api.import("Object") == integer, error.RebindNotVisible);
    try require(
        std.mem.eql(u8, std.mem.span(api.exportedNameOf(object).?), "ObjectAlias"),
        error.ReverseLookupIgnoredRebind,
    );
    try require(
        std.mem.eql(u8, std.mem.span(api.exportedNameOf(integer).?), "Object"),
        error.ReboundEarlierNameDidNotTakeAuthority,
    );
    try require(api.exportObj("NullExport", null) == null, error.NullExportChangedValue);
    try require(api.import("NullExport") == null, error.NullExportImportedValue);
    try require(api.isExported("NullExport") == 0, error.NullExportReportedPresent);
}

fn checkFixtureReload(api: *idapi.IdApi, fixture_path: [*:0]const u8) !void {
    const first_handle = api.dlopen(fixture_path, RTLD_LAZY) orelse return error.FixtureLoadFailed;
    const first_init = try function(FixtureInit, api.dlsym(first_handle, "birnam_fixture_module_init"), error.MissingFixtureInit);
    const state_value = try function(FixtureValue, api.dlsym(first_handle, "birnam_fixture_state_value"), error.MissingFixtureValue);
    const state = first_init(api, null) orelse return error.FixtureDidNotCreateState;
    try require(state_value(state) == 1, error.WrongInitialFixtureState);

    const second_handle = api.dlopen(fixture_path, RTLD_LAZY) orelse return error.FixtureReloadFailed;
    const second_init = try function(FixtureInit, api.dlsym(second_handle, "birnam_fixture_module_init"), error.MissingReloadedFixtureInit);
    const reloaded_state = second_init(api, state) orelse return error.FixtureRejectedState;
    try require(reloaded_state == state, error.FixtureReplacedOpaqueState);
    try require(state_value(state) == 2, error.FixtureStateDidNotAdvance);
    try require(api.dlsym(second_handle, "birnam_fixture_missing") == null, error.MissingSymbolResolved);
}

fn checkFloorReopen(api: *idapi.IdApi, floor_path: [*:0]const u8) !void {
    const handle = api.dlopen(floor_path, RTLD_LAZY) orelse return error.FloorReopenFailed;
    const identity = try function(
        Identity,
        api.dlsym(handle, "birnam_id_rust_crate_identity"),
        error.MissingReopenedIdentity,
    );
    try require(
        std.mem.eql(u8, std.mem.span(identity()), "rust-id-spike-r2.6"),
        error.WrongReopenedFloorIdentity,
    );
    try require(api.dlsym(handle, "id_init") != null, error.ReopenedFloorMissingIdInit);
    // Deliberately retain the loader handle and do not call the resolved id_init.
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const floor_path = args.next() orelse return error.MissingFloorPath;
    const fixture_path = args.next() orelse return error.MissingFixturePath;
    if (args.next() != null) return error.UnexpectedArgument;
    const allocator = std.heap.page_allocator;
    const floor_path_z = try allocator.dupeZ(u8, floor_path);
    defer allocator.free(floor_path_z);
    const fixture_path_z = try allocator.dupeZ(u8, fixture_path);
    defer allocator.free(fixture_path_z);

    var floor = try std.DynLib.open(floor_path);
    defer floor.close();
    const init = floor.lookup(idapi.IdInitFn, "id_init") orelse return error.MissingIdInit;
    const api = init();

    try checkCanonicalExports(api);
    try checkAliasesAndRebinding(api);
    try checkFixtureReload(api, fixture_path_z);
    try checkFloorReopen(api, floor_path_z);
    std.debug.print("rust-env-loader: canonical reflection and opaque reload state passed\n", .{});
}
