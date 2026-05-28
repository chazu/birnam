const std = @import("std");
const ast = @import("ast.zig");
const env_mod = @import("env.zig");
const IdApi = @import("api.zig").IdApi;

pub const Oop = ?*anyopaque;

// -- Tagged integer encoding ------------------------------------------------

/// SmallIntegers are encoded as tagged pointers: (value << 1) | 1
/// They are NOT id objects — no vtable at offset -1.

pub fn isSmallInt(oop: Oop) bool {
    const raw = @intFromPtr(oop orelse return false);
    return (raw & 1) == 1;
}

pub fn intToOop(value: i64) Oop {
    const shifted: u64 = @bitCast(value << 1);
    return @ptrFromInt(shifted | 1);
}

pub fn oopToInt(oop: Oop) i64 {
    const raw: u64 = @intFromPtr(oop orelse return 0);
    const signed: i64 = @bitCast(raw);
    return signed >> 1; // arithmetic shift preserves sign
}

// -- String object layout ---------------------------------------------------

/// String objects are real id objects (vtable at offset -1).
/// Fields stored inline after the vtable slot.
pub const StringObj = extern struct {
    data: ?[*]const u8,
    len: usize,
};

// -- Block object layout ----------------------------------------------------

/// Block objects are real id objects with vtable at offset -1.
/// They store a pointer to BlockData which holds AST + captured env.
pub const BlockData = struct {
    params: []const []const u8,
    body: []const *ast.Node,
    captured_env: *env_mod.Env,
};

pub const BlockObj = extern struct {
    data: ?*anyopaque, // *BlockData
};

// -- Vtables ----------------------------------------------------------------

var smallint_vt: Oop = null;
var string_vt: Oop = null;
var block_vt: Oop = null;
var true_vt: Oop = null;
var false_vt: Oop = null;
var nil_vt: Oop = null;
var api_ptr: *const IdApi = undefined;
var gpa: std.mem.Allocator = std.heap.page_allocator;

// -- Global constants -------------------------------------------------------

var nil_value: Oop = null;
var true_value: Oop = null;
var false_value: Oop = null;

pub fn getNil() Oop {
    return nil_value;
}

pub fn getTrue() Oop {
    return true_value;
}

pub fn getFalse() Oop {
    return false_value;
}

pub fn getSmallIntVt() Oop {
    return smallint_vt;
}

pub fn getStringVt() Oop {
    return string_vt;
}

pub fn getBlockVt() Oop {
    return block_vt;
}

pub fn getTrueVt() Oop {
    return true_vt;
}

pub fn getFalseVt() Oop {
    return false_vt;
}

pub fn getNilVt() Oop {
    return nil_vt;
}

// -- String helpers ---------------------------------------------------------

/// Create a new string object from a byte slice.
pub fn makeString(data: []const u8) Oop {
    const obj = api_ptr.alloc(string_vt, @sizeOf(StringObj)) orelse return null;
    const str: *StringObj = @ptrCast(@alignCast(obj));
    const buf = gpa.alloc(u8, data.len) catch return null;
    @memcpy(buf, data);
    str.data = buf.ptr;
    str.len = data.len;
    return obj;
}

/// Check if an Oop is a string object by comparing its vtable to string_vt.
pub fn isString(oop: Oop) bool {
    if (oop == null) return false;
    if (isSmallInt(oop)) return false;
    return getVt(oop) == string_vt;
}

/// Get the StringObj fields from an Oop (caller must verify isString first).
pub fn asStringObj(oop: Oop) ?*StringObj {
    if (oop == null) return null;
    return @ptrCast(@alignCast(oop.?));
}

/// Get the string bytes as a slice.
pub fn stringSlice(oop: Oop) ?[]const u8 {
    const str = asStringObj(oop) orelse return null;
    const ptr = str.data orelse return "";
    return ptr[0..str.len];
}

// -- Block helpers ----------------------------------------------------------

/// Create a new block object from BlockData.
pub fn makeBlock(data: *BlockData) Oop {
    const obj = api_ptr.alloc(block_vt, @sizeOf(BlockObj)) orelse return null;
    const blk: *BlockObj = @ptrCast(@alignCast(obj));
    blk.data = @ptrCast(data);
    return obj;
}

/// Check if an Oop is a block object.
pub fn isBlock(oop: Oop) bool {
    if (oop == null) return false;
    if (isSmallInt(oop)) return false;
    return getVt(oop) == block_vt;
}

/// Get BlockData from a block Oop.
pub fn asBlockData(oop: Oop) ?*BlockData {
    if (oop == null) return null;
    const blk: *BlockObj = @ptrCast(@alignCast(oop.?));
    if (blk.data == null) return null;
    return @ptrCast(@alignCast(blk.data.?));
}

