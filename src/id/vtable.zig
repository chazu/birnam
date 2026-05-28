const std = @import("std");
const obj = @import("object.zig");
const mcache = @import("mcache.zig");
const send_mod = @import("send.zig");
const Oop = obj.Oop;
const Send = send_mod.Send;

// -- Direct vtable operations (used during bootstrap & by recursion guard) --

/// Find an assoc in vtable's bindings vector by key (pointer equality).
pub fn vtableFindKeyOrNil(vtable: Oop, key: Oop) Oop {
    const vt = obj.asVtable(vtable) orelse return null;
    const vec = obj.asVector(vt.bindings) orelse return null;
    const elems = vec.elements();
    var i: usize = vt.tally;
    while (i > 0) {
        i -= 1;
        const assoc = obj.asAssoc(elems[i]) orelse continue;
        if (assoc.key == key) return elems[i];
    }
    return null;
}

/// Look up a selector in a vtable, following delegation chain.
pub fn vtableLookup(vtable: Oop, selector: Oop) Oop {
    var vt = vtable;
    while (vt != null) {
        const assoc = vtableFindKeyOrNil(vt, selector);
        if (assoc != null) return assoc;
        const vt_s = obj.asVtable(vt) orelse return null;
        if (vt_s.delegate == vt) {
            // Delegation loop — bail
            return null;
        }
        vt = vt_s.delegate;
    }
    return null;
}

/// Add an object to the vtable's bindings vector.
fn vtableAdd(vtable: Oop, object: Oop) Oop {
    const vt = obj.asVtable(vtable) orelse return null;
    var vec = obj.asVector(vt.bindings) orelse return null;
    if (vt.tally == vec.size) {
        // Grow
        vt.bindings = vectorGrow(vt.bindings) orelse return null;
        vec = obj.asVector(vt.bindings) orelse return null;
    }
    const elems = vec.elements();
    elems[vt.tally] = object;
    vt.tally += 1;
    return object;
}

/// Install a method (closure) on a vtable for a given selector.
/// Direct version — no message send, used during bootstrap.
pub fn vtableMethodAtPutWith(vtable: Oop, selector: Oop, method: ?*const anyopaque, data: Oop) Oop {
    var assoc_oop = vtableFindKeyOrNil(vtable, selector);
    const vt_for_alloc = obj.vtableOf(@as(*obj.Object, @ptrCast(@alignCast(obj.asVtable(vtable).?))));
    _ = vt_for_alloc;

    if (assoc_oop == null) {
        // Create new assoc + closure
        // We need the assoc_vtable and closure_vtable — get from bootstrap globals
        const assoc_obj = obj.alloc(getAssocVt(), @sizeOf(obj.Assoc)) orelse return null;
        const assoc = obj.asAssoc(obj.toOop(assoc_obj)).?;
        assoc.key = selector;

        const closure_obj = obj.alloc(getClosureVt(), @sizeOf(obj.Closure)) orelse return null;
        const closure = obj.asClosure(obj.toOop(closure_obj)).?;
        closure.method = method;
        closure.data = data;
        assoc.value = obj.toOop(closure_obj);

        assoc_oop = obj.toOop(assoc_obj);
        _ = vtableAdd(vtable, assoc_oop);
    } else {
        const assoc = obj.asAssoc(assoc_oop).?;
        const closure = obj.asClosure(assoc.value).?;
        closure.method = method;
        closure.data = data;
    }

    mcache.flush(selector);
    return assoc_oop;
}

// -- Method implementations (callable via send) -----------------------------

/// lookup: method — signature matches _imp_t (fn(*Send, selector) Oop)
pub fn lookupMethod(s: *Send, selector: Oop) callconv(.c) Oop {
    return vtableLookup(s.receiver, selector);
}

/// methodAt:put:with: method
pub fn methodAtPutWithMethod(s: *Send, selector: Oop, method_ptr: Oop, data: Oop) callconv(.c) Oop {
    return vtableMethodAtPutWith(s.receiver, selector, @ptrCast(method_ptr), data);
}

