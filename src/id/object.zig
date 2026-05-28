const std = @import("std");

// -- Core opaque object type ------------------------------------------------

/// Every id object is just an opaque pointer. The vtable lives at offset -1
/// (one pointer-width before the object pointer), matching libid.c's layout.
pub const Object = opaque {};
pub const Oop = ?*Object;

// -- Structured types (all extern for C ABI) --------------------------------

pub const Vtable = extern struct {
    tally: usize,
    bindings: Oop, // *Vector (as Oop)
    delegate: Oop, // *Vtable (as Oop)
};

pub const Closure = extern struct {
    method: ?*const anyopaque, // _imp_t — actual fn ptr, cast at call site
    data: Oop,
};

pub const Selector = extern struct {
    size: usize,
    elements: [*:0]const u8,
};

pub const Assoc = extern struct {
    key: Oop,
    value: Oop,
};

pub const Vector = extern struct {
    size: usize,
    // elements follow inline (accessed via pointer arithmetic)

    pub fn elements(self: *Vector) [*]Oop {
        const base: [*]u8 = @ptrCast(self);
        return @ptrCast(@alignCast(base + @sizeOf(Vector)));
    }
};

// -- The -1 vtable trick ----------------------------------------------------

/// Read the vtable pointer stored one slot before `obj`.
pub fn vtableOf(obj: *Object) Oop {
    // obj points to the first user field; vtable is at (obj - sizeof(*anyopaque))
    const raw: [*]align(@alignOf(*anyopaque)) u8 = @ptrCast(@alignCast(obj));
    const vt_slot: *Oop = @ptrCast(@alignCast(raw - @sizeOf(Oop)));
    return vt_slot.*;
}

/// Set the vtable pointer stored one slot before `obj`.
pub fn setVtable(obj: *Object, vt: Oop) void {
    const raw: [*]align(@alignOf(*anyopaque)) u8 = @ptrCast(@alignCast(obj));
    const vt_slot: *Oop = @ptrCast(@alignCast(raw - @sizeOf(Oop)));
    vt_slot.* = vt;
}

// -- Casting helpers --------------------------------------------------------

pub fn asVtable(oop: Oop) ?*Vtable {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}

pub fn asVector(oop: Oop) ?*Vector {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}

pub fn asClosure(oop: Oop) ?*Closure {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}

pub fn asSelector(oop: Oop) ?*Selector {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}

pub fn asAssoc(oop: Oop) ?*Assoc {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}

pub fn toOop(ptr: anytype) Oop {
    return @ptrCast(@alignCast(ptr));
}

// -- Allocation (no GC — plain page allocator for now) ----------------------

var gpa = std.heap.page_allocator;

/// Allocate `size` bytes of object storage preceded by a vtable slot,
/// set the vtable pointer, and return the object pointer.
pub fn alloc(vt: Oop, size: usize) ?*Object {
    const total = @sizeOf(Oop) + @max(size, 1);
    const raw = gpa.alignedAlloc(u8, .fromByteUnits(@alignOf(*anyopaque)), total) catch return null;
    @memset(raw, 0);
    // Write vtable into the first slot
    const vt_slot: *Oop = @ptrCast(@alignCast(raw.ptr));
    vt_slot.* = vt;
    // Object pointer is right after the vtable slot
    return @ptrCast(@alignCast(raw.ptr + @sizeOf(Oop)));
}

/// Allocate raw bytes (no vtable slot). Used for selector string storage.
pub fn balloc(size: usize) ?[*]u8 {
    const raw = gpa.alloc(u8, size) catch return null;
    @memset(raw, 0);
    return raw.ptr;
}
