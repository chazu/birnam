//! Build/test-only proof that the committed language-neutral ABI manifest still
//! describes the Zig `IdApi`. Runtime loading and artifact selection never read
//! the manifest.

const std = @import("std");
const builtin = @import("builtin");
const idapi = @import("idapi.zig");

const manifest_bytes = @import("idapi_manifest").bytes;

const Manifest = struct {
    schema_version: u32,
    role: []const u8,
    target: Target,
    types: []const TypeEntry,
    id_api: IdApiEntry,
    contracts: []const ContractEntry,
    constants: []const ConstantEntry,
    fingerprint: Fingerprint,

    const Target = struct {
        arch: []const u8,
        os: []const u8,
        abi: []const u8,
        object_format: []const u8,
        endian: []const u8,
        pointer_bits: u16,
        c_int_bits: u16,
    };

    const TypeEntry = struct {
        name: []const u8,
        size: usize,
        alignment: usize,
        nullable: bool,
        signature: []const u8,
    };

    const IdApiEntry = struct {
        size: usize,
        alignment: usize,
        field_count: usize,
        fields: []const FieldEntry,
    };

    const FieldEntry = struct {
        index: usize,
        name: []const u8,
        offset: usize,
        size: usize,
        alignment: usize,
        nullable: bool,
        kind: []const u8,
        signature: []const u8,
    };

    const ConstantEntry = struct {
        name: []const u8,
        value: u64,
    };

    const ContractEntry = struct {
        path: []const u8,
        rule: []const u8,
    };

    const Fingerprint = struct {
        algorithm: []const u8,
        value: []const u8,
    };
};

fn abiFn(comptime info: std.builtin.Type.Fn) []const u8 {
    comptime var result: []const u8 = "[" ++ @tagName(info.calling_convention) ++ "](";
    inline for (info.params, 0..) |param, i| {
        if (i != 0) result = result ++ ",";
        result = result ++ comptime abiType(param.type orelse @compileError("generic ABI parameter"));
    }
    return result ++ ")->" ++ comptime abiType(info.return_type orelse @compileError("missing ABI return type"));
}

fn abiType(comptime T: type) []const u8 {
    if (T == void) return "void";
    if (T == anyopaque) return "void";
    if (T == idapi.IdApi) return "IdApi";
    if (T == c_int) return "c_int";
    if (T == u8) return "u8";
    if (T == i64) return "i64";
    if (T == u64) return "u64";
    if (T == usize) return "usize";
    if (T == f64) return "f64";

    return switch (@typeInfo(T)) {
        .optional => |optional| "nullable(" ++ comptime abiType(optional.child) ++ ")",
        .pointer => |pointer| blk: {
            if (@typeInfo(pointer.child) == .@"fn")
                break :blk "fnptr" ++ comptime abiFn(@typeInfo(pointer.child).@"fn");
            const qualifier = if (pointer.is_const) "const " else "";
            if (pointer.size == .many and pointer.sentinel() != null)
                break :blk "cstr<" ++ qualifier ++ comptime abiType(pointer.child) ++ ">";
            const shape = switch (pointer.size) {
                .one => "ptr",
                .many => "many",
                .slice => "slice",
                .c => "cptr",
            };
            break :blk shape ++ "<" ++ qualifier ++ comptime abiType(pointer.child) ++ ">";
        },
        .@"fn" => |function| "fn" ++ comptime abiFn(function),
        else => @compileError("unsupported ABI type " ++ @typeName(T)),
    };
}

fn topLevelNullable(comptime T: type) bool {
    return @typeInfo(T) == .optional;
}

fn fieldKind(comptime T: type) []const u8 {
    if (T == idapi.Oop) return "data-pointer";
    const info = @typeInfo(T);
    if (info == .pointer and @typeInfo(info.pointer.child) == .@"fn")
        return "function-pointer";
    @compileError("unsupported IdApi field kind " ++ @typeName(T));
}

const ActualType = struct {
    name: []const u8,
    size: usize,
    alignment: usize,
    nullable: bool,
    signature: []const u8,
};

