//! Phase 15.1 — segmented arenas: the heap-GC reclamation substrate.
//!
//! An `Arena` is a *recording* `std.mem.Allocator` wrapper. It forwards every
//! request to a backing allocator unchanged — the returned payload pointer is
//! byte-identical to what the backing allocator handed out — and records each
//! live allocation in a side table keyed by address. This buys two things the
//! raw GPA cannot:
//!
//!   1. **Enumeration.** The collector can iterate every live allocation of a
//!      given kind to find the dead ones (mark-sweep needs the full set; a GPA
//!      offers no "walk all my allocations").
//!   2. **Type-by-segment.** `contains(addr)` answers "does the strings arena
//!      own this address?" — so the raw-pointer box kinds that carry no vtable
//!      (Sig, capture arrays, constant-pool RRB, bytecode streams) self-type by
//!      *which arena holds them*, not by a header on the payload.
//!
//! Payload-preserving (vs. an intrusive GC header) is deliberate: the kernel is
//! full of existing pointer arithmetic (`obj[-1]` vtable, `base - @sizeOf(Ref)`)
//! that assumes the allocation base is the box base. A side table leaves all of
//! that untouched, so call sites migrate by swapping which allocator they thread
//! in — nothing about the returned memory changes.
//!
//! Non-moving (ROADMAP Phase 15 §Collector): allocations never relocate, so a
//! recorded address is stable for the allocation's life and a Ref to it stays
//! valid across a sweep.
//!
//! The side table itself allocates from the backing allocator directly (not
//! through this arena's vtable), so the bookkeeping is never self-recorded and
//! never shows up as a sweepable allocation.

const std = @import("std");
const Alignment = std.mem.Alignment;

/// One arena per allocation type. An address self-types by its arena (see
/// module doc): the kinds that carry no vtable on their payload rely on this.
pub const Kind = enum {
    strings, // interner dupes + string boxes (StringHeader + bytes)
    boxes, // float / ffiptr boxes (vt at -1, but kept segregated)
    bytecode, // bytecode byte streams
    captures, // closure capture arrays / BytecodeClosure
    closures_vt, // CLOSURE_VT wrappers ([vt][method][data], Ref=payload=base+8)
    sig, // FFI Sig holders
    structs, // materialized objects (also reclaimed via demote)
    index, // HAMT/RRB index nodes (epoch-freed on compact)
};

const Entry = struct {
    len: usize,
    alignment: Alignment,
    marked: bool = false,
};

