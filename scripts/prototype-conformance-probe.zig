//! Backend-neutral R3.4 prototype, singleton, and clone conformance client.

const std = @import("std");
const idapi = @import("idapi");

var callback_api: ?*idapi.IdApi = null;

fn require(condition: bool, failure: anyerror) !void {
    if (!condition) return failure;
}

fn answerTrue(_: idapi.Oop, _: [*]const idapi.Oop, _: usize) callconv(.c) idapi.Oop {
    return callback_api.?.trueObj;
}

fn answerFalse(_: idapi.Oop, _: [*]const idapi.Oop, _: usize) callconv(.c) idapi.Oop {
    return callback_api.?.falseObj;
}

fn bytes(api: *idapi.IdApi, object: idapi.Oop) []const u8 {
    var length: usize = 0;
    const pointer = api.stringBytes(object, &length);
    return pointer[0..length];
}

fn checkCanonicalGraph(api: *idapi.IdApi) ![14]idapi.Oop {
    const names = [_][*:0]const u8{
        "Object", "Integer",  "Float",    "Word",      "String",  "Boolean", "Vtable",
        "Symbol", "SymbolVt", "ObjectVt", "IntegerVt", "FloatVt", "WordVt",  "StringVt",
    };
    var values: [names.len]idapi.Oop = undefined;
    for (names, 0..) |name, index| {
        values[index] = api.import(name);
        try require(values[index] != null, error.CanonicalExportMissing);
    }
    try require(values[0] == api.vtableOwner(api.object_vt), error.ObjectOwnerChanged);
    try require(values[1] == api.vtableOwner(api.smallint_vt), error.IntegerOwnerChanged);
    try require(values[2] == api.vtableOwner(api.float_vt), error.FloatOwnerChanged);
    try require(values[3] == api.vtableOwner(api.word_vt), error.WordOwnerChanged);
    try require(values[4] == api.vtableOwner(api.string_vt), error.StringOwnerChanged);
    try require(values[5] == api.trueObj, error.BooleanExportChanged);
    try require(values[6] == api.vtable_vt, error.VtableExportChanged);
    try require(values[8] == api.symbol_vt, error.SymbolVtableExportChanged);
    try require(values[9] == api.object_vt, error.ObjectVtableExportChanged);
    try require(values[10] == api.smallint_vt, error.IntegerVtableExportChanged);
    try require(values[11] == api.float_vt, error.FloatVtableExportChanged);
    try require(values[12] == api.word_vt, error.WordVtableExportChanged);
    try require(values[13] == api.string_vt, error.StringVtableExportChanged);
    try require(api.vtableOwner(api.vtableOf(api.nilObj)) == api.nilObj, error.NilOwnerChanged);
    try require(api.vtableOwner(api.vtableOf(api.trueObj)) == api.trueObj, error.BooleanOwnerChanged);
    return values;
}

fn checkProtocolOrder(api: *idapi.IdApi) !void {
    const clone = api.intern("clone");
    const new = api.intern("new");
    const identical = api.intern("==");
    try require(api.vtableSelectorAt(api.object_vt, 0) == api.intern("doesNotUnderstand:"), error.ObjectDnuOrderChanged);
    try require(api.vtableSelectorAt(api.object_vt, 1) == clone, error.ObjectCloneOrderChanged);
    try require(api.vtableSelectorAt(api.object_vt, 2) == new, error.ObjectNewOrderChanged);
    try require(api.vtableSelectorAt(api.object_vt, 3) == identical, error.ObjectIdentityOrderChanged);
    try require(api.vtableSelectorAt(api.object_vt, 4) == null, error.ObjectProtocolGrewUnexpectedly);
    try require(api.vtableSelectorAt(api.string_vt, 0) == clone, error.StringCloneOrderChanged);
    try require(api.vtableSelectorAt(api.string_vt, 1) == new, error.StringNewOrderChanged);
    try require(api.vtableSelectorAt(api.string_vt, 2) == null, error.StringProtocolGrewUnexpectedly);
}

fn checkCloneAndIdentity(api: *idapi.IdApi) !void {
    const clone = api.intern("clone");
    const new = api.intern("new");
    const identical = api.intern("==");
    const object_prototype = api.import("Object");
    const object_clone = api.send(object_prototype, clone, &.{}, 0);
    const object_new = api.send(object_prototype, new, &.{}, 0);
    try require(object_clone != object_prototype, error.ObjectCloneReusedReceiver);
    try require(object_new != object_prototype, error.ObjectNewReusedReceiver);
    try require(api.vtableOf(object_clone) == api.object_vt, error.ObjectCloneFamilyChanged);
    try require(api.vtableOf(object_new) == api.object_vt, error.ObjectNewFamilyChanged);

    const payload_object = api.alloc(api.object_vt, 17);
    const extent = api.objectPayloadSize(payload_object);
    try require(extent >= 17, error.PayloadExtentShrank);
    const payload: [*]u8 = @ptrCast(@alignCast(payload_object.?));
    for (payload[0..extent], 0..) |*byte, index| byte.* = @truncate(index *% 29 +% 3);
    const payload_clone = api.send(payload_object, clone, &.{}, 0);
    const copied: [*]const u8 = @ptrCast(@alignCast(payload_clone.?));
    try require(api.vtableOf(payload_clone) == api.object_vt, error.PayloadCloneFamilyChanged);
    try require(api.objectPayloadSize(payload_clone) == extent, error.PayloadCloneExtentChanged);
    try require(std.mem.eql(u8, payload[0..extent], copied[0..extent]), error.PayloadCloneBytesChanged);

    const same_args = [_]idapi.Oop{payload_object};
    const other_args = [_]idapi.Oop{payload_clone};
    try require(api.send(payload_object, identical, &same_args, 1) == api.trueObj, error.IdentityRejectedSelf);
    try require(api.send(payload_object, identical, &other_args, 1) == api.falseObj, error.IdentityAcceptedClone);
    const nil_args = [_]idapi.Oop{api.nilObj};
    const false_args = [_]idapi.Oop{api.falseObj};
    try require(api.send(api.nilObj, identical, &nil_args, 1) == api.trueObj, error.NilIdentityRejectedSelf);
    try require(api.send(api.trueObj, identical, &false_args, 1) == api.falseObj, error.BooleanIdentityCollapsed);

    const original = api.mkString(&[_]u8{ 'o', 'w', 'n', 'e', 'd', 0, 0xff }, 7);
    for ([_]idapi.Oop{
        api.send(original, clone, &.{}, 0),
        api.send(original, new, &.{}, 0),
    }) |copy| {
        try require(copy != original, error.StringCloneReusedReceiver);
        try require(api.vtableOf(copy) == api.string_vt, error.StringCloneFamilyChanged);
        try require(bytes(api, copy).ptr != bytes(api, original).ptr, error.StringCloneSharedStorage);
        try require(std.mem.eql(u8, bytes(api, copy), bytes(api, original)), error.StringCloneBytesChanged);
    }
}

