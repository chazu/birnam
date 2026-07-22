//! Cross-language proof that the diagnostic Rust cdylib mirrors the frozen
//! IdApi v1 layout. The test reads build/test evidence only and never calls
//! `id_init` or any function in the API table.

const std = @import("std");

const manifest_bytes = @import("idapi_manifest").bytes;

const Manifest = struct {
    schema_version: usize,
    target: Target,
    types: []const TypeEntry,
    id_api: IdApiEntry,
    constants: []const ConstantEntry,
    fingerprint: Fingerprint,

    const Target = struct {
        arch: []const u8,
        os: []const u8,
        abi: []const u8,
        object_format: []const u8,
        endian: []const u8,
        pointer_bits: usize,
        c_int_bits: usize,
    };

    const TypeEntry = struct {
        size: usize,
        alignment: usize,
        nullable: bool,
    };

    const IdApiEntry = struct {
        size: usize,
        alignment: usize,
        field_count: usize,
        fields: []const FieldEntry,
    };

    const FieldEntry = struct {
        index: usize,
        offset: usize,
        size: usize,
        alignment: usize,
        nullable: bool,
        kind: []const u8,
    };

    const ConstantEntry = struct {
        value: usize,
    };

    const Fingerprint = struct {
        value: []const u8,
    };
};

const AbiLayoutEntryV1 = extern struct {
    offset: usize,
    size: usize,
    alignment: usize,
    nullable: usize,
    kind: usize,
};

const AbiDescriptorV1 = extern struct {
    magic: [8]u8,
    descriptor_version: usize,
    descriptor_size: usize,
    schema_version: usize,
    target_arch: usize,
    target_os: usize,
    target_abi: usize,
    object_format: usize,
    endian: usize,
    pointer_bits: usize,
    c_int_bits: usize,
    id_api_size: usize,
    id_api_alignment: usize,
    type_count: usize,
    field_count: usize,
    types: [8]AbiLayoutEntryV1,
    fields: [54]AbiLayoutEntryV1,
    constants: [4]usize,
    fingerprint: [32]u8,
};

const DescribeAbi = *const fn () callconv(.c) *const AbiDescriptorV1;

const kind_type: usize = 0;
const kind_data_pointer: usize = 1;
const kind_function_pointer: usize = 2;

fn expectedCode(actual: []const u8, expected: []const u8, code: usize) !usize {
    if (!std.mem.eql(u8, actual, expected)) return error.UnsupportedManifestTarget;
    return code;
}

fn expectedFieldKind(kind: []const u8) !usize {
    if (std.mem.eql(u8, kind, "data-pointer")) return kind_data_pointer;
    if (std.mem.eql(u8, kind, "function-pointer")) return kind_function_pointer;
    return error.UnsupportedManifestFieldKind;
}

fn checkLayoutEntries(manifest: Manifest, descriptor: *const AbiDescriptorV1) !void {
    if (descriptor.type_count != manifest.types.len or
        descriptor.field_count != manifest.id_api.fields.len)
    {
        return error.LayoutCountMismatch;
    }

    for (manifest.types, descriptor.types) |expected, actual| {
        if (actual.offset != 0 or
            actual.size != expected.size or
            actual.alignment != expected.alignment or
            actual.nullable != @intFromBool(expected.nullable) or
            actual.kind != kind_type)
        {
            return error.TypeLayoutMismatch;
        }
    }

    for (manifest.id_api.fields, descriptor.fields, 0..) |expected, actual, index| {
        if (expected.index != index or
            actual.offset != expected.offset or
            actual.size != expected.size or
            actual.alignment != expected.alignment or
            actual.nullable != @intFromBool(expected.nullable) or
            actual.kind != try expectedFieldKind(expected.kind))
        {
            return error.FieldLayoutMismatch;
        }
    }
}

fn checkDescriptor(manifest: Manifest, descriptor: *const AbiDescriptorV1) !void {
    if (!std.mem.eql(u8, &descriptor.magic, "BIRNABI1") or
        descriptor.descriptor_version != 1 or
        descriptor.descriptor_size != @sizeOf(AbiDescriptorV1) or
        descriptor.schema_version != manifest.schema_version)
    {
        return error.DescriptorHeaderMismatch;
    }

    if (descriptor.target_arch != try expectedCode(manifest.target.arch, "aarch64", 1) or
        descriptor.target_os != try expectedCode(manifest.target.os, "macos", 1) or
        descriptor.target_abi != try expectedCode(manifest.target.abi, "none", 0) or
        descriptor.object_format != try expectedCode(manifest.target.object_format, "macho", 1) or
        descriptor.endian != try expectedCode(manifest.target.endian, "little", 0) or
        descriptor.pointer_bits != manifest.target.pointer_bits or
        descriptor.c_int_bits != manifest.target.c_int_bits)
    {
        return error.TargetMismatch;
    }

    if (descriptor.id_api_size != manifest.id_api.size or
        descriptor.id_api_alignment != manifest.id_api.alignment or
        descriptor.field_count != manifest.id_api.field_count)
    {
        return error.IdApiLayoutMismatch;
    }

    try checkLayoutEntries(manifest, descriptor);

    if (descriptor.constants.len != manifest.constants.len)
        return error.ConstantCountMismatch;
    for (manifest.constants, descriptor.constants) |expected, actual| {
        if (actual != expected.value) return error.ConstantMismatch;
    }

    var expected_fingerprint: [32]u8 = undefined;
    _ = try std.fmt.hexToBytes(&expected_fingerprint, manifest.fingerprint.value);
    if (!std.mem.eql(u8, &descriptor.fingerprint, &expected_fingerprint))
        return error.FingerprintMismatch;
}

test "Rust descriptor exactly matches the frozen IdApi v1 manifest" {
    const parsed = try std.json.parseFromSlice(
        Manifest,
        std.testing.allocator,
        manifest_bytes,
        .{ .ignore_unknown_fields = true },
    );
    defer parsed.deinit();

    const dylib_path = std.posix.getenv("BIRNAM_RUST_ID_DYLIB") orelse
        return error.MissingRustIdDylib;
    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();

    try std.testing.expect(dylib.lookup(*const anyopaque, "id_init") == null);
    const describe = dylib.lookup(
        DescribeAbi,
        "birnam_id_rust_abi_descriptor_v1",
    ) orelse return error.MissingRustAbiDescriptor;
    try checkDescriptor(parsed.value, describe());
}

test "Rust descriptor gate rejects a field reorder" {
    const parsed = try std.json.parseFromSlice(
        Manifest,
        std.testing.allocator,
        manifest_bytes,
        .{ .ignore_unknown_fields = true },
    );
    defer parsed.deinit();

    const dylib_path = std.posix.getenv("BIRNAM_RUST_ID_DYLIB") orelse
        return error.MissingRustIdDylib;
    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();

    const describe = dylib.lookup(
        DescribeAbi,
        "birnam_id_rust_abi_descriptor_v1",
    ) orelse return error.MissingRustAbiDescriptor;
    var reordered = describe().*;
    std.mem.swap(AbiLayoutEntryV1, &reordered.fields[0], &reordered.fields[1]);
    try std.testing.expectError(
        error.FieldLayoutMismatch,
        checkDescriptor(parsed.value, &reordered),
    );
}
