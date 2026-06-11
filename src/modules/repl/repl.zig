//! REPL — Phase 6 module. Loaded as `libbirnam_repl.dylib`.
//!
//! Single REPL instance per dylib load. State (current target) stored as a
//! fact: `[repl_instance :target ?ref]`. Shell hands every input line to
//! the registered handler; the handler parses + dispatches.
//!
//! IdApi + ShellContext duplicated — must stay layout-compatible with
//! `src/kernel/id_api.zig`.

const std = @import("std");
const parser = @import("parser.zig");

const abi = @import("../../kernel/module_abi.zig");
const Ref = abi.Ref;
const NIL = abi.NIL;

const mkEid = abi.mkEid;

fn mkSmallInt(n: i64) Ref {
    const shifted = @as(u64, @bitCast(n << 3));
    return shifted | 0b001;
}

fn untagSmallInt(r: Ref) i64 {
    return @as(i64, @bitCast(r)) >> 3;
}

fn isNil(r: Ref) bool {
    return r == NIL;
}

fn isPointer(r: Ref) bool {
    return (r & 0b111) == 0b000;
}
fn isSmallInt(r: Ref) bool {
    return (r & 0b111) == 0b001;
}
fn isEid(r: Ref) bool {
    return (r & 0b111) == 0b010;
}
fn isString(r: Ref) bool {
    return (r & 0b111) == 0b100;
}
fn untagEid(r: Ref) u64 {
    return r >> 3;
}

// String header layout (matches strings.zig).
const StringHeader = extern struct { len: u64 };
fn stringBytes(r: Ref) []const u8 {
    const addr = r & ~@as(u64, 0b111);
    const h: *const StringHeader = @ptrFromInt(addr);
    const base: [*]const u8 = @ptrCast(h);
    return base[@sizeOf(StringHeader) .. @sizeOf(StringHeader) + h.len];
}

const ShellContext = abi.ShellContext;

const FactCallback = abi.FactCallback;

const IdApi = abi.IdApi;

// -- Module-static state ----------------------------------------------------

var g_api: ?*const IdApi = null;
var g_log: ?*const fn ([*:0]const u8) callconv(.c) void = null;
var g_module_eid: u64 = 0;
var g_repl_eid: u64 = 0;
var g_attr_target: u64 = 0;
var g_attr_prompt: u64 = 0;

// -- Output formatting ------------------------------------------------------

fn printRef(r: Ref) void {
    const log = g_log orelse return;
    var buf: [256]u8 = undefined;
    const out = formatRef(&buf, r) catch {
        log("<format error>");
        return;
    };
    log(@ptrCast(out.ptr));
}

fn formatRef(buf: []u8, r: Ref) ![:0]u8 {
    if (isNil(r)) return std.fmt.bufPrintZ(buf, "nil", .{});
    if (isSmallInt(r)) return std.fmt.bufPrintZ(buf, "{d}", .{untagSmallInt(r)});
    if (isString(r)) return std.fmt.bufPrintZ(buf, "\"{s}\"", .{stringBytes(r)});
    if (isEid(r)) return std.fmt.bufPrintZ(buf, "#<eid {d}>", .{untagEid(r)});
    if (isPointer(r)) return std.fmt.bufPrintZ(buf, "<obj 0x{x}>", .{r});
    return std.fmt.bufPrintZ(buf, "<unknown tag 0b{b}>", .{r & 0b111});
}

// -- Handler -----------------------------------------------------------------