// -- Array object layout ----------------------------------------------------

/// Array objects are real id objects with vtable at offset -1.
/// They store a pointer to elements and a length.
pub const ArrayObj = extern struct {
    elements: ?[*]Oop,
    len: usize,
};

var array_vt: Oop = null;

pub fn getArrayVt() Oop {
    return array_vt;
}

/// Create a new array object from a slice of Oops.
pub fn makeArray(elements: []const Oop) Oop {
    const obj = api_ptr.alloc(array_vt, @sizeOf(ArrayObj)) orelse return null;
    const arr: *ArrayObj = @ptrCast(@alignCast(obj));
    if (elements.len > 0) {
        const buf = gpa.alloc(Oop, elements.len) catch return null;
        @memcpy(buf, elements);
        arr.elements = buf.ptr;
    } else {
        arr.elements = null;
    }
    arr.len = elements.len;
    return obj;
}

/// Check if an Oop is an array object.
pub fn isArray(oop: Oop) bool {
    if (oop == null) return false;
    if (isSmallInt(oop)) return false;
    return getVt(oop) == array_vt;
}

/// Get ArrayObj from an array Oop.
pub fn asArrayObj(oop: Oop) ?*ArrayObj {
    if (oop == null) return null;
    return @ptrCast(@alignCast(oop.?));
}

/// Get array element at index.
pub fn arrayAt(oop: Oop, index: usize) Oop {
    const arr = asArrayObj(oop) orelse return null;
    if (index >= arr.len) return null;
    const elems = arr.elements orelse return null;
    return elems[index];
}

/// Set array element at index.
pub fn arrayAtPut(oop: Oop, index: usize, value: Oop) void {
    const arr = asArrayObj(oop) orelse return;
    if (index >= arr.len) return;
    const elems = arr.elements orelse return;
    elems[index] = value;
}

/// Get array length.
pub fn arraySize(oop: Oop) usize {
    const arr = asArrayObj(oop) orelse return 0;
    return arr.len;
}

// -- Instance helpers -------------------------------------------------------

/// Read slot at index from an id object (slots are Oop fields starting at offset 0).
pub fn getSlot(oop: Oop, index: usize) Oop {
    if (oop == null) return null;
    const base: [*]Oop = @ptrCast(@alignCast(oop.?));
    return base[index];
}

/// Write slot at index on an id object.
pub fn setSlot(oop: Oop, index: usize, value: Oop) void {
    if (oop == null) return;
    const base: [*]Oop = @ptrCast(@alignCast(oop.?));
    base[index] = value;
}

/// Allocate a new instance with given vtable and number of slots.
pub fn allocInstance(vt: Oop, n_slots: usize) Oop {
    return api_ptr.alloc(vt, n_slots * @sizeOf(Oop));
}

// -- Type registry ----------------------------------------------------------

pub const TypeInfo = struct {
    name: []const u8,
    vt: Oop,
    parent_vt: Oop,
    slots: []const []const u8,
};

var type_registry: std.StringHashMap(TypeInfo) = undefined;
var type_registry_initialized: bool = false;

pub fn initTypeRegistry(allocator: std.mem.Allocator) void {
    type_registry = std.StringHashMap(TypeInfo).init(allocator);
    type_registry_initialized = true;
}

pub fn registerType(name: []const u8, info: TypeInfo) void {
    if (!type_registry_initialized) return;
    type_registry.put(name, info) catch {};
}

pub fn lookupType(name: []const u8) ?TypeInfo {
    if (!type_registry_initialized) return null;
    return type_registry.get(name);
}

/// Look up slot index by name for a given type.
pub fn slotIndex(type_name: []const u8, slot_name: []const u8) ?usize {
    const info = lookupType(type_name) orelse return null;
    for (info.slots, 0..) |s, i| {
        if (std.mem.eql(u8, s, slot_name)) return i;
    }
    return null;
}

/// Find which type a vtable belongs to.
pub fn typeNameForVt(vt: Oop) ?[]const u8 {
    if (!type_registry_initialized) return null;
    var it = type_registry.iterator();
    while (it.next()) |entry| {
        if (entry.value_ptr.vt == vt) return entry.key_ptr.*;
    }
    return null;
}

/// Find TypeInfo by vtable.
pub fn lookupTypeByVt(vt: Oop) ?TypeInfo {
    if (!type_registry_initialized) return null;
    var it = type_registry.iterator();
    while (it.next()) |entry| {
        if (entry.value_ptr.vt == vt) return entry.value_ptr.*;
    }
    return null;
}

// -- Method data for language-defined methods -------------------------------

pub const MethodData = struct {
    selector: []const u8,
    params: []const []const u8,
    body: []const *ast.Node,
    type_name: []const u8,
};

