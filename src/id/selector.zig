const std = @import("std");
const obj = @import("object.zig");
const Oop = obj.Oop;

// -- Selector interning -----------------------------------------------------
//
// Mirrors libid.c's sorted-array approach using the _selector_Table vtable
// as storage.  Binary search for fast lookup, memmove for insertion.

var selector_vtable: Oop = null;
var selector_proto: Oop = null; // the _selector prototype object
var selector_table: Oop = null; // vtable used as sorted array of selectors

pub fn init(sel_vt: Oop, sel_proto: Oop, sel_tbl: Oop) void {
    selector_vtable = sel_vt;
    selector_proto = sel_proto;
    selector_table = sel_tbl;
}

/// Create a new selector object with the given C string.
fn selectorWithCString(string: [*:0]const u8) ?*obj.Selector {
    const len = std.mem.len(string);
    const buf = obj.balloc(len + 1) orelse return null;
    @memcpy(buf[0 .. len + 1], string[0 .. len + 1]); // include NUL
    const o = obj.alloc(selector_vtable, @sizeOf(obj.Selector)) orelse return null;
    const sel = obj.asSelector(obj.toOop(o)).?;
    sel.size = len;
    sel.elements = @ptrCast(buf);
    return sel;
}

/// Intern a selector string.  Returns pointer-equal Oop for equal strings.
pub fn intern(string: [*:0]const u8) Oop {
    const tbl = obj.asVtable(selector_table) orelse return null;
    const bindings_vec = obj.asVector(tbl.bindings) orelse return null;
    const elems = bindings_vec.elements();
    const tally = tbl.tally;
    const key = std.mem.span(string);

    // Binary search
    var low: usize = 0;
    var high: usize = tally;
    while (low < high) {
        const mid = (low + high) / 2;
        const mid_sel = obj.asSelector(elems[mid]) orelse {
            low = mid + 1;
            continue;
        };
        const mid_str = std.mem.span(mid_sel.elements);
        if (std.mem.lessThan(u8, mid_str, key)) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    // Exact match?
    if (low < tally) {
        if (obj.asSelector(elems[low])) |found| {
            const found_str = std.mem.span(found.elements);
            if (std.mem.eql(u8, found_str, key)) {
                return elems[low];
            }
        }
    }

    // Need to grow?
    if (tally == bindings_vec.size) {
        const new_vec = vectorGrow(tbl.bindings) orelse return null;
        tbl.bindings = new_vec;
        // re-read elements from new vector
        return internInsert(tbl, obj.asVector(new_vec).?, low, string);
    }

    return internInsert(tbl, bindings_vec, low, string);
}

fn internInsert(tbl: *obj.Vtable, vec: *obj.Vector, pos: usize, string: [*:0]const u8) Oop {
    const elems = vec.elements();
    const tally = tbl.tally;

    // memmove to make room
    if (pos < tally) {
        var i: usize = tally;
        while (i > pos) : (i -= 1) {
            elems[i] = elems[i - 1];
        }
    }

    // Create and insert
    const sel = selectorWithCString(string) orelse return null;
    elems[pos] = obj.toOop(sel);
    tbl.tally += 1;
    return elems[pos];
}

/// Grow a vector by doubling its size (matching libid.c _vector__grow).
fn vectorGrow(vec_oop: Oop) Oop {
    const old = obj.asVector(vec_oop) orelse return null;
    const new_size = old.size * 2;
    const vector_vt = obj.vtableOf(@as(*obj.Object, @ptrCast(@alignCast(old))));
    const new_obj = obj.alloc(vector_vt, @sizeOf(obj.Vector) + @sizeOf(Oop) * new_size) orelse return null;
    const new_vec = obj.asVector(obj.toOop(new_obj)).?;
    new_vec.size = new_size;
    // Copy old elements
    const old_elems = old.elements();
    const new_elems = new_vec.elements();
    for (0..old.size) |i| {
        new_elems[i] = old_elems[i];
    }
    return obj.toOop(new_obj);
}