/// allocate: method — _vtable___alloc_ equivalent
pub fn allocateMethod(s: *Send, size_oop: Oop) callconv(.c) Oop {
    const size = @intFromPtr(size_oop);
    return obj.toOop(obj.alloc(s.receiver, size));
}

/// delegated method — create a child vtable delegating to self
pub fn delegatedMethod(s: *Send) callconv(.c) Oop {
    return vtableDelegated(s.receiver);
}

/// _vtable method on objects — return vtable of self
pub fn vtableAccessor(s: *Send) callconv(.c) Oop {
    if (s.receiver) |r| return obj.vtableOf(r);
    return null;
}

/// _delegate method on objects — return null (objects don't delegate by default)
pub fn delegateAccessor(_: *Send) callconv(.c) Oop {
    return null;
}

/// _delegated method on objects — create delegated proto
pub fn objectDelegated(s: *Send) callconv(.c) Oop {
    if (s.receiver) |r| {
        const vt = obj.vtableOf(r) orelse return null;
        const child_vt = vtableDelegated(vt) orelse return null;
        return obj.toOop(obj.alloc(child_vt, 0));
    }
    return null;
}

/// init method on objects — just return self
pub fn objectInit(s: *Send) callconv(.c) Oop {
    return s.receiver;
}

// -- Vtable creation helpers ------------------------------------------------

/// Create a new child vtable delegating to `parent`.
pub fn vtableDelegated(parent: Oop) Oop {
    const parent_obj: *obj.Object = @ptrCast(@alignCast(obj.asVtable(parent) orelse return null));
    const parent_vt = obj.vtableOf(parent_obj);

    const new_obj = obj.alloc(parent_vt, @sizeOf(obj.Vtable)) orelse return null;
    const new_vt = obj.asVtable(obj.toOop(new_obj)).?;
    new_vt.tally = 0;
    new_vt.delegate = parent;

    // Allocate initial bindings vector
    const vec_obj = allocVector(4) orelse return null;
    new_vt.bindings = obj.toOop(vec_obj);

    return obj.toOop(new_obj);
}

// -- Vector helpers ---------------------------------------------------------

var vector_vtable: Oop = null;

pub fn setVectorVt(vt: Oop) void {
    vector_vtable = vt;
}

pub fn allocVector(size: usize) ?*obj.Object {
    const o = obj.alloc(vector_vtable, @sizeOf(obj.Vector) + @sizeOf(Oop) * size) orelse return null;
    const v = obj.asVector(obj.toOop(o)).?;
    v.size = size;
    return o;
}

fn vectorGrow(vec_oop: Oop) Oop {
    const old = obj.asVector(vec_oop) orelse return null;
    const new_size = if (old.size == 0) 4 else old.size * 2;
    const old_obj: *obj.Object = @ptrCast(@alignCast(old));
    const vt = obj.vtableOf(old_obj);
    const new_obj = obj.alloc(vt, @sizeOf(obj.Vector) + @sizeOf(Oop) * new_size) orelse return null;
    const new_vec = obj.asVector(obj.toOop(new_obj)).?;
    new_vec.size = new_size;
    const old_elems = old.elements();
    const new_elems = new_vec.elements();
    for (0..old.size) |i| {
        new_elems[i] = old_elems[i];
    }
    return obj.toOop(new_obj);
}

// -- Bootstrap vtable accessors (set by bootstrap) --------------------------

var assoc_vtable: Oop = null;
var closure_vtable: Oop = null;

pub fn setAssocVt(vt: Oop) void {
    assoc_vtable = vt;
}
pub fn setClosureVt(vt: Oop) void {
    closure_vtable = vt;
}
fn getAssocVt() Oop {
    return assoc_vtable;
}
fn getClosureVt() Oop {
    return closure_vtable;
}
