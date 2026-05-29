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

// -- The greet method --------------------------------------------------------

var g_api: ?*const IdApi = null;
var g_greeting_str: Ref = NIL;

fn greetImpl(s: *Send) callconv(.c) Ref {
    _ = s;
    return g_greeting_str;
}

// -- Module entry point ------------------------------------------------------

export fn birnam_module_init(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) void {
    g_api = api;
    ctx.log("[hello] initializing");

    // Pre-build the greeting string and stash its Ref.
    const greeting = "hello, birnam!";
    g_greeting_str = api.make_string(greeting.ptr, greeting.len);

    // Create the Hello vtable. We mint an EID for it, then assert its
    // :vtable and :delegate facts so bind's cold path can dispatch.
    const hello_vt = api.alloc_eid();
    _ = api.assert_fact(hello_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(hello_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);

    // Intern the `greet` selector and install the method.
    const sel_greet = api.intern("greet", 5);
    const closure_eid = api.make_closure(@ptrCast(&greetImpl), NIL);
    _ = api.assert_fact(hello_vt, sel_greet, mkEid(closure_eid), module_eid);

    // Export ourselves as a fact: `[module :hello_vt hello_vt_ref]`.
    const sel_hello_vt = api.intern("hello_vt", 8);
    _ = api.assert_fact(module_eid, sel_hello_vt, mkEid(hello_vt), module_eid);
}
