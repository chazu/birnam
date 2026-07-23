//! Direct loader probe for the production Rust floor's legacy R2.6 diagnostic.

const std = @import("std");

const CrateIdentity = *const fn () callconv(.c) [*:0]const u8;
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
const AbiDescriptorFn = *const fn () callconv(.c) *const AbiDescriptorV1;

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const path = args.next() orelse return error.MissingDylibPath;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(path);
    defer dylib.close();

    const identity = dylib.lookup(
        CrateIdentity,
        "birnam_id_rust_crate_identity",
    ) orelse return error.MissingRustCrateIdentity;
    if (!std.mem.eql(u8, std.mem.span(identity()), "rust-id-spike-r2.6")) {
        return error.WrongRustCrateIdentity;
    }

    const floor_identity = dylib.lookup(
        CrateIdentity,
        "birnam_id_floor_identity",
    ) orelse return error.MissingRustFloorIdentity;
    if (!std.mem.eql(u8, std.mem.span(floor_identity()), "rust-id-v1")) {
        return error.WrongRustFloorIdentity;
    }

    _ = dylib.lookup(*const anyopaque, "id_init") orelse
        return error.MissingIdInitExport;

    const describe = dylib.lookup(
        AbiDescriptorFn,
        "birnam_id_rust_abi_descriptor_v1",
    ) orelse return error.MissingRustAbiDescriptor;
    const descriptor = describe();
    if (!std.mem.eql(u8, &descriptor.magic, "BIRNABI1"))
        return error.WrongRustAbiDescriptorMagic;
    if (descriptor.descriptor_version != 1 or
        descriptor.descriptor_size != @sizeOf(AbiDescriptorV1) or
        descriptor.type_count != 8 or
        descriptor.field_count != 54)
    {
        return error.WrongRustAbiDescriptorHeader;
    }

    std.debug.print(
        "rust-floor-shell: crate/floor identities, ABI descriptor, and id_init present\n",
        .{},
    );
}