fn checkPrivateVtables(api: *idapi.IdApi) !void {
    try require(api.ensurePrivateVtable(api.object_vt) == api.object_vt, error.RawVtableSplit);
    try require(api.ensurePrivateVtable(api.import("Object")) == api.object_vt, error.ObjectPrototypeSplit);
    try require(api.ensurePrivateVtable(api.import("Integer")) == api.smallint_vt, error.IntegerPrototypeSplit);
    try require(api.ensurePrivateVtable(api.import("String")) == api.string_vt, error.StringPrototypeSplit);
    const nil_vt = api.vtableOf(api.nilObj);
    const bool_vt = api.vtableOf(api.trueObj);
    try require(api.ensurePrivateVtable(api.nilObj) == nil_vt, error.NilSingletonSplit);
    try require(api.ensurePrivateVtable(api.trueObj) == bool_vt, error.BooleanPrototypeSplit);

    const first = api.alloc(api.object_vt, 0);
    const sibling = api.alloc(api.object_vt, 0);
    const private = api.ensurePrivateVtable(first);
    try require(private != api.object_vt, error.InstanceDidNotSplit);
    try require(api.ensurePrivateVtable(first) == private, error.InstanceSplitTwice);
    try require(api.vtableOf(first) == private, error.InstanceDidNotAdoptPrivateVtable);
    try require(api.vtableOf(sibling) == api.object_vt, error.InstanceSplitLeakedToSibling);
    try require(api.vtableParent(private) == api.object_vt, error.PrivateParentChanged);
    try require(api.vtableOwner(private) == first, error.PrivateOwnerChanged);
    const private_selector = api.intern("r34PrivateAnswer");
    api.method(private, private_selector, &answerFalse);
    try require(api.send(first, private_selector, &.{}, 0) == api.falseObj, error.PrivateMethodDidNotDispatch);

    const family_selector = api.intern("r34BooleanSpecialization");
    api.method(bool_vt, family_selector, &answerTrue);
    try require(api.send(api.trueObj, family_selector, &.{}, 0) == api.trueObj, error.BooleanFamilyMethodMissedTrue);
    try require(api.send(api.falseObj, family_selector, &.{}, 0) == api.trueObj, error.BooleanFamilyMethodMissedFalse);
    const false_private = api.ensurePrivateVtable(api.falseObj);
    try require(false_private != bool_vt, error.FalseDidNotSplit);
    try require(api.vtableParent(false_private) == bool_vt, error.FalsePrivateParentChanged);
    try require(api.vtableOwner(false_private) == api.falseObj, error.FalsePrivateOwnerChanged);
    api.method(false_private, family_selector, &answerFalse);
    try require(api.send(api.trueObj, family_selector, &.{}, 0) == api.trueObj, error.FalseOverrideLeakedToTrue);
    try require(api.send(api.falseObj, family_selector, &.{}, 0) == api.falseObj, error.FalseOverrideDidNotDispatch);
}

fn checkParentAndOwnerMutation(api: *idapi.IdApi) !void {
    const parent_a = api.proto(api.object_vt);
    const parent_b = api.proto(api.object_vt);
    const child = api.proto(parent_a);
    try require(api.vtableParent(child) == parent_a, error.InitialParentChanged);
    api.setParent(child, parent_b);
    try require(api.vtableParent(child) == parent_b, error.ParentMutationFailed);

    const owner = api.alloc(child, 0);
    api.setOwner(child, owner);
    try require(api.vtableOwner(child) == owner, error.OwnerMutationFailed);
    try require(api.ensurePrivateVtable(owner) == child, error.OwnerStillSplit);
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
    callback_api = api;

    try require(api.vtableCount() == 11, error.BootstrapVtableOrderChanged);
    try require(api.vtableOf(api.trueObj) == api.vtableOf(api.falseObj), error.BooleanFamilySplitEarly);
    const canonical = try checkCanonicalGraph(api);
    try checkProtocolOrder(api);
    try checkCloneAndIdentity(api);
    try checkPrivateVtables(api);
    try checkParentAndOwnerMutation(api);
    const after = try checkCanonicalGraph(api);
    try require(std.mem.eql(idapi.Oop, &canonical, &after), error.CanonicalIdentityReplaced);
    std.debug.print("prototype-conformance: {s} preserved prototype and singleton semantics\n", .{label});
}