pub const MethodDataObj = extern struct {
    data: ?*anyopaque, // *MethodData
};

/// Get the IdApi pointer (needed by evaluator for method installation).
pub fn getApi() *const IdApi {
    return api_ptr;
}

// -- Vtable helper ----------------------------------------------------------

pub fn getVt(oop: Oop) Oop {
    const raw: [*]align(@alignOf(*anyopaque)) u8 = @ptrCast(@alignCast(oop.?));
    const vt_slot: *Oop = @ptrCast(@alignCast(raw - @sizeOf(Oop)));
    return vt_slot.*;
}

/// Check if receiver is true (the singleton true object)
pub fn isTrueObj(oop: Oop) bool {
    return oop == true_value;
}

/// Check if receiver is false (the singleton false object)
pub fn isFalseObj(oop: Oop) bool {
    return oop == false_value;
}

// -- Init -------------------------------------------------------------------

pub fn initBuiltins(id_api: *const IdApi) void {
    api_ptr = id_api;

    // -- SmallInt vtable --
    smallint_vt = id_api.vtable_delegated(id_api.object_vt);
    if (smallint_vt == null) return;

    installMethodOn(smallint_vt, "+", @ptrCast(&smallintAdd));
    installMethodOn(smallint_vt, "-", @ptrCast(&smallintSub));
    installMethodOn(smallint_vt, "*", @ptrCast(&smallintMul));
    installMethodOn(smallint_vt, "//", @ptrCast(&smallintDiv));
    installMethodOn(smallint_vt, "\\\\", @ptrCast(&smallintMod));
    installMethodOn(smallint_vt, "<", @ptrCast(&smallintLt));
    installMethodOn(smallint_vt, ">", @ptrCast(&smallintGt));
    installMethodOn(smallint_vt, "=", @ptrCast(&smallintEq));
    installMethodOn(smallint_vt, "~=", @ptrCast(&smallintNeq));
    installMethodOn(smallint_vt, "printString", @ptrCast(&smallintPrintString));
    installMethodOn(smallint_vt, "println", @ptrCast(&smallintPrintln));

    // -- String vtable --
    string_vt = id_api.vtable_delegated(id_api.object_vt);
    if (string_vt == null) return;

    installMethodOn(string_vt, ",", @ptrCast(&stringConcat));
    installMethodOn(string_vt, "size", @ptrCast(&stringSize));
    installMethodOn(string_vt, "at:", @ptrCast(&stringAt));
    installMethodOn(string_vt, "println", @ptrCast(&stringPrintln));
    installMethodOn(string_vt, "print", @ptrCast(&stringPrint));
    installMethodOn(string_vt, "=", @ptrCast(&stringEq));
    installMethodOn(string_vt, "~=", @ptrCast(&stringNeq));

    // -- Block vtable --
    block_vt = id_api.vtable_delegated(id_api.object_vt);
    if (block_vt == null) return;
    // Block sends (value, value:, etc.) are handled in the evaluator fast path,
    // not via id dispatch. No methods installed here for now.

    // -- Array vtable --
    array_vt = id_api.vtable_delegated(id_api.object_vt);
    if (array_vt == null) return;
    // Array sends are handled in the evaluator fast path.

    // -- Boolean vtables --
    true_vt = id_api.vtable_delegated(id_api.object_vt);
    false_vt = id_api.vtable_delegated(id_api.object_vt);
    nil_vt = id_api.vtable_delegated(id_api.object_vt);

    // -- Global constants --
    nil_value = null; // nil is just null pointer
    true_value = if (true_vt != null) id_api.alloc(true_vt, 0) else null;
    false_value = if (false_vt != null) id_api.alloc(false_vt, 0) else null;
}

fn installMethodOn(vt: Oop, name: [*:0]const u8, method: ?*const anyopaque) void {
    const sel = api_ptr.intern(name);
    if (sel == null) return;
    _ = api_ptr.vtable_method_at_put_with(vt, sel, method, null);
}

// -- Trampoline structure matching Send -------------------------------------

const Send = extern struct {
    selector: Oop,
    n_args: c_int,
    receiver: Oop,
    state: Oop,
    closure: Oop,
};

// -- SmallInt trampolines ---------------------------------------------------

fn smallintAdd(s: *Send, arg: Oop) callconv(.c) Oop {
    return intToOop(oopToInt(s.receiver) + oopToInt(arg));
}

fn smallintSub(s: *Send, arg: Oop) callconv(.c) Oop {
    return intToOop(oopToInt(s.receiver) - oopToInt(arg));
}

fn smallintMul(s: *Send, arg: Oop) callconv(.c) Oop {
    return intToOop(oopToInt(s.receiver) * oopToInt(arg));
}

