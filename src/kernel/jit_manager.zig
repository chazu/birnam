//! JitManager — owns native-compilation lifetimes + the per-method bytecode⇄native
//! dial (docs/10a). `promote(eid)` compiles+installs native; `demote(eid)` reverts
//! to the interpreter; `promoteAll`/`demoteAll` sweep the whole image. The
//! kernel-owned manager (vs the caller-owns-`Jit` test pattern) is what makes
//! native compilation first-class: any method can be flipped, live, both ways,
//! and a re-JIT/demote correctly drops the old native code.

const std = @import("std");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const jit = @import("jit.zig");
const jit_method = @import("jit_method.zig");

/// Set by the shell so the dispatch layer can reach the manager for auto-JIT
/// (hotness) without the kernel statically depending on libtcc.
pub var g_manager: ?*JitManager = null;

pub const JitManager = struct {
    allocator: std.mem.Allocator,
    /// method_eid → its live native compilation (the owning Jit + registry index).
    native: std.AutoHashMap(u64, Native),
    /// methods the codegen can't lower (e.g. a block's `^`) — remembered so a
    /// sweep doesn't re-attempt + re-fail them every time.
    declined: std.AutoHashMap(u64, void),

    const Native = struct { jit: jit.Jit, idx: u64 };

    pub const Stats = struct { compiled: usize = 0, declined: usize = 0, already: usize = 0 };

    pub fn init(allocator: std.mem.Allocator) JitManager {
        return .{
            .allocator = allocator,
            .native = std.AutoHashMap(u64, Native).init(allocator),
            .declined = std.AutoHashMap(u64, void).init(allocator),
        };
    }

    pub fn deinit(self: *JitManager) void {
        var it = self.native.valueIterator();
        while (it.next()) |n| n.jit.deinit();
        self.native.deinit();
        self.declined.deinit();
    }

    pub fn isNative(self: *JitManager, eid: u64) bool {
        return self.native.contains(eid);
    }

    /// bytecode → native. Re-JITs in place if already native (drops the old code,
    /// reuses the registry index so `closure.data` stays valid). Returns true iff
    /// the method is native afterward.
    pub fn promote(
        self: *JitManager,
        store: *const fact_store.FactStore,
        table: *object_table.ObjectTable,
        eid: u64,
    ) bool {
        if (self.declined.contains(eid)) return false;
        var compiled = jit_method.compileMethod(self.allocator, store, eid) catch |e| {
            if (e == error.Declined) self.declined.put(eid, {}) catch {};
            return false;
        };
        if (self.native.getPtr(eid)) |n| {
            n.jit.deinit(); // drop stale native code; reuse the index
            n.jit = compiled.jit;
            jit_method.registrySet(n.idx, compiled.fn_ptr);
            jit_method.installTrampoline(self.allocator, table, eid, n.idx) catch {};
            return true;
        }
        const idx = jit_method.registerFn(compiled.fn_ptr) orelse {
            compiled.jit.deinit();
            return false;
        };
        jit_method.installTrampoline(self.allocator, table, eid, idx) catch {
            compiled.jit.deinit();
            return false;
        };
        self.native.put(eid, .{ .jit = compiled.jit, .idx = idx }) catch {
            compiled.jit.deinit();
            return false;
        };
        return true;
    }

    /// native → bytecode. Drops the native code + reinstalls the interpreter
    /// closure, so dispatch reverts to `runMethod` over the canonical bytecode.
    pub fn demote(self: *JitManager, table: *object_table.ObjectTable, eid: u64) void {
        if (self.native.fetchRemove(eid)) |kv| {
            var n = kv.value;
            n.jit.deinit();
            jit_method.installBytecode(self.allocator, table, eid) catch {};
        }
    }

    /// Re-JIT a method whose bytecode changed (redefine), IF it is currently
    /// native. No-op otherwise. Keeps native in sync with edits — the malleable
    /// AND native loop.
    pub fn refresh(
        self: *JitManager,
        store: *const fact_store.FactStore,
        table: *object_table.ObjectTable,
        eid: u64,
    ) void {
        if (self.isNative(eid)) _ = self.promote(store, table, eid);
    }

    /// Compile every (eligible) bytecode method to native. The whole-image dial.
    pub fn promoteAll(
        self: *JitManager,
        store: *const fact_store.FactStore,
        table: *object_table.ObjectTable,
    ) Stats {
        var s = Stats{};
        var it = store.entitiesWith(kernel_eids.ATTR_BYTECODE_STREAM);
        while (it.next()) |entry| {
            if (self.isNative(entry.e)) {
                s.already += 1;
                continue;
            }
            if (self.promote(store, table, entry.e)) s.compiled += 1 else s.declined += 1;
        }
        return s;
    }

    /// Revert all native methods to bytecode.
    pub fn demoteAll(self: *JitManager, table: *object_table.ObjectTable) usize {
        var keys = std.ArrayList(u64){};
        defer keys.deinit(self.allocator);
        var it = self.native.keyIterator();
        while (it.next()) |k| keys.append(self.allocator, k.*) catch {};
        for (keys.items) |eid| self.demote(table, eid);
        return keys.items.len;
    }
};
