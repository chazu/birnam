//! Hello — Phase 5 smoke-test module.
//!
//! Loads as `libbirnam_hello.dylib`. On init:
//!   - Creates a vtable `Hello_vt` delegating to OBJECT_VT.
//!   - Installs a `greet` method that returns a string.
//!   - Exports its Hello_vt EID as the fact
//!     `[module_eid :hello_vt mkEid(Hello_vt)]`.
//!
//! IdApi + ShellContext layout duplicated below — must stay
//! layout-identical with `src/kernel/id_api.zig`. Zig 0.15 nominal
//! types are per-module; extern struct field layout is what matters.

const std = @import("std");

const Ref = u64;
const NIL: Ref = 0b010; // mkEid(0)

const ShellContext = extern struct {
    log: *const fn ([*:0]const u8) callconv(.c) void,
    set_repl_handler: *const fn (?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void,
};

const FactCallback = fn (e: u64, a: u64, v: Ref, ctx: ?*anyopaque) callconv(.c) void;

const IdApi = extern struct {
    nil_eid: u64,
    vtable_vt_eid: u64,
    object_vt_eid: u64,
    selector_vt_eid: u64,
    closure_vt_eid: u64,
    nil_vt_eid: u64,

    sel_lookup: u64,
    sel_methodAtPutWith: u64,
    sel_delegated: u64,
    sel_init: u64,
    sel_allocate: u64,
    sel_vtable: u64,

    attr_vtable: u64,
    attr_delegate: u64,
    attr_name: u64,
    attr_module: u64,

    alloc_eid: *const fn () callconv(.c) u64,
    promote: *const fn (u64, Ref) callconv(.c) void,
    demote: *const fn (u64) callconv(.c) void,
    alloc_object: *const fn (Ref, usize) callconv(.c) ?*anyopaque,
    object_to_ref: *const fn (?*anyopaque) callconv(.c) Ref,
    make_closure: *const fn (?*const anyopaque, Ref) callconv(.c) u64,

    intern: *const fn ([*]const u8, usize) callconv(.c) u64,
    make_string: *const fn ([*]const u8, usize) callconv(.c) Ref,

    assert_fact: *const fn (u64, u64, Ref, u64) callconv(.c) u64,
    retract_fact: *const fn (u64, u64, Ref, u64) callconv(.c) u64,
    query_eavt: *const fn (u64, u64) callconv(.c) Ref,
    facts_of_module: *const fn (u64, *const FactCallback, ?*anyopaque) callconv(.c) usize,

    send0: *const fn (Ref, u64) callconv(.c) Ref,
    send1: *const fn (Ref, u64, Ref) callconv(.c) Ref,
    send2: *const fn (Ref, u64, Ref, Ref) callconv(.c) Ref,
    send3: *const fn (Ref, u64, Ref, Ref, Ref) callconv(.c) Ref,
};

const Send = extern struct {
    selector: u64,
    receiver: Ref,
    n_args: u8,
    state: Ref,
    closure: ?*anyopaque,
};

// -- Tag helpers (duplicated subset of ref.zig) -----------------------------

fn mkEid(eid: u64) Ref {
    return (eid << 3) | 0b010;
}

fn mkSmallInt(n: i64) Ref {
    return @as(u64, @bitCast(n << 3)) | 0b001;
}

fn untagSmallInt(r: Ref) i64 {
    return @as(i64, @bitCast(r)) >> 3;
}

fn isEid(r: Ref) bool {
    return (r & 0b111) == 0b010;
}

fn isSmallInt(r: Ref) bool {
    return (r & 0b111) == 0b001;
}

fn untagEid(r: Ref) u64 {
    return r >> 3;
}

// -- The greet method --------------------------------------------------------

var g_api: ?*const IdApi = null;
var g_module_eid: u64 = 0;
var g_greeting_str: Ref = NIL;
var g_attr_count_value: u64 = 0;

fn greetImpl(s: *Send) callconv(.c) Ref {
    _ = s;
    return g_greeting_str;
}

fn incImpl(s: *Send) callconv(.c) Ref {
    const api = g_api orelse return NIL;
    if (!isEid(s.receiver)) return NIL;
    const eid = untagEid(s.receiver);
    const current = api.query_eavt(eid, g_attr_count_value);
    const n: i64 = if (isSmallInt(current)) untagSmallInt(current) else 0;
    const new_val = mkSmallInt(n + 1);
    _ = api.assert_fact(eid, g_attr_count_value, new_val, g_module_eid);
    return new_val;
}

fn countImpl(s: *Send) callconv(.c) Ref {
    const api = g_api orelse return NIL;
    if (!isEid(s.receiver)) return NIL;
    const eid = untagEid(s.receiver);
    const v = api.query_eavt(eid, g_attr_count_value);
    return if (isSmallInt(v)) v else mkSmallInt(0);
}

// -- Module entry point ------------------------------------------------------

export fn birnam_module_init(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) void {
    g_api = api;
    g_module_eid = module_eid;
    ctx.log("[hello] initializing");

    // Pre-build the greeting string and stash its Ref.
    const greeting = "hello, birnam!";
    g_greeting_str = api.make_string(greeting.ptr, greeting.len);

    // Create the Hello vtable.
    const hello_vt = api.alloc_eid();
    _ = api.assert_fact(hello_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(hello_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);

    // greet method.
    const sel_greet = api.intern("greet", 5);
    const c_greet = api.make_closure(@ptrCast(&greetImpl), NIL);
    _ = api.assert_fact(hello_vt, sel_greet, mkEid(c_greet), module_eid);

    // Export Hello_vt.
    const sel_hello_vt = api.intern("hello_vt", 8);
    _ = api.assert_fact(module_eid, sel_hello_vt, mkEid(hello_vt), module_eid);

    // Singleton hello instance (so REPL can target it for `greet`).
    const hello_eid = api.alloc_eid();
    _ = api.assert_fact(hello_eid, api.attr_vtable, mkEid(hello_vt), module_eid);
    const sel_hello = api.intern("hello", 5);
    _ = api.assert_fact(module_eid, sel_hello, mkEid(hello_eid), module_eid);

    // Counter vtable + methods + instance.
    g_attr_count_value = api.intern("count_value", 11);
    const counter_vt = api.alloc_eid();
    _ = api.assert_fact(counter_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(counter_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);

    const sel_inc = api.intern("inc", 3);
    const sel_count = api.intern("count", 5);
    const c_inc = api.make_closure(@ptrCast(&incImpl), NIL);
    const c_count = api.make_closure(@ptrCast(&countImpl), NIL);
    _ = api.assert_fact(counter_vt, sel_inc, mkEid(c_inc), module_eid);
    _ = api.assert_fact(counter_vt, sel_count, mkEid(c_count), module_eid);

    // Singleton counter instance, count_value = 0.
    const counter_eid = api.alloc_eid();
    _ = api.assert_fact(counter_eid, api.attr_vtable, mkEid(counter_vt), module_eid);
    _ = api.assert_fact(counter_eid, g_attr_count_value, mkSmallInt(0), module_eid);

    const sel_counter = api.intern("counter", 7);
    _ = api.assert_fact(module_eid, sel_counter, mkEid(counter_eid), module_eid);

    // Print discoverable EIDs so the REPL user can target them.
    var buf: [128]u8 = undefined;
    const msg = std.fmt.bufPrintZ(&buf, "[hello] counter eid = {d}, hello instance eid = {d}", .{ counter_eid, hello_eid }) catch return;
    ctx.log(msg);
}
