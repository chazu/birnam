//! Module: dylib + its assigned EID.
//!
//! Each loaded module owns its own EID. Every fact it asserts carries
//! that EID in the `m` column. Unload = walk MEAT for this EID, retract
//! each fact. Init failure (Q2 resolution) = catch, walk, retract, drop
//! the module record.

const std = @import("std");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const id_api = @import("id_api.zig");
const send_mod = @import("send.zig");

pub const InitFn = *const fn (*id_api.ShellContext, *const id_api.IdApi, u64) callconv(.c) void;

pub const Module = struct {
    eid: u64,
    name: [:0]u8,
    lib: std.DynLib,
    init_fn: InitFn,
};

pub const Manager = struct {
    allocator: std.mem.Allocator,
    modules: std.ArrayList(Module),
    api: id_api.IdApi,
    shell_ctx: *id_api.ShellContext,

    pub fn init(
        allocator: std.mem.Allocator,
        shell_ctx: *id_api.ShellContext,
        api: id_api.IdApi,
    ) Manager {
        return .{
            .allocator = allocator,
            .modules = .{},
            .api = api,
            .shell_ctx = shell_ctx,
        };
    }

    pub fn deinit(self: *Manager) void {
        for (self.modules.items) |*m| m.lib.close();
        self.modules.deinit(self.allocator);
    }

    /// Load a module from `path`. Mints an EID, dlopen + dlsym, calls init.
    /// Returns the module's EID on success, or null on failure (after
    /// retract-and-dlclose cleanup).
    pub fn load(self: *Manager, path: [:0]const u8) !?u64 {
        const allocator = self.allocator;

        var lib = std.DynLib.open(path) catch return null;
        const init_fn = lib.lookup(InitFn, "birnam_module_init") orelse {
            lib.close();
            return null;
        };

        const table = send_mod.tablePtr() orelse {
            lib.close();
            return null;
        };
        const eid = table.alloc();

        const name_buf = try allocator.dupeZ(u8, path);

        // Call init. Module asserts its facts using `eid` as the `m` column.
        init_fn(self.shell_ctx, &self.api, eid);

        try self.modules.append(allocator, .{
            .eid = eid,
            .name = name_buf,
            .lib = lib,
            .init_fn = init_fn,
        });
        return eid;
    }

    /// Unload: retract every fact tagged with the module's EID, dlclose,
    /// drop the registry entry.
    pub fn unload(self: *Manager, module_eid: u64) !void {
        const idx = self.findIndex(module_eid) orelse return;
        try self.retractAllOf(module_eid);
        var m = self.modules.orderedRemove(idx);
        m.lib.close();
        self.allocator.free(m.name);
    }

    /// Hot reload: retract module's facts, dlclose, dlopen, init.
    pub fn reload(self: *Manager, module_eid: u64) !void {
        const idx = self.findIndex(module_eid) orelse return;
        try self.retractAllOf(module_eid);
        var m = &self.modules.items[idx];
        m.lib.close();

        var lib = std.DynLib.open(m.name) catch {
            // Failed to reopen — drop the module entry.
            const removed = self.modules.orderedRemove(idx);
            self.allocator.free(removed.name);
            return;
        };
        const init_fn = lib.lookup(InitFn, "birnam_module_init") orelse {
            lib.close();
            const removed = self.modules.orderedRemove(idx);
            self.allocator.free(removed.name);
            return;
        };

        m.lib = lib;
        m.init_fn = init_fn;
        init_fn(self.shell_ctx, &self.api, module_eid);
    }

    fn findIndex(self: *const Manager, module_eid: u64) ?usize {
        for (self.modules.items, 0..) |m, i| {
            if (m.eid == module_eid) return i;
        }
        return null;
    }

    fn retractAllOf(self: *Manager, module_eid: u64) !void {
        const store = send_mod.storePtr() orelse return;
        var it = try store.factsOfModule(module_eid, self.allocator);
        defer it.deinit();
        // Collect first; retract reorders the underlying RRB, but iterator
        // currently buffers eagerly — safe to retract during iteration.
        // Buffer anyway for clarity.
        var to_retract = std.ArrayList(fact_store.FactStore.ModuleFactIter.EAV){};
        defer to_retract.deinit(self.allocator);
        while (it.next()) |triple| try to_retract.append(self.allocator, triple);
        for (to_retract.items) |t| _ = try store.retract(t.e, t.a, t.v, module_eid);
    }
};