fn actualType(comptime name: []const u8, comptime T: type) ActualType {
    return .{
        .name = name,
        .size = @sizeOf(T),
        .alignment = @alignOf(T),
        .nullable = topLevelNullable(T),
        .signature = abiType(T),
    };
}

const actual_types = [_]ActualType{
    actualType("Oop", idapi.Oop),
    actualType("Method", idapi.Method),
    actualType("RootProvider", idapi.RootProvider),
    actualType("TraceHook", idapi.TraceHook),
    actualType("Finalizer", idapi.Finalizer),
    actualType("MethodOverwriteHook", idapi.MethodOverwriteHook),
    actualType("IdInitFn", idapi.IdInitFn),
    actualType("ModuleInitFn", idapi.ModuleInitFn),
};

const actual_constants = [_]Manifest.ConstantEntry{
    .{ .name = "slot_raw", .value = idapi.slot_raw },
    .{ .name = "slot_oop", .value = idapi.slot_oop },
    .{ .name = "slot_code", .value = idapi.slot_code },
    .{ .name = "slot_unknown", .value = idapi.slot_unknown },
};

const actual_contracts = [_]Manifest.ContractEntry{
    .{ .path = "Method.args", .rule = "null permitted exactly when count is zero" },
    .{ .path = "IdApi.send.args", .rule = "null permitted exactly when count is zero" },
    .{ .path = "IdApi.setLayout.kinds", .rule = "null permitted exactly when prefix count is zero" },
};

fn hashU64(hasher: *std.crypto.hash.sha2.Sha256, value: u64) void {
    var bytes: [8]u8 = undefined;
    std.mem.writeInt(u64, &bytes, value, .little);
    hasher.update(&bytes);
}

fn hashBool(hasher: *std.crypto.hash.sha2.Sha256, value: bool) void {
    hasher.update(&.{if (value) 1 else 0});
}

fn hashBytes(hasher: *std.crypto.hash.sha2.Sha256, bytes: []const u8) void {
    hashU64(hasher, bytes.len);
    hasher.update(bytes);
}

fn expectType(expected: Manifest.TypeEntry, actual: ActualType) !void {
    try std.testing.expectEqualStrings(expected.name, actual.name);
    try std.testing.expectEqual(expected.size, actual.size);
    try std.testing.expectEqual(expected.alignment, actual.alignment);
    try std.testing.expectEqual(expected.nullable, actual.nullable);
    try std.testing.expectEqualStrings(expected.signature, actual.signature);
}

