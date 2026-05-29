//! Bidirectional string ⟷ EID intern table.
//!
//! `intern(str)` returns the same EID for byte-equal strings. New strings
//! get a user-range EID minted from the object table, are recorded in
//! both directions of the table, and get a `:name` fact asserted in the
//! store.
//!
//! Kernel selectors (well-known EIDs in 0..63) are pre-seeded during
//! bootstrap so `intern("lookup:")` returns `SEL_LOOKUP`.

const std = @import("std");
const hamt = @import("hamt.zig");
const ref_mod = @import("ref.zig");
const strings = @import("strings.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");

pub const Ref = ref_mod.Ref;

const Str2Eid = hamt.Hamt([]const u8, u64, hamt.StrCtx);
const Eid2Str = hamt.Hamt(u64, []const u8, hamt.U64Ctx);

pub const Interner = struct {
    allocator: std.mem.Allocator,
    str_to_eid: Str2Eid,
    eid_to_str: Eid2Str,

    pub fn init(allocator: std.mem.Allocator) Interner {
        return .{
            .allocator = allocator,
            .str_to_eid = Str2Eid.empty(allocator, .{}),
            .eid_to_str = Eid2Str.empty(allocator, .{}),
        };
    }

    /// Pre-seed: register an existing well-known EID with a name. Used by
    /// bootstrap for kernel selectors. Also asserts the `:name` fact.
    pub fn seed(
        self: *Interner,
        store: *fact_store.FactStore,
        eid: u64,
        name: []const u8,
    ) !void {
        const dup = try self.allocator.dupe(u8, name);
        self.str_to_eid = try self.str_to_eid.assoc(dup, eid);
        self.eid_to_str = try self.eid_to_str.assoc(eid, dup);
        const str_ref = try strings.mkString(self.allocator, dup);
        _ = try store.assert(eid, kernel_eids.ATTR_NAME, str_ref, kernel_eids.M_BOOTSTRAP);
    }

    /// Look up an EID for a string. Mints a new user-range EID if absent,
    /// records both directions, asserts the `:name` fact.
    pub fn intern(
        self: *Interner,
        store: *fact_store.FactStore,
        table: *object_table.ObjectTable,
        name: []const u8,
    ) !u64 {
        if (self.str_to_eid.get(name)) |eid| return eid;
        const dup = try self.allocator.dupe(u8, name);
        const eid = table.alloc();
        self.str_to_eid = try self.str_to_eid.assoc(dup, eid);
        self.eid_to_str = try self.eid_to_str.assoc(eid, dup);
        const str_ref = try strings.mkString(self.allocator, dup);
        _ = try store.assert(eid, kernel_eids.ATTR_NAME, str_ref, kernel_eids.M_BOOTSTRAP);
        return eid;
    }

    /// Reverse lookup: name of an EID, if known.
    pub fn nameOf(self: *const Interner, eid: u64) ?[]const u8 {
        return self.eid_to_str.get(eid);
    }

    pub fn count(self: *const Interner) usize {
        return self.str_to_eid.count();
    }
};
