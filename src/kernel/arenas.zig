//! Phase 15.1b — the global arena registry.
//!
//! Holds one `Arena` per allocation kind (ROADMAP Phase 15 §15.1), created
//! once at process start from the kernel's backing allocator. The collector
//! (15.2/15.3) and the `:gc` command (15.6) reach the arenas through here.
//!
//! **Lifecycle boundary (deliberate):** the registry is initialized by the
//! *live* entry points (`shell.main` / the AOT driver), NOT by
//! `bootstrap.boot`. Many unit tests call `boot` with `std.testing.allocator`
//! and share one process; if `boot` populated a global arena, each test's
//! allocations would entangle the next test's leak check. Box constructors
//! therefore keep their `allocator` parameter (tests pass `testing.allocator`
//! and free locally); only the live init points thread an arena allocator in.
//! A test that wants arena behavior calls `arenas.init` / `arenas.deinit`
//! explicitly and owns the pairing.

const std = @import("std");
const arena_mod = @import("arena.zig");

const Arena = arena_mod.Arena;
const Kind = arena_mod.Kind;

pub const Registry = struct {
    strings: Arena,
    boxes: Arena,
    bytecode: Arena,
    captures: Arena,
    sig: Arena,
    structs: Arena,
    index: Arena,

    fn make(backing: std.mem.Allocator) Registry {
        return .{
            .strings = Arena.init(backing, .strings),
            .boxes = Arena.init(backing, .boxes),
            .bytecode = Arena.init(backing, .bytecode),
            .captures = Arena.init(backing, .captures),
            .sig = Arena.init(backing, .sig),
            .structs = Arena.init(backing, .structs),
            .index = Arena.init(backing, .index),
        };
    }

    fn deinitAll(self: *Registry) void {
        self.strings.deinit();
        self.boxes.deinit();
        self.bytecode.deinit();
        self.captures.deinit();
        self.sig.deinit();
        self.structs.deinit();
        self.index.deinit();
    }
};

var g_backing: ?std.mem.Allocator = null;
var g: ?*Registry = null;

/// Create the registry from `backing`. Idempotent-safe: a second call tears
/// the prior registry down first (so a re-`init` in a long-lived test process
/// never silently leaks the old one). Call once per live process at startup.
pub fn init(backing: std.mem.Allocator) !void {
    if (g) |old| {
        old.deinitAll();
        backing.destroy(old);
    }
    const reg = try backing.create(Registry);
    reg.* = Registry.make(backing);
    g = reg;
    g_backing = backing;
}

/// Free every arena (and its still-live allocations) and release the registry.
/// The live binary never calls this (process exit reclaims); tests that called
/// `init` call this to stay leak-clean.
pub fn deinit() void {
    const reg = g orelse return;
    reg.deinitAll();
    (g_backing.?).destroy(reg);
    g = null;
    g_backing = null;
}

/// True once `init` has run. Call sites guard on this so the same code path
/// works booted-live (arena) or in a bare unit test (no registry).
pub fn ready() bool {
    return g != null;
}

fn arenaPtr(comptime field: []const u8) *Arena {
    return &@field(g.?, field);
}

// -- Per-kind allocator accessors (assert the registry is initialized) ------

pub fn strings() std.mem.Allocator {
    return arenaPtr("strings").allocator();
}
pub fn boxes() std.mem.Allocator {
    return arenaPtr("boxes").allocator();
}
pub fn bytecode() std.mem.Allocator {
    return arenaPtr("bytecode").allocator();
}
pub fn captures() std.mem.Allocator {
    return arenaPtr("captures").allocator();
}
pub fn sig() std.mem.Allocator {
    return arenaPtr("sig").allocator();
}
pub fn structs() std.mem.Allocator {
    return arenaPtr("structs").allocator();
}
pub fn index() std.mem.Allocator {
    return arenaPtr("index").allocator();
}

// -- Collector handles ------------------------------------------------------

/// The arena for `kind`, for mark/sweep/contains. Null if not initialized.
pub fn arena(kind: Kind) ?*Arena {
    const reg = g orelse return null;
    return switch (kind) {
        .strings => &reg.strings,
        .boxes => &reg.boxes,
        .bytecode => &reg.bytecode,
        .captures => &reg.captures,
        .sig => &reg.sig,
        .structs => &reg.structs,
        .index => &reg.index,
    };
}
