//! Backend-neutral R3.2 boxed scalar conformance client.

const std = @import("std");
const idapi = @import("idapi");

fn require(condition: bool, failure: anyerror) !void {
    if (!condition) return failure;
}

fn checkInteger(api: *idapi.IdApi) !void {
    const prototype = api.import("Integer");
    try require(prototype == api.vtableOwner(api.smallint_vt), error.IntegerPrototypeChanged);
    for ([_]i64{ std.math.minInt(i64), -1, 0, 1, 42, std.math.maxInt(i64) }) |value| {
        const object = api.mkSmallInt(value);
        try require(api.smallIntValue(object) == value, error.IntegerBitsChanged);
        try require(api.vtableOf(object) == api.smallint_vt, error.IntegerVtableChanged);
        try require(api.isSmallInt(object) == 1, error.IntegerPredicateRejected);
        try require(api.isFloat(object) == 0, error.FloatPredicateAcceptedInteger);
        try require(api.isWord(object) == 0, error.WordPredicateAcceptedInteger);
    }
    try require(api.import("Integer") == prototype, error.IntegerPrototypeReplaced);
}

fn checkFloat(api: *idapi.IdApi) !void {
    const prototype = api.import("Float");
    try require(prototype == api.vtableOwner(api.float_vt), error.FloatPrototypeChanged);
    for ([_]u64{
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0x7ff8_0000_0000_1234,
    }) |bits| {
        const object = api.mkFloat(@bitCast(bits));
        try require(@as(u64, @bitCast(api.floatValue(object))) == bits, error.FloatBitsChanged);
        try require(api.vtableOf(object) == api.float_vt, error.FloatVtableChanged);
        try require(api.isFloat(object) == 1, error.FloatPredicateRejected);
        try require(api.isSmallInt(object) == 0, error.IntegerPredicateAcceptedFloat);
        try require(api.isWord(object) == 0, error.WordPredicateAcceptedFloat);
    }
    try require(api.import("Float") == prototype, error.FloatPrototypeReplaced);
}

fn checkWord(api: *idapi.IdApi) !void {
    const prototype = api.import("Word");
    try require(prototype == api.vtableOwner(api.word_vt), error.WordPrototypeChanged);
    for ([_]u64{
        0,
        1,
        0x7fff_ffff_ffff_ffff,
        0x8000_0000_0000_0000,
        0xfedc_ba98_7654_3210,
        std.math.maxInt(u64),
    }) |value| {
        const object = api.mkWord(value);
        try require(api.wordValue(object) == value, error.WordBitsChanged);
        try require(api.vtableOf(object) == api.word_vt, error.WordVtableChanged);
        try require(api.isWord(object) == 1, error.WordPredicateRejected);
        try require(api.isSmallInt(object) == 0, error.IntegerPredicateAcceptedWord);
        try require(api.isFloat(object) == 0, error.FloatPredicateAcceptedWord);
    }
    try require(api.import("Word") == prototype, error.WordPrototypeReplaced);
}

fn checkRejection(api: *idapi.IdApi) !void {
    var foreign: usize = 0;
    const carriers = [_]idapi.Oop{
        null,
        @ptrCast(&foreign),
        @ptrCast(@constCast(&checkRejection)),
        api.nilObj,
        api.trueObj,
        api.falseObj,
    };
    for (carriers) |carrier| {
        try require(api.isSmallInt(carrier) == 0, error.IntegerPredicateAcceptedForeign);
        try require(api.isFloat(carrier) == 0, error.FloatPredicateAcceptedForeign);
        try require(api.isWord(carrier) == 0, error.WordPredicateAcceptedForeign);
    }
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    const label = args.next() orelse return error.MissingLabel;
    if (args.next() != null) return error.UnexpectedArgument;
    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    const init = dylib.lookup(idapi.IdInitFn, "id_init") orelse return error.MissingIdInit;
    const api = init();
    try require(api.vtableCount() == 11, error.CanonicalSnapshotChanged);
    try checkInteger(api);
    try checkFloat(api);
    try checkWord(api);
    try checkRejection(api);
    std.debug.print("scalar-conformance: {s} preserved boxed scalar identities and bits\n", .{label});
}