fn smallintDiv(s: *Send, arg: Oop) callconv(.c) Oop {
    const b = oopToInt(arg);
    if (b == 0) return intToOop(0);
    return intToOop(@divTrunc(oopToInt(s.receiver), b));
}

fn smallintMod(s: *Send, arg: Oop) callconv(.c) Oop {
    const b = oopToInt(arg);
    if (b == 0) return intToOop(0);
    return intToOop(@rem(oopToInt(s.receiver), b));
}

fn smallintLt(s: *Send, arg: Oop) callconv(.c) Oop {
    return intToOop(if (oopToInt(s.receiver) < oopToInt(arg)) @as(i64, 1) else @as(i64, 0));
}

fn smallintGt(s: *Send, arg: Oop) callconv(.c) Oop {
    return intToOop(if (oopToInt(s.receiver) > oopToInt(arg)) @as(i64, 1) else @as(i64, 0));
}

fn smallintEq(s: *Send, arg: Oop) callconv(.c) Oop {
    return intToOop(if (oopToInt(s.receiver) == oopToInt(arg)) @as(i64, 1) else @as(i64, 0));
}

fn smallintNeq(s: *Send, arg: Oop) callconv(.c) Oop {
    return intToOop(if (oopToInt(s.receiver) != oopToInt(arg)) @as(i64, 1) else @as(i64, 0));
}

fn smallintPrintString(s: *Send) callconv(.c) Oop {
    var buf: [32]u8 = undefined;
    const val = oopToInt(s.receiver);
    const formatted = std.fmt.bufPrint(&buf, "{d}", .{val}) catch return null;
    return makeString(formatted);
}

fn smallintPrintln(s: *Send) callconv(.c) Oop {
    const val = oopToInt(s.receiver);
    const w = std.fs.File.stdout().deprecatedWriter();
    w.print("{d}\n", .{val}) catch {};
    return s.receiver;
}

// -- String trampolines -----------------------------------------------------

fn stringConcat(s: *Send, arg: Oop) callconv(.c) Oop {
    const a_str = asStringObj(s.receiver) orelse return null;
    const b_str = asStringObj(arg) orelse return null;

    const a_data = if (a_str.data) |d| d[0..a_str.len] else "";
    const b_data = if (b_str.data) |d| d[0..b_str.len] else "";

    const new_len = a_data.len + b_data.len;
    const buf = gpa.alloc(u8, new_len) catch return null;
    @memcpy(buf[0..a_data.len], a_data);
    @memcpy(buf[a_data.len..], b_data);

    const obj = api_ptr.alloc(string_vt, @sizeOf(StringObj)) orelse return null;
    const result: *StringObj = @ptrCast(@alignCast(obj));
    result.data = buf.ptr;
    result.len = new_len;
    return obj;
}

fn stringSize(s: *Send) callconv(.c) Oop {
    const str = asStringObj(s.receiver) orelse return intToOop(0);
    return intToOop(@as(i64, @intCast(str.len)));
}

fn stringAt(s: *Send, index_oop: Oop) callconv(.c) Oop {
    const str = asStringObj(s.receiver) orelse return null;
    if (!isSmallInt(index_oop)) return null;
    const idx = oopToInt(index_oop);
    if (idx < 0) return null;
    const i: usize = @intCast(idx);
    const data = if (str.data) |d| d[0..str.len] else return null;
    if (i >= data.len) return null;
    return intToOop(@as(i64, @intCast(data[i])));
}

fn stringPrintln(s: *Send) callconv(.c) Oop {
    const str = asStringObj(s.receiver) orelse return s.receiver;
    const data = if (str.data) |d| d[0..str.len] else "";
    const w = std.fs.File.stdout().deprecatedWriter();
    w.print("{s}\n", .{data}) catch {};
    return s.receiver;
}

fn stringPrint(s: *Send) callconv(.c) Oop {
    const str = asStringObj(s.receiver) orelse return s.receiver;
    const data = if (str.data) |d| d[0..str.len] else "";
    const w = std.fs.File.stdout().deprecatedWriter();
    w.print("{s}", .{data}) catch {};
    return s.receiver;
}

fn stringEq(s: *Send, arg: Oop) callconv(.c) Oop {
    const a = stringSlice(s.receiver) orelse return intToOop(0);
    const b = stringSlice(arg) orelse return intToOop(0);
    return intToOop(if (std.mem.eql(u8, a, b)) @as(i64, 1) else @as(i64, 0));
}

fn stringNeq(s: *Send, arg: Oop) callconv(.c) Oop {
    const a = stringSlice(s.receiver) orelse return intToOop(1);
    const b = stringSlice(arg) orelse return intToOop(1);
    return intToOop(if (!std.mem.eql(u8, a, b)) @as(i64, 1) else @as(i64, 0));
}