pub const Arena = struct {
    backing: std.mem.Allocator,
    kind: Kind,
    live: std.AutoHashMapUnmanaged(usize, Entry) = .{},

    pub fn init(backing: std.mem.Allocator, kind: Kind) Arena {
        return .{ .backing = backing, .kind = kind };
    }

    /// Free every still-live allocation, then release the side table. After
    /// this the arena owns no heap — clean for a leak-checked backing.
    pub fn deinit(self: *Arena) void {
        var it = self.live.iterator();
        while (it.next()) |e| {
            const ptr: [*]u8 = @ptrFromInt(e.key_ptr.*);
            self.backing.rawFree(ptr[0..e.value_ptr.len], e.value_ptr.alignment, 0);
        }
        self.live.deinit(self.backing);
    }

    /// A `std.mem.Allocator` that records into this arena. Thread it into the
    /// call sites whose allocations this arena owns.
    pub fn allocator(self: *Arena) std.mem.Allocator {
        return .{ .ptr = self, .vtable = &vtable };
    }

    const vtable: std.mem.Allocator.VTable = .{
        .alloc = rawAlloc,
        .resize = rawResize,
        .remap = rawRemap,
        .free = rawFree,
    };

    fn rawAlloc(ctx: *anyopaque, len: usize, alignment: Alignment, ret_addr: usize) ?[*]u8 {
        const self: *Arena = @ptrCast(@alignCast(ctx));
        const p = self.backing.rawAlloc(len, alignment, ret_addr) orelse return null;
        self.live.put(self.backing, @intFromPtr(p), .{ .len = len, .alignment = alignment }) catch {
            self.backing.rawFree(p[0..len], alignment, ret_addr);
            return null;
        };
        return p;
    }

    fn rawResize(ctx: *anyopaque, memory: []u8, alignment: Alignment, new_len: usize, ret_addr: usize) bool {
        const self: *Arena = @ptrCast(@alignCast(ctx));
        const ok = self.backing.rawResize(memory, alignment, new_len, ret_addr);
        if (ok) {
            // In-place: address unchanged, length grows/shrinks.
            if (self.live.getPtr(@intFromPtr(memory.ptr))) |e| e.len = new_len;
        }
        return ok;
    }

    fn rawRemap(ctx: *anyopaque, memory: []u8, alignment: Alignment, new_len: usize, ret_addr: usize) ?[*]u8 {
        const self: *Arena = @ptrCast(@alignCast(ctx));
        const np = self.backing.rawRemap(memory, alignment, new_len, ret_addr) orelse return null;
        // Address may have moved: retire the old record, install the new one,
        // preserving the mark bit across the move.
        const old = self.live.fetchRemove(@intFromPtr(memory.ptr));
        const marked = if (old) |o| o.value.marked else false;
        self.live.put(self.backing, @intFromPtr(np), .{ .len = new_len, .alignment = alignment, .marked = marked }) catch {
            // Bookkeeping lost; the memory is still valid — better to leak the
            // record than corrupt. Return the pointer; deinit/sweep may miss it.
            return np;
        };
        return np;
    }

    fn rawFree(ctx: *anyopaque, memory: []u8, alignment: Alignment, ret_addr: usize) void {
        const self: *Arena = @ptrCast(@alignCast(ctx));
        _ = self.live.remove(@intFromPtr(memory.ptr));
        self.backing.rawFree(memory, alignment, ret_addr);
    }

    // -- Collector surface --------------------------------------------------

    /// Does this arena own `addr`? Used for type-by-segment discrimination.
    pub fn contains(self: *const Arena, addr: usize) bool {
        return self.live.contains(addr);
    }

    /// Mark `addr` live for the current sweep. No-op if not owned (an address
    /// from another arena / an immediate). Returns true iff this call *newly*
    /// marked an owned allocation (owned AND previously unmarked) — the
    /// collector uses that to enqueue a freshly-reached allocation exactly once.
    pub fn mark(self: *Arena, addr: usize) bool {
        if (self.live.getPtr(addr)) |e| {
            const was = e.marked;
            e.marked = true;
            return !was;
        }
        return false;
    }

    /// Recorded length of the allocation based at `addr`, if owned.
    pub fn lenOf(self: *const Arena, addr: usize) ?usize {
        if (self.live.get(addr)) |e| return e.len;
        return null;
    }

    /// Free every unmarked allocation and clear marks on survivors. Returns the
    /// count freed. This is the sweep half of mark-sweep for one segment.
    pub fn sweep(self: *Arena) usize {
        var dead = std.ArrayList(usize){};
        defer dead.deinit(self.backing);
        var it = self.live.iterator();
        while (it.next()) |e| {
            if (e.value_ptr.marked) {
                e.value_ptr.marked = false;
            } else {
                // Defer the free: can't free + clear-mark while iterating the
                // map we're about to mutate.
                dead.append(self.backing, e.key_ptr.*) catch continue;
            }
        }
        for (dead.items) |addr| {
            const e = self.live.fetchRemove(addr).?;
            const ptr: [*]u8 = @ptrFromInt(addr);
            self.backing.rawFree(ptr[0..e.value.len], e.value.alignment, 0);
        }
        return dead.items.len;
    }

    pub fn liveCount(self: *const Arena) usize {
        return self.live.count();
    }

    pub const Allocation = struct { base: usize, len: usize };

    pub const Iterator = struct {
        inner: std.AutoHashMapUnmanaged(usize, Entry).Iterator,
        pub fn next(self: *Iterator) ?Allocation {
            if (self.inner.next()) |e| return .{ .base = e.key_ptr.*, .len = e.value_ptr.len };
            return null;
        }
    };

    /// Walk every live allocation (base + len). Used by the collector to
    /// conservatively scan Ref-bearing arenas (e.g. closure captures) for
    /// pointers into another arena.
    pub fn iterate(self: *Arena) Iterator {
        return .{ .inner = self.live.iterator() };
    }
};