test "committed v1 manifest exactly describes the Zig IdApi" {
    const parsed = try std.json.parseFromSlice(Manifest, std.testing.allocator, manifest_bytes, .{});
    defer parsed.deinit();
    const manifest = parsed.value;

    try std.testing.expectEqual(@as(u32, 1), manifest.schema_version);
    try std.testing.expectEqualStrings("build-test-evidence-only", manifest.role);
    try std.testing.expectEqualStrings(@tagName(builtin.target.cpu.arch), manifest.target.arch);
    try std.testing.expectEqualStrings(@tagName(builtin.target.os.tag), manifest.target.os);
    try std.testing.expectEqualStrings(@tagName(builtin.target.abi), manifest.target.abi);
    try std.testing.expectEqualStrings(@tagName(builtin.target.ofmt), manifest.target.object_format);
    try std.testing.expectEqualStrings(@tagName(builtin.target.cpu.arch.endian()), manifest.target.endian);
    try std.testing.expectEqual(@as(u16, @bitSizeOf(usize)), manifest.target.pointer_bits);
    try std.testing.expectEqual(@as(u16, @bitSizeOf(c_int)), manifest.target.c_int_bits);

    try std.testing.expectEqual(actual_types.len, manifest.types.len);
    for (manifest.types, actual_types) |expected, actual| try expectType(expected, actual);

    const fields = @typeInfo(idapi.IdApi).@"struct".fields;
    try std.testing.expectEqual(@sizeOf(idapi.IdApi), manifest.id_api.size);
    try std.testing.expectEqual(@alignOf(idapi.IdApi), manifest.id_api.alignment);
    try std.testing.expectEqual(fields.len, manifest.id_api.field_count);
    try std.testing.expectEqual(fields.len, manifest.id_api.fields.len);

    inline for (fields, 0..) |field, i| {
        const expected = manifest.id_api.fields[i];
        try std.testing.expectEqual(i, expected.index);
        try std.testing.expectEqualStrings(field.name, expected.name);
        try std.testing.expectEqual(@offsetOf(idapi.IdApi, field.name), expected.offset);
        try std.testing.expectEqual(@sizeOf(field.type), expected.size);
        try std.testing.expectEqual(@alignOf(field.type), expected.alignment);
        try std.testing.expectEqual(topLevelNullable(field.type), expected.nullable);
        try std.testing.expectEqualStrings(fieldKind(field.type), expected.kind);
        try std.testing.expectEqualStrings(abiType(field.type), expected.signature);
    }

    try std.testing.expectEqual(actual_constants.len, manifest.constants.len);
    for (manifest.constants, actual_constants) |expected, actual| {
        try std.testing.expectEqualStrings(actual.name, expected.name);
        try std.testing.expectEqual(actual.value, expected.value);
    }

    try std.testing.expectEqual(actual_contracts.len, manifest.contracts.len);
    for (manifest.contracts, actual_contracts) |expected, actual| {
        try std.testing.expectEqualStrings(actual.path, expected.path);
        try std.testing.expectEqualStrings(actual.rule, expected.rule);
    }

    try std.testing.expectEqualStrings("sha256-length-prefixed-v1", manifest.fingerprint.algorithm);
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    hashBytes(&hasher, "idapi-abi-v1");
    hashU64(&hasher, manifest.schema_version);
    hashBytes(&hasher, @tagName(builtin.target.cpu.arch));
    hashBytes(&hasher, @tagName(builtin.target.os.tag));
    hashBytes(&hasher, @tagName(builtin.target.abi));
    hashBytes(&hasher, @tagName(builtin.target.ofmt));
    hashBytes(&hasher, @tagName(builtin.target.cpu.arch.endian()));
    hashU64(&hasher, @bitSizeOf(usize));
    hashU64(&hasher, @bitSizeOf(c_int));
    inline for (actual_types) |actual| {
        hashBytes(&hasher, actual.name);
        hashU64(&hasher, actual.size);
        hashU64(&hasher, actual.alignment);
        hashBool(&hasher, actual.nullable);
        hashBytes(&hasher, actual.signature);
    }
    hashU64(&hasher, @sizeOf(idapi.IdApi));
    hashU64(&hasher, @alignOf(idapi.IdApi));
    hashU64(&hasher, fields.len);
    inline for (fields, 0..) |field, i| {
        hashU64(&hasher, i);
        hashBytes(&hasher, field.name);
        hashU64(&hasher, @offsetOf(idapi.IdApi, field.name));
        hashU64(&hasher, @sizeOf(field.type));
        hashU64(&hasher, @alignOf(field.type));
        hashBool(&hasher, topLevelNullable(field.type));
        hashBytes(&hasher, fieldKind(field.type));
        hashBytes(&hasher, abiType(field.type));
    }
    inline for (actual_constants) |constant| {
        hashBytes(&hasher, constant.name);
        hashU64(&hasher, constant.value);
    }
    inline for (actual_contracts) |contract| {
        hashBytes(&hasher, contract.path);
        hashBytes(&hasher, contract.rule);
    }
    var digest: [32]u8 = undefined;
    hasher.final(&digest);
    const actual_fingerprint = std.fmt.bytesToHex(digest, .lower);
    if (!std.mem.eql(u8, manifest.fingerprint.value, &actual_fingerprint))
        std.debug.print("current IdApi v1 fingerprint: {s}\n", .{actual_fingerprint});
    try std.testing.expectEqualStrings(manifest.fingerprint.value, &actual_fingerprint);
}
