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
    /// Phase 11.7.vi (ADR-11 #7): single-writer discipline. Acquired
    /// around any mutation of `str_to_eid` / `eid_to_str`. Readers
    /// (`get` / `nameOf`) walk the persistent HAMT lock-free — the root
    /// pointer reassignment is the publication point, atomic on every
    /// supported arch (aligned 64-bit pointer store). Under the Datomic
    /// shape the transactor thread is the sole acquirer; today the
    /// single application thread acquires uncontended.
    writer_lock: std.Thread.Mutex = .{},

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
        self.writer_lock.lock();
        defer self.writer_lock.unlock();
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
        // Lock-free fast path — concurrent readers + this writer alike
        // see a complete HAMT snapshot via the root pointer.
        if (self.str_to_eid.get(name)) |eid| return eid;

        self.writer_lock.lock();
        defer self.writer_lock.unlock();
        // Re-check under lock — another writer may have interned between
        // the lock-free probe and the acquire.
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

    /// Restore a name↔EID entry directly into the maps without asserting a
    /// `:name` fact. Phase 12 image load uses this to rebuild the interner
    /// from a serialized snapshot so user-interned names keep their original
    /// EIDs across save/load (the eid↔name bijection is otherwise
    /// session-local). Idempotent for entries boot already seeded (overwrite
    /// with identical data).
    pub fn restoreEntry(self: *Interner, eid: u64, name: []const u8) !void {
        self.writer_lock.lock();
        defer self.writer_lock.unlock();
        const dup = try self.allocator.dupe(u8, name);
        self.str_to_eid = try self.str_to_eid.assoc(dup, eid);
        self.eid_to_str = try self.eid_to_str.assoc(eid, dup);
    }
};
