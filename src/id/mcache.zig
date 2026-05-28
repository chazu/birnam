const obj = @import("object.zig");
const Oop = obj.Oop;

const CACHE_SIZE = 1024;

const Entry = struct {
    vtable: Oop,
    selector: Oop,
    closure: ?*obj.Closure,
};

var cache: [CACHE_SIZE]Entry = [_]Entry{.{ .vtable = null, .selector = null, .closure = null }} ** CACHE_SIZE;

fn probe(vtable: Oop, selector: Oop) usize {
    const vt_int = @intFromPtr(vtable);
    const sel_int = @intFromPtr(selector);
    return ((vt_int << 2) ^ (sel_int >> 3)) & (CACHE_SIZE - 1);
}

pub fn lookup(vtable: Oop, selector: Oop) ?*obj.Closure {
    const idx = probe(vtable, selector);
    const e = &cache[idx];
    if (e.vtable == vtable and e.selector == selector) {
        return e.closure;
    }
    return null;
}

pub fn store(vtable: Oop, selector: Oop, closure: *obj.Closure) void {
    const idx = probe(vtable, selector);
    cache[idx] = .{
        .vtable = vtable,
        .selector = selector,
        .closure = closure,
    };
}

/// Flush entries matching a specific selector (called on addMethod).
pub fn flush(selector: Oop) void {
    for (&cache) |*e| {
        if (e.selector == selector) {
            e.* = .{ .vtable = null, .selector = null, .closure = null };
        }
    }
}

/// Flush entire cache (called on hot reload).
pub fn flushAll() void {
    cache = [_]Entry{.{ .vtable = null, .selector = null, .closure = null }} ** CACHE_SIZE;
}
