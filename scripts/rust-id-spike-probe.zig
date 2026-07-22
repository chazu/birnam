//! Frozen-IdApi client for the isolated R2.6 Rust floor spike.

const std = @import("std");
const idapi = @import("idapi");

const Oop = idapi.Oop;

fn answer4242(_: Oop, args: [*]const Oop, count: usize) callconv(.c) Oop {
    if (count != 0 or @intFromPtr(args) == 0) return null;
    return @ptrFromInt(4242);
}

fn require(condition: bool, failure: anyerror) !void {
    if (!condition) return failure;
}

fn checkBootstrapSnapshot(api: *idapi.IdApi) !void {
    try require(api.vtableCount() == 11, error.WrongInitialVtableCount);
    try require(api.vtableAt(0) == api.vtable_vt, error.WrongVtableOrder);
    try require(api.vtableAt(1) == api.object_vt, error.WrongVtableOrder);
    try require(api.vtableAt(2) == api.symbol_vt, error.WrongVtableOrder);
    try require(api.vtableAt(5) == api.smallint_vt, error.WrongVtableOrder);
    try require(api.vtableAt(6) == api.string_vt, error.WrongVtableOrder);
    try require(api.vtableAt(7) == api.float_vt, error.WrongVtableOrder);
    try require(api.vtableAt(8) == api.word_vt, error.WrongVtableOrder);
    try require(api.vtableAt(9) == api.vtableOf(api.nilObj), error.WrongVtableOrder);
    try require(api.vtableAt(10) == api.vtableOf(api.trueObj), error.WrongVtableOrder);
    try require(api.vtableOf(api.falseObj) == api.vtableAt(10), error.WrongBooleanFamily);
    try require(api.vtableAt(11) == null, error.VtableEnumerationNotBounded);

    const internal_symbols = api.vtableAt(3);
    const environment = api.vtableAt(4);
    try require(internal_symbols != null, error.MissingInternalSymbolDictionary);
    try require(environment != null, error.MissingEnvironmentDictionary);
    try require(internal_symbols != environment, error.InternalDictionariesAliased);
    try require(api.vtableParent(api.vtable_vt) == api.object_vt, error.WrongMetaParent);
    try require(api.vtableParent(api.object_vt) == null, error.WrongObjectParent);
    try require(api.vtableParent(api.symbol_vt) == api.object_vt, error.WrongSymbolParent);
    try require(api.vtableParent(internal_symbols) == null, error.WrongInternalParent);
    try require(api.vtableParent(environment) == null, error.WrongInternalParent);

    try require(api.vtableOwner(api.smallint_vt) != null, error.MissingIntegerOwner);
    try require(api.vtableOwner(api.string_vt) != null, error.MissingStringOwner);
    try require(api.vtableOwner(api.float_vt) != null, error.MissingFloatOwner);
    try require(api.vtableOwner(api.word_vt) != null, error.MissingWordOwner);
    try require(api.vtableOwner(api.vtableAt(9)) == api.nilObj, error.WrongNilOwner);
    try require(api.vtableOwner(api.vtableAt(10)) == api.trueObj, error.WrongBooleanOwner);
    const symbol_prototype = api.vtableOwner(api.symbol_vt);
    try require(symbol_prototype != null, error.MissingSymbolOwner);
    try require(
        std.mem.eql(u8, std.mem.span(api.symbolName(symbol_prototype)), "Symbol"),
        error.WrongSymbolPrototype,
    );

    try require(api.objectPayloadSize(api.nilObj) == 0, error.WrongNilShape);
    try require(api.objectPayloadSize(api.trueObj) == 0, error.WrongBooleanShape);
    try require(api.objectPayloadSize(api.falseObj) == 0, error.WrongBooleanShape);
    try require(
        api.objectPayloadSize(api.vtableOwner(api.object_vt)) == 0,
        error.WrongObjectPrototypeShape,
    );
    try require(
        api.objectPayloadSize(api.vtableOwner(api.smallint_vt)) == 8,
        error.WrongIntegerPrototypeShape,
    );
    try require(
        api.objectPayloadSize(api.vtableOwner(api.float_vt)) == 8,
        error.WrongFloatPrototypeShape,
    );
    try require(
        api.objectPayloadSize(api.vtableOwner(api.word_vt)) == 8,
        error.WrongWordPrototypeShape,
    );
    try require(
        api.objectPayloadSize(api.vtableOwner(api.string_vt)) == 16,
        error.WrongStringPrototypeShape,
    );
}

fn runFunctional(api: *idapi.IdApi) !void {
    try checkBootstrapSnapshot(api);

    const selector = api.intern("zigSpikeAnswer");
    try require(selector == api.intern("zigSpikeAnswer"), error.UnstableInterning);
    const child = api.proto(api.object_vt);
    try require(api.vtableCount() == 12, error.ProtoDidNotRegisterVtable);
    try require(api.vtableAt(11) == child, error.ProtoWrongEnumerationOrder);
    try require(api.vtableParent(child) == api.object_vt, error.ProtoWrongParent);
    api.method(child, selector, answer4242);
    try require(api.vtableSelectorAt(child, 0) == selector, error.MethodNotEnumerated);
    try require(api.vtableSelectorAt(child, 1) == null, error.MethodEnumerationNotBounded);
    const receiver = api.alloc(child, 0);
    try require(api.vtableOf(receiver) == child, error.AllocWrongVtable);
    try require(
        api.send(receiver, selector, @ptrFromInt(8), 0) == @as(Oop, @ptrFromInt(4242)),
        error.WrongSendResult,
    );
    std.debug.print("rust-id-spike: frozen IdApi client dispatched through Rust\n", .{});
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    const mode = args.next() orelse return error.MissingMode;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    const init = dylib.lookup(idapi.IdInitFn, "id_init") orelse
        return error.MissingIdInit;
    const api = init();

    if (std.mem.eql(u8, mode, "functional")) return runFunctional(api);
    if (std.mem.eql(u8, mode, "unsupported")) {
        _ = api.mkString("x", 1);
        return error.UnsupportedFieldReturned;
    }
    return error.UnknownMode;
}