fn handleLine(line: [*:0]const u8) callconv(.c) void {
    const api = g_api orelse return;
    const log = g_log orelse return;
    const slice = std.mem.span(line);
    const p = parser.parse(slice);

    switch (p.form) {
        .empty => return,
        .quit => {
            log("[repl] bye");
        },
        .unary => {
            const target = api.query_eavt(g_repl_eid, g_attr_target);
            if (isNil(target)) {
                log("[repl] no target set — use `target: <eid>`");
                return;
            }
            const sel = api.intern(p.selector_text.ptr, p.selector_text.len);
            const result = api.send0(target, sel);
            printRef(result);
        },
        .keyword => {
            // Special: `target: <integer>` switches the REPL's target.
            if (std.mem.eql(u8, p.selector_text, "target:")) {
                if (p.arg) |a| switch (a) {
                    .integer => |n| {
                        if (n < 0) {
                            log("[repl] target: requires non-negative integer");
                            return;
                        }
                        const new_target = mkEid(@intCast(n));
                        _ = api.assert_fact(g_repl_eid, g_attr_target, new_target, g_module_eid);
                        var buf: [128]u8 = undefined;
                        const out = std.fmt.bufPrintZ(&buf, "[repl] target = #<eid {d}>", .{n}) catch return;
                        log(@ptrCast(out.ptr));
                        return;
                    },
                    else => {
                        log("[repl] target: requires integer EID");
                        return;
                    },
                };
                log("[repl] target: needs an argument");
                return;
            }
            // Regular keyword send.
            const target = api.query_eavt(g_repl_eid, g_attr_target);
            if (isNil(target)) {
                log("[repl] no target set");
                return;
            }
            const sel = api.intern(p.selector_text.ptr, p.selector_text.len);
            const arg_ref: Ref = if (p.arg) |a| switch (a) {
                .integer => |n| mkSmallInt(n),
                .string => |s| api.make_string(s.ptr, s.len),
                .ident => |id| mkEid(api.intern(id.ptr, id.len)),
            } else NIL;
            const result = api.send1(target, sel, arg_ref);
            printRef(result);
        },
    }
}

// -- REPL methods (callable via send to the REPL instance) ------------------

const Send = extern struct {
    selector: u64,
    receiver: Ref,
    n_args: u8,
    state: Ref,
    closure: ?*anyopaque,
};

fn replSetTargetImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    const target_ref = args[0];
    const api = g_api orelse return NIL;
    _ = api.assert_fact(g_repl_eid, g_attr_target, target_ref, g_module_eid);
    return target_ref;
}

fn replStopImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = args;
    _ = n_args;
    if (g_log) |log| log("[repl] stop");
    return NIL;
}

fn replRunImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = args;
    _ = n_args;
    // Shell-driven loop; nothing to do.
    if (g_log) |log| log("[repl] run (shell-driven; no-op)");
    return NIL;
}

// -- Module entry point -----------------------------------------------------

export fn birnam_module_init(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) void {
    g_api = api;
    g_log = ctx.log;
    g_module_eid = module_eid;
    ctx.log("[repl] initializing");

    // Intern attributes the REPL stores its state under.
    g_attr_target = api.intern("target", 6);
    g_attr_prompt = api.intern("prompt", 6);

    // Build the REPL_VT and install methods.
    const repl_vt = api.alloc_eid();
    _ = api.assert_fact(repl_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(repl_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);

    const sel_run = api.intern("run", 3);
    const sel_set_target = api.intern("setTarget:", 10);
    const sel_stop = api.intern("stop", 4);

    const c_run = api.make_closure(@ptrCast(&replRunImpl), NIL);
    const c_set_target = api.make_closure(@ptrCast(&replSetTargetImpl), NIL);
    const c_stop = api.make_closure(@ptrCast(&replStopImpl), NIL);

    _ = api.assert_fact(repl_vt, sel_run, mkEid(c_run), module_eid);
    _ = api.assert_fact(repl_vt, sel_set_target, mkEid(c_set_target), module_eid);
    _ = api.assert_fact(repl_vt, sel_stop, mkEid(c_stop), module_eid);

    // Allocate the singleton REPL instance.
    g_repl_eid = api.alloc_eid();
    _ = api.assert_fact(g_repl_eid, api.attr_vtable, mkEid(repl_vt), module_eid);

    // Default prompt.
    const default_prompt = "birnam> ";
    const prompt_ref = api.make_string(default_prompt.ptr, default_prompt.len);
    _ = api.assert_fact(g_repl_eid, g_attr_prompt, prompt_ref, module_eid);

    // Export module-level reference.
    const sel_repl_obj = api.intern("repl_obj", 8);
    _ = api.assert_fact(module_eid, sel_repl_obj, mkEid(g_repl_eid), module_eid);

    const sel_repl_vt = api.intern("repl_vt", 7);
    _ = api.assert_fact(module_eid, sel_repl_vt, mkEid(repl_vt), module_eid);

    // Register the shell handler.
    ctx.set_repl_handler(&handleLine);
    ctx.log("[repl] ready");
}
